use async_trait::async_trait;
use bunctl_core::{
    AppConfig, AppId, Error, ExitStatus, ProcessHandle, ProcessInfo, ProcessSupervisor, Result,
    SupervisorEvent,
};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, mpsc};
use tracing::{debug, warn};

use crate::common::ProcessRegistry;

pub struct LinuxSupervisor {
    registry: Arc<ProcessRegistry>,
    event_tx: mpsc::Sender<SupervisorEvent>,
    event_rx: parking_lot::Mutex<Option<mpsc::Receiver<SupervisorEvent>>>,
    cgroup_root: Option<PathBuf>,
    cgroups: Arc<RwLock<HashMap<AppId, PathBuf>>>,
    use_cgroups: bool,
}

impl LinuxSupervisor {
    pub async fn new() -> Result<Self> {
        let (event_tx, event_rx) = mpsc::channel(1024);

        // Try to detect cgroups, but don't fail if unavailable
        let (cgroup_root, use_cgroups) = match Self::find_cgroup_root() {
            Ok(root) => {
                // Check if we have permission to use cgroups
                let test_path = root.join("bunctl");
                match std::fs::create_dir(&test_path) {
                    Ok(_) => {
                        let _ = std::fs::remove_dir(&test_path);
                        (Some(root), true)
                    }
                    Err(_) => {
                        debug!("No permission to create cgroups, running without cgroup support");
                        (None, false)
                    }
                }
            }
            Err(_) => {
                debug!("cgroups v2 not available, running without cgroup support");
                (None, false)
            }
        };

        if use_cgroups {
            debug!("Linux supervisor initialized with cgroups v2 support");
        } else {
            debug!("Linux supervisor initialized without cgroups support");
        }

        Ok(Self {
            registry: Arc::new(ProcessRegistry::new()),
            event_tx,
            event_rx: parking_lot::Mutex::new(Some(event_rx)),
            cgroup_root,
            cgroups: Arc::new(RwLock::new(HashMap::new())),
            use_cgroups,
        })
    }

    fn find_cgroup_root() -> Result<PathBuf> {
        let v2_path = Path::new("/sys/fs/cgroup");
        if v2_path.join("cgroup.controllers").exists() {
            return Ok(v2_path.to_path_buf());
        }

        Err(Error::Supervisor("cgroups v2 not found".to_string()))
    }

    async fn create_cgroup(&self, app_id: &AppId) -> Result<PathBuf> {
        let cgroup_root = self
            .cgroup_root
            .as_ref()
            .ok_or_else(|| Error::Supervisor("cgroups not available".to_string()))?;

        let cgroup_path = cgroup_root.join("bunctl").join(app_id.as_str());

        // Create cgroup directory
        tokio::fs::create_dir_all(&cgroup_path).await?;

        // Enable controllers in parent
        let subtree_control = cgroup_path.parent().unwrap().join("cgroup.subtree_control");
        if subtree_control.exists() {
            let _ = tokio::fs::write(&subtree_control, b"+cpu +memory +pids").await;
        }

        self.cgroups
            .write()
            .await
            .insert(app_id.clone(), cgroup_path.clone());
        Ok(cgroup_path)
    }

    async fn add_to_cgroup(&self, cgroup_path: &Path, pid: u32) -> Result<()> {
        let procs_file = cgroup_path.join("cgroup.procs");
        tokio::fs::write(&procs_file, pid.to_string()).await?;
        Ok(())
    }

    async fn set_cgroup_limits(&self, cgroup_path: &Path, config: &AppConfig) -> Result<()> {
        if let Some(max_memory) = config.max_memory {
            let memory_max = cgroup_path.join("memory.max");
            tokio::fs::write(&memory_max, max_memory.to_string()).await?;
        }

        if let Some(cpu_percent) = config.max_cpu_percent {
            let cpu_max = cgroup_path.join("cpu.max");
            // Clamp CPU percentage to valid range (0.1% to 10000%) and convert safely
            let clamped_percent = cpu_percent.clamp(0.1, 10000.0);
            let quota = (clamped_percent * 1000.0).round() as u32;
            // Ensure quota is at least 100 (0.1% of CPU) and at most 10000000 (10000%)
            let quota = quota.clamp(100, 10000000);
            tokio::fs::write(&cpu_max, format!("{} 100000", quota)).await?;
        }

        Ok(())
    }

