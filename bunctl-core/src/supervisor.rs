use async_trait::async_trait;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::{AppConfig, AppId, ExitStatus, ProcessHandle};

#[derive(Debug, Clone)]
pub enum SupervisorEvent {
    ProcessStarted { app: AppId, pid: u32 },
    ProcessExited { app: AppId, status: ExitStatus },
    ProcessCrashed { app: AppId, reason: String },
    ProcessRestarting { app: AppId, attempt: u32, delay: Duration },
    BackoffExhausted { app: AppId },
    ConfigReloaded,
    HealthCheckFailed { app: AppId, reason: String },
    ResourceLimitExceeded { app: AppId, resource: String, limit: u64, current: u64 },
}

#[async_trait]
pub trait ProcessSupervisor: Send + Sync {
    async fn spawn(&self, config: &AppConfig) -> crate::Result<ProcessHandle>;
    
    async fn kill_tree(&self, handle: &ProcessHandle) -> crate::Result<()>;
    
    async fn wait(&self, handle: &mut ProcessHandle) -> crate::Result<ExitStatus>;
    
    async fn get_process_info(&self, pid: u32) -> crate::Result<crate::ProcessInfo>;
    
    async fn set_resource_limits(&self, handle: &ProcessHandle, config: &AppConfig) -> crate::Result<()>;
    
    fn events(&self) -> mpsc::Receiver<SupervisorEvent>;
    
    async fn graceful_stop(&self, handle: &mut ProcessHandle, timeout: Duration) -> crate::Result<ExitStatus> {
        handle.signal(crate::process::Signal::Terminate).await?;
        
        let result = tokio::select! {
            status = handle.wait() => status,
            _ = tokio::time::sleep(timeout) => {
                handle.kill().await?;
                handle.wait().await
            }
        };
        
        result
    }
    
    async fn reload(&self, handle: &mut ProcessHandle) -> crate::Result<()> {
        handle.signal(crate::process::Signal::Reload).await
    }
}