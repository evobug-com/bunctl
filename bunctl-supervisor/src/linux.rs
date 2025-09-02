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
use tokio::sync::mpsc;

use crate::common::ProcessRegistry;

pub struct LinuxSupervisor {
    registry: Arc<ProcessRegistry>,
    event_tx: mpsc::Sender<SupervisorEvent>,
    event_rx: parking_lot::Mutex<Option<mpsc::Receiver<SupervisorEvent>>>,
    cgroup_root: PathBuf,
    cgroups: dashmap::DashMap<AppId, PathBuf>,
}

impl LinuxSupervisor {
    pub async fn new() -> Result<Self> {
        let (event_tx, event_rx) = mpsc::channel(1024);
        let cgroup_root = Self::find_cgroup_root()?;

        Ok(Self {
            registry: Arc::new(ProcessRegistry::new()),
            event_tx,
            event_rx: parking_lot::Mutex::new(Some(event_rx)),
            cgroup_root,
            cgroups: dashmap::DashMap::new(),
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
        let cgroup_path = self.cgroup_root.join("bunctl").join(app_id.as_str());

        tokio::fs::create_dir_all(&cgroup_path).await?;

        let subtree_control = cgroup_path.parent().unwrap().join("cgroup.subtree_control");
        if subtree_control.exists() {
            let _ = tokio::fs::write(&subtree_control, b"+cpu +memory +pids").await;
        }

        self.cgroups.insert(app_id.clone(), cgroup_path.clone());
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
            let quota = (cpu_percent * 1000.0) as u32;
            tokio::fs::write(&cpu_max, format!("{} 100000", quota)).await?;
        }

        Ok(())
    }

    async fn kill_cgroup(&self, cgroup_path: &Path) -> Result<()> {
        let procs_file = cgroup_path.join("cgroup.procs");
        if let Ok(content) = tokio::fs::read_to_string(&procs_file).await {
            for line in content.lines() {
                if let Ok(pid) = line.parse::<i32>() {
                    let _ = signal::kill(Pid::from_raw(pid), Signal::SIGKILL);
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;

        let _ = tokio::fs::remove_dir(cgroup_path).await;
        Ok(())
    }

    async fn spawn_with_cgroup(&self, config: &AppConfig) -> Result<ProcessHandle> {
        let app_id = AppId::new(&config.name)?;
        let cgroup_path = self.create_cgroup(&app_id).await?;

        self.set_cgroup_limits(&cgroup_path, config).await?;

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

        self.add_to_cgroup(&cgroup_path, pid).await?;

        let handle = ProcessHandle::new(pid, app_id.clone(), child);
        self.registry.register(
            app_id.clone(),
            ProcessHandle {
                pid,
                app_id: app_id.clone(),
                inner: None,
            },
        );

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
}

#[async_trait]
impl ProcessSupervisor for LinuxSupervisor {
    async fn spawn(&self, config: &AppConfig) -> Result<ProcessHandle> {
        self.spawn_with_cgroup(config).await
    }

    async fn kill_tree(&self, handle: &ProcessHandle) -> Result<()> {
        if let Some(cgroup_path) = self.cgroups.get(&handle.app_id) {
            self.kill_cgroup(&cgroup_path).await?;
            self.cgroups.remove(&handle.app_id);
        } else {
            signal::kill(Pid::from_raw(handle.pid as i32), Signal::SIGKILL)?;
        }

        self.registry.unregister(&handle.app_id);
        Ok(())
    }

    async fn wait(&self, handle: &mut ProcessHandle) -> Result<ExitStatus> {
        handle.wait().await
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
        if let Some(cgroup_path) = self.cgroups.get(&handle.app_id) {
            self.set_cgroup_limits(&cgroup_path, config).await?;
        }
        Ok(())
    }

    fn events(&self) -> mpsc::Receiver<SupervisorEvent> {
        self.event_rx
            .lock()
            .take()
            .expect("Events receiver already taken")
    }
}
