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

        let command = if interpreter == "none" {
            self.script.clone()
        } else {
            format!("{} {}", interpreter, self.script)
        };

        let args = self
            .args
            .as_ref()
            .map(|a| shell_words::split(a).unwrap_or_default())
            .unwrap_or_default();

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
    pub async fn load_from_js(path: &Path) -> crate::Result<Self> {
        // Execute the JS file with Bun and capture JSON output
        let output = tokio::process::Command::new("bun")
            .arg("--print")
            .arg(format!(
                "JSON.stringify(require('{}').apps || require('{}'))",
                path.display(),
                path.display()
            ))
            .output()
            .await?;

        if !output.status.success() {
            return Err(crate::Error::Config(format!(
                "Failed to load ecosystem.config.js: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let json = String::from_utf8(output.stdout)
            .map_err(|e| crate::Error::Config(format!("Invalid UTF-8 in config: {}", e)))?;

        let apps: Vec<EcosystemApp> = serde_json::from_str(&json)
            .map_err(|e| crate::Error::Config(format!("Failed to parse config: {}", e)))?;

        Ok(Self { apps })
    }

    pub async fn load_from_json(path: &Path) -> crate::Result<Self> {
        let content = tokio::fs::read_to_string(path).await?;
        serde_json::from_str(&content)
            .map_err(|e| crate::Error::Config(format!("Failed to parse JSON config: {}", e)))
    }
}
