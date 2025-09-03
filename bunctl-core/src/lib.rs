pub mod app;
pub mod backoff;
pub mod config;
pub mod error;
pub mod process;
pub mod supervisor;

pub use app::{App, AppId, AppState};
pub use backoff::BackoffStrategy;
pub use config::{AppConfig, Config, ConfigWatcher};
pub use error::{Error, Result};
pub use process::{ExitStatus, ProcessBuilder, ProcessHandle, ProcessInfo, Signal};
pub use supervisor::{ProcessSupervisor, SupervisorEvent};

#[cfg(test)]
mod backoff_exhaustion_test;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_id_sanitization() {
        assert_eq!(AppId::new("test-app").unwrap().as_str(), "test-app");
        assert_eq!(AppId::new("Test App").unwrap().as_str(), "test-app");
        assert_eq!(AppId::new("TEST_APP").unwrap().as_str(), "test_app");
        assert_eq!(AppId::new("test@app!").unwrap().as_str(), "test-app");
        assert_eq!(AppId::new("  test  ").unwrap().as_str(), "test");
    }

    #[test]
    fn test_app_id_validation() {
        assert!(AppId::new("valid-name").is_ok());
        assert!(AppId::new("valid.name").is_ok());
        assert!(AppId::new("valid_name").is_ok());
        assert!(AppId::new("123").is_ok());
        assert!(AppId::new("").is_err());
        assert!(AppId::new("   ").is_err());
    }
}
