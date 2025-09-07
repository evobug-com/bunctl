use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// PM2 ecosystem.config.js compatible format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcosystemConfig {
    pub apps: Vec<EcosystemApp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcosystemApp {
    pub name: String,
    pub script: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub interpreter: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub interpreter_args: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub instances: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub exec_mode: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub watch: Option<BoolOrVec>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_watch: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_memory_restart: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_production: Option<HashMap<String, String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_development: Option<HashMap<String, String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_file: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub out_file: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_file: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_date_format: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_logs: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub autorestart: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_delay: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_uptime: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_restarts: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub kill_timeout: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait_ready: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub listen_timeout: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BoolOrVec {
    Bool(bool),
    Vec(Vec<String>),
}

impl EcosystemApp {
    /// Convert to our internal AppConfig format
    pub fn to_app_config(&self) -> crate::AppConfig {
        let interpreter = self.interpreter.as_deref().unwrap_or("bun");
        let _script_path = PathBuf::from(&self.script);

        // Separate command and args to prevent command injection
        let (command, mut base_args) = if interpreter == "none" {
            (self.script.clone(), Vec::new())
        } else {
            // Keep interpreter as command, script as first argument
            (interpreter.to_string(), vec![self.script.clone()])
        };

        // Parse additional args if provided (with warning about security)
        let additional_args = self
            .args
            .as_ref()
            .map(|a| {
                // Split args by whitespace only, no shell parsing for security
                a.split_whitespace()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        base_args.extend(additional_args);
        let args = base_args;

        let cwd = self
            .cwd
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));

        let mut env = HashMap::new();
        if let Some(base_env) = &self.env {
            env.extend(base_env.clone());
        }

        // Apply environment-specific overrides
        let node_env = std::env::var("NODE_ENV").unwrap_or_else(|_| "production".to_string());
        if node_env == "production" {
            if let Some(prod_env) = &self.env_production {
                env.extend(prod_env.clone());
            }
        } else if node_env == "development"
            && let Some(dev_env) = &self.env_development
        {
            env.extend(dev_env.clone());
        }

        let max_memory = self
            .max_memory_restart
            .as_ref()
            .and_then(|m| parse_memory_string(m));

        let restart_policy = if self.autorestart.unwrap_or(true) {
            crate::config::RestartPolicy::Always
        } else {
            crate::config::RestartPolicy::No
        };

        crate::AppConfig {
            name: self.name.clone(),
            command,
            args,
            cwd,
            env,
            auto_start: true,
            restart_policy,
            max_memory,
            max_cpu_percent: None,
            uid: None,
            gid: None,
            stdout_log: self.out_file.as_ref().map(PathBuf::from),
            stderr_log: self.error_file.as_ref().map(PathBuf::from),
            combined_log: self.log_file.as_ref().map(PathBuf::from),
            log_max_size: None,
            log_max_files: None,
            health_check: None,
            stop_timeout: std::time::Duration::from_millis(self.kill_timeout.unwrap_or(5000)),
            kill_timeout: std::time::Duration::from_secs(5),
            backoff: crate::config::BackoffConfig {
                base_delay_ms: self.restart_delay.unwrap_or(100),
                max_delay_ms: 30000,
                multiplier: 2.0,
                jitter: 0.3,
                max_attempts: self.max_restarts,
                exhausted_action: crate::config::ExhaustedAction::default(),
            },
        }
    }
}

fn parse_memory_string(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }

    if let Some(kb) = s.strip_suffix("k") {
        kb.parse::<u64>().ok().map(|v| v * 1024)
    } else if let Some(mb) = s.strip_suffix("m") {
        mb.parse::<u64>().ok().map(|v| v * 1024 * 1024)
    } else if let Some(gb) = s.strip_suffix("g") {
        gb.parse::<u64>().ok().map(|v| v * 1024 * 1024 * 1024)
    } else {
        s.parse::<u64>().ok()
    }
}

impl EcosystemConfig {
    pub async fn load_from_js(_path: &Path) -> crate::Result<Self> {
        // SECURITY: Do not execute JavaScript files to prevent code injection attacks.
        // ecosystem.config.js files must be converted to JSON format.
        Err(crate::Error::Config(
            "JavaScript config files are not supported for security reasons. \
             Please convert your ecosystem.config.js to ecosystem.config.json format. \
             Example: module.exports = { apps: [...] } should become { \"apps\": [...] }"
                .to_string(),
        ))
    }

    pub async fn load_from_json(path: &Path) -> crate::Result<Self> {
        let content = tokio::fs::read_to_string(path).await?;
        serde_json::from_str(&content)
            .map_err(|e| crate::Error::Config(format!("Failed to parse JSON config: {}", e)))
    }
}