    async fn kill_cgroup(&self, cgroup_path: &Path) -> Result<()> {
        // First try to kill all processes in the cgroup
        let procs_file = cgroup_path.join("cgroup.procs");
        if let Ok(content) = tokio::fs::read_to_string(&procs_file).await {
            for line in content.lines() {
                if let Ok(pid) = line.parse::<i32>() {
                    let _ = signal::kill(Pid::from_raw(pid), Signal::SIGKILL);
                }
            }
        }

        // Wait a bit for processes to die
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Try to remove the cgroup directory
        let _ = tokio::fs::remove_dir(cgroup_path).await;
        Ok(())
    }

    async fn cleanup_cgroup(&self, app_id: &AppId) {
        if let Some(cgroup_path) = self.cgroups.write().await.remove(app_id) {
            let _ = self.kill_cgroup(&cgroup_path).await;
        }
    }

    async fn spawn_simple(&self, config: &AppConfig) -> Result<ProcessHandle> {
        let app_id = AppId::new(&config.name)?;

        let mut builder = bunctl_core::process::ProcessBuilder::new(&config.command);
        builder = builder
            .args(&config.args)
            .current_dir(&config.cwd)
            .envs(&config.env);

        if let Some(uid) = config.uid {
            builder = builder.uid(uid);
        }
        if let Some(gid) = config.gid {
            builder = builder.gid(gid);
        }

        let child = builder.spawn().await?;
        let pid = child.id().unwrap();

        let handle = ProcessHandle::new(pid, app_id.clone(), child);
        self.registry.register(app_id.clone(), handle.clone());

        let _ = self
            .event_tx
            .send(SupervisorEvent::ProcessStarted { app: app_id, pid })
            .await;

        Ok(handle)
    }

    async fn spawn_with_cgroup(&self, config: &AppConfig) -> Result<ProcessHandle> {
        let app_id = AppId::new(&config.name)?;

        // Create cgroup BEFORE spawning process
        let cgroup_path = self.create_cgroup(&app_id).await?;

        // Set limits before adding process
        self.set_cgroup_limits(&cgroup_path, config).await?;

        // Use a pre-exec function to add the process to the cgroup atomically
        // This is done by writing to cgroup.procs before exec
        let _cgroup_procs = cgroup_path.join("cgroup.procs");

        let mut builder = bunctl_core::process::ProcessBuilder::new(&config.command);
        builder = builder
            .args(&config.args)
            .current_dir(&config.cwd)
            .envs(&config.env);

        if let Some(uid) = config.uid {
            builder = builder.uid(uid);
        }
        if let Some(gid) = config.gid {
            builder = builder.gid(gid);
        }

        // Spawn the process
        let child = builder.spawn().await?;
        let pid = child.id().unwrap();

        // Add to cgroup immediately after spawn
        // There's still a small race window here, but it's minimized
        if let Err(e) = self.add_to_cgroup(&cgroup_path, pid).await {
            warn!("Failed to add process {} to cgroup: {}", pid, e);
            // Don't fail the spawn, just run without cgroup limits
        }

        let handle = ProcessHandle::new(pid, app_id.clone(), child);
        self.registry.register(app_id.clone(), handle.clone());

        let _ = self
            .event_tx
            .send(SupervisorEvent::ProcessStarted { app: app_id, pid })
            .await;

        Ok(handle)
    }

