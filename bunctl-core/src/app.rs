use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use crate::BackoffStrategy;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppId(String);

impl AppId {
    pub fn new(name: impl Into<String>) -> crate::Result<Self> {
        let name = name.into();
        let sanitized = Self::sanitize(&name);
        if sanitized.is_empty() {
            return Err(crate::Error::InvalidAppName(name));
        }
        Ok(Self(sanitized))
    }

    fn sanitize(name: &str) -> String {
        name.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .trim_matches('-')
            .to_string()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AppId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Crashed,
    Backoff { attempt: u32, next_retry: Instant },
}

impl AppState {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }

    pub fn is_stopped(&self) -> bool {
        matches!(self, Self::Stopped)
    }
}

#[derive(Debug)]
pub struct App {
    pub id: AppId,
    pub config: Arc<RwLock<crate::AppConfig>>,
    pub state: Arc<RwLock<AppState>>,
    pub pid: Arc<RwLock<Option<u32>>>,
    pub start_time: Arc<RwLock<Option<Instant>>>,
    pub restart_count: Arc<RwLock<u32>>,
    pub last_exit_code: Arc<RwLock<Option<i32>>>,
    pub backoff: Arc<RwLock<Option<BackoffStrategy>>>,
}

impl App {
    pub fn new(id: AppId, config: crate::AppConfig) -> Self {
        Self {
            id,
            config: Arc::new(RwLock::new(config)),
            state: Arc::new(RwLock::new(AppState::Stopped)),
            pid: Arc::new(RwLock::new(None)),
            start_time: Arc::new(RwLock::new(None)),
            restart_count: Arc::new(RwLock::new(0)),
            last_exit_code: Arc::new(RwLock::new(None)),
            backoff: Arc::new(RwLock::new(None)),
        }
    }

    pub fn uptime(&self) -> Option<Duration> {
        self.start_time.read().map(|t| t.elapsed())
    }

    pub fn set_state(&self, state: AppState) {
        *self.state.write() = state;
    }

    pub fn get_state(&self) -> AppState {
        *self.state.read()
    }

    pub fn set_pid(&self, pid: Option<u32>) {
        *self.pid.write() = pid;
        if pid.is_some() {
            *self.start_time.write() = Some(Instant::now());
        } else {
            *self.start_time.write() = None;
        }
    }

    pub fn get_pid(&self) -> Option<u32> {
        *self.pid.read()
    }

    pub fn increment_restart_count(&self) {
        *self.restart_count.write() += 1;
    }

    pub fn reset_restart_count(&self) {
        *self.restart_count.write() = 0;
    }

    pub fn get_or_create_backoff(&self, config: &crate::AppConfig) -> BackoffStrategy {
        let mut backoff_lock = self.backoff.write();
        
        if let Some(existing_backoff) = &*backoff_lock {
            existing_backoff.clone()
        } else {
            let mut new_backoff = BackoffStrategy::new()
                .with_base_delay(Duration::from_millis(config.backoff.base_delay_ms))
                .with_max_delay(Duration::from_millis(config.backoff.max_delay_ms))
                .with_multiplier(config.backoff.multiplier)
                .with_jitter(config.backoff.jitter);

            if let Some(max) = config.backoff.max_attempts {
                new_backoff = new_backoff.with_max_attempts(max);
            }

            *backoff_lock = Some(new_backoff.clone());
            new_backoff
        }
    }

    pub fn update_backoff(&self, backoff: BackoffStrategy) {
        *self.backoff.write() = Some(backoff);
    }

    pub fn reset_backoff(&self) {
        *self.backoff.write() = None;
    }

    pub fn is_backoff_exhausted(&self) -> bool {
        self.backoff
            .read()
            .as_ref()
            .map(|b| b.is_exhausted())
            .unwrap_or(false)
    }
}
