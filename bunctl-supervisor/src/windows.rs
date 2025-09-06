use async_trait::async_trait;
use bunctl_core::{
    AppConfig, AppId, ExitStatus, ProcessHandle, ProcessInfo, ProcessSupervisor, Result,
    SupervisorEvent,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, trace, warn};

use crate::common::ProcessRegistry;

pub struct WindowsSupervisor {
    registry: Arc<ProcessRegistry>,
    event_tx: mpsc::Sender<SupervisorEvent>,
    event_rx: parking_lot::Mutex<Option<mpsc::Receiver<SupervisorEvent>>>,
}

impl WindowsSupervisor {
    pub async fn new() -> Result<Self> {
        debug!("Initializing Windows supervisor");
        let (event_tx, event_rx) = mpsc::channel(1024);
        debug!("Created supervisor event channel with capacity 1024");

        Ok(Self {
            registry: Arc::new(ProcessRegistry::new()),
            event_tx,
            event_rx: parking_lot::Mutex::new(Some(event_rx)),
        })
    }

    async fn spawn_process(&self, config: &AppConfig) -> Result<ProcessHandle> {
        let app_id = AppId::new(&config.name)?;
        debug!(
            "Spawning process for app: {} with command: {} {:?}",
            app_id, config.command, config.args
        );

        let mut builder = bunctl_core::process::ProcessBuilder::new(&config.command);

        // Start with the config environment variables
        let mut env_vars = config.env.clone();

        // Add important environment variables from the current process
        let important_env_vars = [
            "RUST_LOG",
            "PATH",
            "HOME",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
        ];
        for env_var in &important_env_vars {
            if let Ok(value) = std::env::var(env_var) {
                // Only add if not already set in config
                if !env_vars.contains_key(*env_var) {
                    env_vars.insert(env_var.to_string(), value);
                    debug!(
                        "Inherited environment variable {}: {}",
                        env_var,
                        env_vars.get(*env_var).unwrap_or(&"<empty>".to_string())
                    );
                }
            }
        }

        builder = builder
            .args(&config.args)
            .current_dir(&config.cwd)
            .envs(&env_vars);

        debug!(
            "Process builder configured - cwd: {:?}, env vars: {} (including inherited)",
            config.cwd,
            env_vars.len()
        );

        if let Some(uid) = config.uid {
            builder = builder.uid(uid);
            debug!("Set process UID to: {}", uid);
        }
        if let Some(gid) = config.gid {
            builder = builder.gid(gid);
            debug!("Set process GID to: {}", gid);
        }

        // CRITICAL FIX: Redirect stdout/stderr to log files instead of using pipes
        let log_dir = if cfg!(windows) {
            std::path::PathBuf::from(
                std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string()),
            )
            .join("bunctl")
            .join("logs")
        } else {
            std::path::PathBuf::from("/var/log/bunctl")
        };

        // Ensure log directory exists
        if let Err(e) = std::fs::create_dir_all(&log_dir) {
            debug!("Failed to create log directory {:?}: {}", log_dir, e);
        }

        let stdout_path = log_dir.join(format!("{}-out.log", app_id));
        let stderr_path = log_dir.join(format!("{}-err.log", app_id));

        use std::process::Stdio;
        let stdout_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&stdout_path)
            .map_err(|e| {
                error!("Failed to open stdout log file {:?}: {}", stdout_path, e);
                bunctl_core::Error::Io(e)
            })?;

        let stderr_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&stderr_path)
            .map_err(|e| {
                error!("Failed to open stderr log file {:?}: {}", stderr_path, e);
                bunctl_core::Error::Io(e)
            })?;

        builder = builder
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file));

        debug!(
            "Configured output redirection: stdout -> {:?}, stderr -> {:?}",
            stdout_path, stderr_path
        );

        debug!("Spawning child process for app: {}", app_id);
        let child = builder.spawn().await.map_err(|e| {
            error!("Failed to spawn process for app {}: {}", app_id, e);
            e
        })?;
        let pid = child.id().unwrap();
        debug!(
            "Child process spawned successfully for app: {} with PID: {}",
            app_id, pid
        );

        let handle = ProcessHandle::new(pid, app_id.clone(), child);

        // Register a clone for internal tracking (stdout/stderr will be None in clone)
        debug!(
            "Registering process handle for app: {} in process registry",
            app_id
        );
        self.registry.register(app_id.clone(), handle.clone());

        debug!(
            "Sending ProcessStarted event for app: {} PID: {}",
            app_id, pid
        );
        if let Err(e) = self
            .event_tx
            .send(SupervisorEvent::ProcessStarted {
                app: app_id.clone(),
                pid,
            })
            .await
        {
            warn!(
                "Failed to send ProcessStarted event for app {}: {}",
                app_id, e
            );
        } else {
            trace!("ProcessStarted event sent successfully for app: {}", app_id);
        }

        debug!("Process spawn completed successfully for app: {}", app_id);
        Ok(handle)
    }
}

#[async_trait]
impl ProcessSupervisor for WindowsSupervisor {
    async fn spawn(&self, config: &AppConfig) -> Result<ProcessHandle> {
        self.spawn_process(config).await
    }

    async fn kill_tree(&self, handle: &ProcessHandle) -> Result<()> {
        let mut h = handle.clone();
        h.kill().await?;
        self.registry.unregister(&handle.app_id);
        Ok(())
    }

    async fn wait(&self, handle: &mut ProcessHandle) -> Result<ExitStatus> {
        handle.wait().await
    }

    async fn get_process_info(&self, pid: u32) -> Result<ProcessInfo> {
        Ok(ProcessInfo {
            pid,
            name: String::new(),
            command: String::new(),
            args: Vec::new(),
            memory_bytes: None,
            cpu_percent: None,
            threads: None,
            open_files: None,
        })
    }

    async fn set_resource_limits(
        &self,
        _handle: &ProcessHandle,
        _config: &AppConfig,
    ) -> Result<()> {
        Ok(())
    }

    fn events(&self) -> mpsc::Receiver<SupervisorEvent> {
        self.event_rx
            .lock()
            .take()
            .expect("Events receiver already taken")
    }
}
