#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::LinuxSupervisor as PlatformSupervisor;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::MacOSSupervisor as PlatformSupervisor;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::WindowsSupervisor as PlatformSupervisor;

mod common;
pub use common::*;

use bunctl_core::{ProcessSupervisor, Result};
use std::sync::Arc;

pub async fn create_supervisor() -> Result<Arc<dyn ProcessSupervisor>> {
    Ok(Arc::new(PlatformSupervisor::new().await?))
}