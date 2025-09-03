use super::{AppConfig, Config, EcosystemConfig};
use std::path::{Path, PathBuf};

/// Config loader with auto-discovery
pub struct ConfigLoader {
    search_paths: Vec<PathBuf>,
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self {
            search_paths: vec![
                PathBuf::from("."),
                PathBuf::from("./config"),
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            ],
        }
    }
}

impl ConfigLoader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_search_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.search_paths.push(path.into());
        self
    }

    /// Auto-discover and load config from various sources
    pub async fn load(&self) -> crate::Result<Config> {
        // Priority order:
        // 1. bunctl.json
        // 2. ecosystem.config.js
        // 3. ecosystem.config.json
        // 4. pm2.config.js
        // 5. package.json with bunctl/pm2 section

        for dir in &self.search_paths {
            // Try bunctl.json
            let bunctl_json = dir.join("bunctl.json");
            if bunctl_json.exists() {
                return self.load_bunctl_json(&bunctl_json).await;
            }

            // Try ecosystem.config.js
            let ecosystem_js = dir.join("ecosystem.config.js");
            if ecosystem_js.exists() {
                return self.load_ecosystem_js(&ecosystem_js).await;
            }

            // Try ecosystem.config.json
            let ecosystem_json = dir.join("ecosystem.config.json");
            if ecosystem_json.exists() {
                return self.load_ecosystem_json(&ecosystem_json).await;
            }

            // Try pm2.config.js (PM2 compatibility)
            let pm2_js = dir.join("pm2.config.js");
            if pm2_js.exists() {
                return self.load_ecosystem_js(&pm2_js).await;
            }

            // Try package.json
            let package_json = dir.join("package.json");
            if package_json.exists()
                && let Ok(config) = self.load_package_json(&package_json).await
            {
                return Ok(config);
            }
        }

        // No config found, return default
        Ok(Config::default())
    }

    async fn load_bunctl_json(&self, path: &Path) -> crate::Result<Config> {
        let content = tokio::fs::read_to_string(path).await?;
        serde_json::from_str(&content)
            .map_err(|e| crate::Error::Config(format!("Failed to parse bunctl.json: {}", e)))
    }

    async fn load_ecosystem_js(&self, path: &Path) -> crate::Result<Config> {
        let ecosystem = EcosystemConfig::load_from_js(path).await?;
        Ok(self.ecosystem_to_config(ecosystem))
    }

    async fn load_ecosystem_json(&self, path: &Path) -> crate::Result<Config> {
        let ecosystem = EcosystemConfig::load_from_json(path).await?;
        Ok(self.ecosystem_to_config(ecosystem))
    }

    async fn load_package_json(&self, path: &Path) -> crate::Result<Config> {
        let content = tokio::fs::read_to_string(path).await?;
        let package: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| crate::Error::Config(format!("Failed to parse package.json: {}", e)))?;

        // Check for bunctl section
        if let Some(bunctl_section) = package.get("bunctl") {
            let config: Config = serde_json::from_value(bunctl_section.clone()).map_err(|e| {
                crate::Error::Config(format!("Failed to parse bunctl section: {}", e))
            })?;
            return Ok(config);
        }

        // Check for pm2 section (PM2 compatibility)
        if let Some(pm2_section) = package.get("pm2")
            && let Ok(ecosystem) = serde_json::from_value::<EcosystemConfig>(pm2_section.clone())
        {
            return Ok(self.ecosystem_to_config(ecosystem));
        }

        // Try to create a simple config from package.json scripts
        if let Some(name) = package.get("name").and_then(|v| v.as_str())
            && let Some(scripts) = package.get("scripts").and_then(|v| v.as_object())
            && let Some(_start_script) = scripts.get("start").and_then(|v| v.as_str())
        {
            let app = AppConfig {
                name: name.to_string(),
                command: "bun".to_string(),
                args: vec!["run".to_string(), "start".to_string()],
                ..Default::default()
            };
            return Ok(Config {
                apps: vec![app],
            });
        }

        Err(crate::Error::Config(
            "No valid config found in package.json".to_string(),
        ))
    }

    fn ecosystem_to_config(&self, ecosystem: EcosystemConfig) -> Config {
        Config {
            apps: ecosystem
                .apps
                .iter()
                .map(|app| app.to_app_config())
                .collect(),
        }
    }

    /// Load a specific config file
    pub async fn load_file(&self, path: &Path) -> crate::Result<Config> {
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");

        match (extension, filename) {
            ("json", _) if filename.starts_with("bunctl") => self.load_bunctl_json(path).await,
            ("json", _) if filename.contains("ecosystem") || filename.contains("pm2") => {
                self.load_ecosystem_json(path).await
            }
            ("js", _) if filename.contains("ecosystem") || filename.contains("pm2") => {
                self.load_ecosystem_js(path).await
            }
            ("json", "package.json") => self.load_package_json(path).await,
            _ => {
                // Try to parse as bunctl.json format first
                if let Ok(config) = self.load_bunctl_json(path).await {
                    Ok(config)
                } else {
                    // Fall back to ecosystem format
                    self.load_ecosystem_json(path).await
                }
            }
        }
    }
}