    fn read_proc_stat(&self, pid: u32) -> Result<HashMap<String, u64>> {
        let stat_path = format!("/proc/{}/stat", pid);
        let content =
            fs::read_to_string(&stat_path).map_err(|_| Error::ProcessNotFound(pid.to_string()))?;

        let fields: Vec<&str> = content.split_whitespace().collect();
        if fields.len() < 24 {
            return Err(Error::Supervisor(
                "Invalid /proc/pid/stat format".to_string(),
            ));
        }

        let mut stats = HashMap::new();
        stats.insert("utime".to_string(), fields[13].parse().unwrap_or(0));
        stats.insert("stime".to_string(), fields[14].parse().unwrap_or(0));
        stats.insert("num_threads".to_string(), fields[19].parse().unwrap_or(0));
        stats.insert("vsize".to_string(), fields[22].parse().unwrap_or(0));
        stats.insert("rss".to_string(), fields[23].parse().unwrap_or(0));

        Ok(stats)
    }

    async fn signal_process(&self, pid: u32, sig: Signal) -> Result<()> {
        // Safely convert u32 PID to i32, checking for overflow
        let pid_i32 = i32::try_from(pid)
            .map_err(|_| Error::Supervisor(format!("PID {} too large for system", pid)))?;

        signal::kill(Pid::from_raw(pid_i32), sig)
            .map_err(|e| Error::Supervisor(format!("Failed to send signal: {}", e)))
    }

    async fn get_cgroup_pids(&self, app_id: &AppId) -> Vec<u32> {
        if let Some(cgroup_path) = self.cgroups.read().await.get(app_id) {
            let procs_file = cgroup_path.join("cgroup.procs");
            if let Ok(content) = tokio::fs::read_to_string(&procs_file).await {
                return content
                    .lines()
                    .filter_map(|line| line.parse::<u32>().ok())
                    .collect();
            }
        }
        Vec::new()
    }
}

#[async_trait]
impl ProcessSupervisor for LinuxSupervisor {
    async fn spawn(&self, config: &AppConfig) -> Result<ProcessHandle> {
        if self.use_cgroups {
            match self.spawn_with_cgroup(config).await {
                Ok(handle) => Ok(handle),
                Err(e) => {
                    warn!(
                        "Failed to spawn with cgroup: {}, falling back to simple spawn",
                        e
                    );
                    self.spawn_simple(config).await
                }
            }
        } else {
            self.spawn_simple(config).await
        }
    }

    async fn kill_tree(&self, handle: &ProcessHandle) -> Result<()> {
        debug!("Killing process tree for app: {}", handle.app_id);

        if let Some(cgroup_path) = self.cgroups.read().await.get(&handle.app_id) {
            // Use cgroup to kill all processes
            self.kill_cgroup(cgroup_path).await?;
            self.cgroups.write().await.remove(&handle.app_id);
        } else {
            // Fallback to killing process group
            // Try to kill process group first
            // Safely convert u32 PID to i32 and negate for process group
            match i32::try_from(handle.pid) {
                Ok(pid_i32) => {
                    // Check if negation would overflow (unlikely but possible for i32::MAX)
                    let pgid = pid_i32.checked_neg().map(Pid::from_raw).ok_or_else(|| {
                        Error::Supervisor(format!("PID {} too large for process group", handle.pid))
                    })?;

                    if signal::kill(pgid, Signal::SIGKILL).is_err() {
                        // If process group kill fails, kill individual process
                        signal::kill(Pid::from_raw(pid_i32), Signal::SIGKILL)?;
                    }
                }
                Err(_) => {
                    // PID too large, just try to kill the individual process
                    // This is extremely unlikely on real systems
                    return Err(Error::Supervisor(format!(
                        "PID {} too large for system",
                        handle.pid
                    )));
                }
            }
        }

        self.registry.unregister(&handle.app_id);
        Ok(())
    }

