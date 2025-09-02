use async_trait::async_trait;
use bunctl_core::{
    AppConfig, AppId, ExitStatus, ProcessHandle, ProcessInfo, ProcessSupervisor, Result,
    SupervisorEvent,
};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::common::ProcessRegistry;

pub struct WindowsSupervisor {
    registry: Arc<ProcessRegistry>,
    event_tx: mpsc::Sender<SupervisorEvent>,
    event_rx: parking_lot::Mutex<Option<mpsc::Receiver<SupervisorEvent>>>,
}

impl WindowsSupervisor {
    pub async fn new() -> Result<Self> {
        let (event_tx, event_rx) = mpsc::channel(1024);

        Ok(Self {
            registry: Arc::new(ProcessRegistry::new()),
            event_tx,
            event_rx: parking_lot::Mutex::new(Some(event_rx)),
        })
    }

    async fn spawn_process(&self, config: &AppConfig) -> Result<ProcessHandle> {
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
