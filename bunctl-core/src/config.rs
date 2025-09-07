pub mod ecosystem;
pub mod loader;

use arc_swap::ArcSwap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;

pub use ecosystem::{EcosystemApp, EcosystemConfig};
pub use loader::ConfigLoader;

/// Get the default socket path based on the platform
pub fn default_socket_path() -> PathBuf {
    if cfg!(windows) {
        // Use named pipe on Windows
        PathBuf::from(r"\\.\pipe\bunctl")
    } else {
        // Use Unix domain socket on Unix-like systems
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            PathBuf::from(runtime_dir).join("bunctl.sock")
        } else {
            PathBuf::from("/tmp/bunctl.sock")
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AppConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub auto_start: bool,
    pub restart_policy: RestartPolicy,
    pub max_memory: Option<u64>,
    pub max_cpu_percent: Option<f32>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub stdout_log: Option<PathBuf>,
    pub stderr_log: Option<PathBuf>,
    pub combined_log: Option<PathBuf>,
    pub log_max_size: Option<u64>,
    pub log_max_files: Option<u32>,
    pub health_check: Option<HealthCheck>,
    pub stop_timeout: Duration,
    pub kill_timeout: Duration,
    pub backoff: BackoffConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            command: String::new(),
            args: Vec::new(),
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            env: HashMap::new(),
            auto_start: false,
            restart_policy: RestartPolicy::OnFailure,
            max_memory: None,
            max_cpu_percent: None,
            uid: None,
            gid: None,
            stdout_log: None,
            stderr_log: None,
            combined_log: None,
            log_max_size: Some(10 * 1024 * 1024),
            log_max_files: Some(10),
            health_check: None,
            stop_timeout: Duration::from_secs(10),
            kill_timeout: Duration::from_secs(5),
            backoff: BackoffConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RestartPolicy {
    #[serde(rename = "no")]
    No,
    #[serde(rename = "always")]
    Always,
    #[serde(rename = "on-failure")]
    OnFailure,
    #[serde(rename = "unless-stopped")]
    UnlessStopped,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ExhaustedAction {
    Stop,
    Remove,
}

impl Default for ExhaustedAction {
    fn default() -> Self {
        Self::Stop
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackoffConfig {
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub multiplier: f64,
    pub jitter: f64,
    pub max_attempts: Option<u32>,
    #[serde(default)]
    pub exhausted_action: ExhaustedAction,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            base_delay_ms: 100,
            max_delay_ms: 30000,
            multiplier: 2.0,
            jitter: 0.3,
            max_attempts: None,
            exhausted_action: ExhaustedAction::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthCheck {
    pub check_type: HealthCheckType,
    pub interval: Duration,
    pub timeout: Duration,
    pub retries: u32,
    pub start_period: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HealthCheckType {
    #[serde(rename = "http")]
    Http { url: String, expected_status: u16 },
    #[serde(rename = "tcp")]
    Tcp { host: String, port: u16 },
    #[serde(rename = "exec")]
    Exec { command: String, args: Vec<String> },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub apps: Vec<AppConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DaemonConfig {
    pub socket_path: PathBuf,
    pub log_level: String,
    pub metrics_port: Option<u16>,
    pub max_parallel_starts: usize,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: default_socket_path(),
            log_level: "info".to_string(),
            metrics_port: None,
            max_parallel_starts: 4,
        }
    }
}

impl DaemonConfig {
    pub fn is_default(&self) -> bool {
        let default = Self::default();
        self.socket_path == default.socket_path
            && self.log_level == default.log_level
            && self.metrics_port == default.metrics_port
            && self.max_parallel_starts == default.max_parallel_starts
    }

    pub async fn load_from_file(path: impl AsRef<Path>) -> crate::Result<Self> {
        let content = fs::read_to_string(path.as_ref()).await?;
        let config: DaemonConfig = serde_json::from_str(&content)
            .map_err(|e| crate::Error::Config(format!("Failed to parse daemon config: {}", e)))?;

        validate_log_level(&config.log_level).map_err(crate::Error::Config)?;

        validate_daemon_config(&config).map_err(crate::Error::Config)?;

        Ok(config)
    }
}

pub struct ConfigWatcher {
    path: PathBuf,
    current: ArcSwap<Config>,
    checksum: Arc<RwLock<Vec<u8>>>,
}

impl ConfigWatcher {
    pub async fn new(path: impl AsRef<Path>) -> crate::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let config = Self::load_config(&path).await?;
        let checksum = Self::compute_checksum(&path).await?;

        Ok(Self {
            path,
            current: ArcSwap::new(Arc::new(config)),
            checksum: Arc::new(RwLock::new(checksum)),
        })
    }

    async fn load_config(path: &Path) -> crate::Result<Config> {
        let content = fs::read_to_string(path).await?;
        let mut config: Config = serde_json::from_str(&content)
            .map_err(|e| crate::Error::Config(format!("Failed to parse config: {}", e)))?;

        // Validate configuration values
        Self::validate_config(&mut config)?;

        Ok(config)
    }

    fn validate_config(config: &mut Config) -> crate::Result<()> {
        // Validate each app configuration
        for app in &config.apps {
            validate_restart_policy(&app.restart_policy)
                .map_err(|e| crate::Error::Config(format!("App '{}': {}", app.name, e)))?;

            validate_exhausted_action(&app.backoff.exhausted_action)
                .map_err(|e| crate::Error::Config(format!("App '{}': {}", app.name, e)))?;

            // Validate numeric ranges
            if let Some(cpu) = app.max_cpu_percent {
                if cpu <= 0.0 {
                    return Err(crate::Error::Config(format!(
                        "App '{}': max_cpu_percent must be greater than 0.0, got {}",
                        app.name, cpu
                    )));
                }
                // Get number of CPU cores for validation
                let num_cores = std::thread::available_parallelism()
                    .map(|n| n.get() as f32)
                    .unwrap_or(1.0);
                let max_cpu = 100.0 * num_cores;
                if cpu > max_cpu {
                    return Err(crate::Error::Config(format!(
                        "App '{}': max_cpu_percent ({}) exceeds system maximum ({}% for {} cores)",
                        app.name, cpu, max_cpu, num_cores
                    )));
                }
            }

            if app.backoff.multiplier < 1.0 {
                return Err(crate::Error::Config(format!(
                    "App '{}': backoff multiplier must be >= 1.0, got {}",
                    app.name, app.backoff.multiplier
                )));
            }

            if app.backoff.jitter < 0.0 || app.backoff.jitter > 1.0 {
                return Err(crate::Error::Config(format!(
                    "App '{}': backoff jitter must be between 0.0 and 1.0, got {}",
                    app.name, app.backoff.jitter
                )));
            }
        }

        Ok(())
    }

    async fn compute_checksum(path: &Path) -> crate::Result<Vec<u8>> {
        let content = fs::read(path).await?;
        let mut hasher = Sha256::new();
        hasher.update(&content);
        Ok(hasher.finalize().to_vec())
    }

    pub async fn check_reload(&self) -> crate::Result<bool> {
        let new_checksum = Self::compute_checksum(&self.path).await?;
        let current_checksum = self.checksum.read().clone();

        if new_checksum != current_checksum {
            let new_config = Self::load_config(&self.path).await?;
            self.current.store(Arc::new(new_config));
            *self.checksum.write() = new_checksum;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn get(&self) -> Arc<Config> {
        self.current.load_full()
    }
}

// Raw deserialization struct for AppConfig with strict validation
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AppConfigRaw {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    pub cwd: Option<PathBuf>,
    pub env: Option<HashMap<String, String>>,
    pub auto_start: Option<bool>,
    pub restart_policy: Option<RestartPolicy>,
    pub max_memory: Option<u64>,
    pub max_cpu_percent: Option<f32>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub stdout_log: Option<PathBuf>,
    pub stderr_log: Option<PathBuf>,
    pub combined_log: Option<PathBuf>,
    pub log_max_size: Option<u64>,
    pub log_max_files: Option<u32>,
    pub health_check: Option<HealthCheck>,
    pub stop_timeout: Option<Duration>,
    pub kill_timeout: Option<Duration>,
    pub backoff: Option<BackoffConfig>,
}

impl<'de> Deserialize<'de> for AppConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        AppConfigRaw::deserialize(deserializer).map(Into::into)
    }
}

fn validate_log_level(level: &str) -> Result<(), String> {
    match level.to_lowercase().as_str() {
        "error" | "warn" | "info" | "debug" | "trace" => Ok(()),
        _ => Err(format!(
            "Invalid log_level '{}'. Must be one of: error, warn, info, debug, trace",
            level
        )),
    }
}

fn validate_restart_policy(_policy: &RestartPolicy) -> Result<(), String> {
    // All enum variants are valid by definition
    Ok(())
}

fn validate_exhausted_action(_action: &ExhaustedAction) -> Result<(), String> {
    // All enum variants are valid by definition
    Ok(())
}

fn validate_daemon_config(daemon: &DaemonConfig) -> Result<(), String> {
    // Validate max_parallel_starts
    if daemon.max_parallel_starts == 0 {
        return Err("max_parallel_starts must be greater than 0".to_string());
    }

    if daemon.max_parallel_starts > 100 {
        return Err(
            "max_parallel_starts must be 100 or less to prevent resource exhaustion".to_string(),
        );
    }

    // Validate metrics port if specified
    if let Some(port) = daemon.metrics_port
        && port < 1024
    {
        return Err("metrics_port should be >= 1024 to avoid privileged ports".to_string());
    }

    // Validate socket path is not empty
    if daemon.socket_path.as_os_str().is_empty() {
        return Err("socket_path cannot be empty".to_string());
    }

    Ok(())
}

impl From<AppConfigRaw> for AppConfig {
    fn from(raw: AppConfigRaw) -> Self {
        // Security: Do not parse shell commands to prevent command injection
        // Users must provide command and args separately in the config
        let (command, args) = if let Some(args) = raw.args {
            (raw.command, args)
        } else {
            // If args are not explicitly provided, assume command has no arguments
            // Do NOT attempt to parse the command string for security reasons
            (raw.command, Vec::new())
        };

        AppConfig {
            name: raw.name,
            command,
            args,
            cwd: raw
                .cwd
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))),
            env: raw.env.unwrap_or_default(),
            auto_start: raw.auto_start.unwrap_or(false),
            restart_policy: raw.restart_policy.unwrap_or(RestartPolicy::OnFailure),
            max_memory: raw.max_memory,
            max_cpu_percent: raw.max_cpu_percent,
            uid: raw.uid,
            gid: raw.gid,
            stdout_log: raw.stdout_log,
            stderr_log: raw.stderr_log,
            combined_log: raw.combined_log,
            log_max_size: raw.log_max_size.or(Some(10 * 1024 * 1024)),
            log_max_files: raw.log_max_files.or(Some(10)),
            health_check: raw.health_check,
            stop_timeout: raw.stop_timeout.unwrap_or(Duration::from_secs(10)),
            kill_timeout: raw.kill_timeout.unwrap_or(Duration::from_secs(5)),
            backoff: raw.backoff.unwrap_or_default(),
        }
    }
}