    async fn graceful_stop(
        &self,
        handle: &mut ProcessHandle,
        timeout: Duration,
    ) -> Result<ExitStatus> {
        debug!(
            "Attempting graceful stop for app: {} with timeout: {:?}",
            handle.app_id, timeout
        );

        let start = std::time::Instant::now();

        // First send SIGTERM to all processes
        if self.use_cgroups {
            let pids = self.get_cgroup_pids(&handle.app_id).await;
            for pid in pids {
                let _ = self.signal_process(pid, Signal::SIGTERM).await;
            }
        } else {
            // Try process group first
            let pgid = Pid::from_raw(-(handle.pid as i32));
            if signal::kill(pgid, Signal::SIGTERM).is_err() {
                // Fallback to individual process
                self.signal_process(handle.pid, Signal::SIGTERM).await?;
            }
        }

        // Wait for process to exit gracefully
        let remaining_time = timeout.saturating_sub(start.elapsed());
        match tokio::time::timeout(remaining_time, handle.wait()).await {
            Ok(Ok(status)) => {
                debug!("Process exited gracefully");
                self.cleanup_cgroup(&handle.app_id).await;
                self.registry.unregister(&handle.app_id);
                Ok(status)
            }
            Ok(Err(e)) => {
                // Error waiting for process
                Err(e)
            }
            Err(_) => {
                // Timeout elapsed, send SIGKILL
                debug!("Graceful stop timeout exceeded, sending SIGKILL");
                self.kill_tree(handle).await?;
                handle.wait().await
            }
        }
    }

    async fn wait(&self, handle: &mut ProcessHandle) -> Result<ExitStatus> {
        let status = handle.wait().await?;

        // Clean up cgroup after process exits
        self.cleanup_cgroup(&handle.app_id).await;
        self.registry.unregister(&handle.app_id);

        Ok(status)
    }

    async fn get_process_info(&self, pid: u32) -> Result<ProcessInfo> {
        let stats = self.read_proc_stat(pid)?;
        let page_size =
            nix::unistd::sysconf(nix::unistd::SysconfVar::PAGE_SIZE)?.unwrap_or(4096) as u64;

        let cmdline_path = format!("/proc/{}/cmdline", pid);
        let cmdline = fs::read_to_string(&cmdline_path)
            .unwrap_or_default()
            .replace('\0', " ");

        let parts: Vec<&str> = cmdline.split_whitespace().collect();
        let command = parts.first().unwrap_or(&"").to_string();
        let args = parts.iter().skip(1).map(|s| s.to_string()).collect();

        let fd_dir = format!("/proc/{}/fd", pid);
        let open_files = fs::read_dir(&fd_dir)
            .map(|entries| entries.count() as u32)
            .ok();

        Ok(ProcessInfo {
            pid,
            name: command.clone(),
            command,
            args,
            memory_bytes: stats.get("rss").map(|&rss| rss * page_size),
            cpu_percent: None,
            threads: stats.get("num_threads").map(|&t| t as u32),
            open_files,
        })
    }

    async fn set_resource_limits(&self, handle: &ProcessHandle, config: &AppConfig) -> Result<()> {
        if self.use_cgroups
            && let Some(cgroup_path) = self.cgroups.read().await.get(&handle.app_id)
        {
            self.set_cgroup_limits(cgroup_path, config).await?;
        }
        // When cgroups are not available, resource limits cannot be set dynamically
        Ok(())
    }

    fn events(&self) -> mpsc::Receiver<SupervisorEvent> {
        // Take the receiver if available, otherwise return a dummy channel
        // that immediately closes (indicating the receiver was already taken)
        self.event_rx.lock().take().unwrap_or_else(|| {
            // Create a closed channel to indicate the receiver was already taken
            // This avoids panicking and allows the caller to handle the situation
            let (_, rx) = mpsc::channel(1);
            rx
        })
    }
}

impl Drop for LinuxSupervisor {
    fn drop(&mut self) {
        // Best effort cleanup - we can't use async operations in Drop
        // Processes should be cleaned up through normal shutdown

        // Clear registry
        self.registry.clear();
    }
}
