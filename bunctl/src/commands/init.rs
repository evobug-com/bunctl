use crate::cli::InitArgs;
use bunctl_core::config::{AppConfig, Config, EcosystemConfig};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub async fn execute(args: InitArgs) -> anyhow::Result<()> {
    // If loading from existing ecosystem.config.js
    if let Some(ecosystem_path) = args.from_ecosystem {
        return import_from_ecosystem(&ecosystem_path).await;
    }

    // Determine application name
    let name = args.name.unwrap_or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
            .unwrap_or_else(|| "app".to_string())
    });

    // Determine entry point
    let entry = args.entry.or(args.script).unwrap_or_else(|| {
        // Auto-detect common entry files
        let candidates = vec![
            "src/server.ts",
            "src/index.ts",
            "src/app.ts",
            "server.ts",
            "index.ts",
            "app.ts",
            "src/server.js",
            "src/index.js",
            "src/app.js",
            "server.js",
            "index.js",
            "app.js",
        ];

        for candidate in candidates {
            let path = PathBuf::from(candidate);
            if path.exists() {
                return path;
            }
        }

        PathBuf::from("index.ts")
    });

    let cwd = args
        .cwd
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| anyhow::anyhow!("Failed to determine working directory"))?;

    // Parse memory limit
    let max_memory = parse_memory_string(&args.memory);

    // Build app config
    let app_config = AppConfig {
        name: name.clone(),
        command: if args.runtime == "bun" {
            format!("bun run {}", entry.display())
        } else {
            format!("node {}", entry.display())
        },
        args: Vec::new(),
        cwd: cwd.clone(),
        env: {
            let mut env = HashMap::new();
            if let Some(port) = args.port {
                env.insert("PORT".to_string(), port.to_string());
            }
            env.insert("NODE_ENV".to_string(), "production".to_string());
            env
        },
        auto_start: args.autostart,
        restart_policy: if args.autostart {
            bunctl_core::config::RestartPolicy::Always
        } else {
            bunctl_core::config::RestartPolicy::OnFailure
        },
        max_memory,
        max_cpu_percent: Some(args.cpu),
        uid: None,
        gid: None,
        stdout_log: Some(PathBuf::from(format!("logs/{}-out.log", name))),
        stderr_log: Some(PathBuf::from(format!("logs/{}-error.log", name))),
        combined_log: None,
        log_max_size: Some(10 * 1024 * 1024),
        log_max_files: Some(10),
        health_check: None,
        stop_timeout: std::time::Duration::from_secs(10),
        kill_timeout: std::time::Duration::from_secs(5),
        backoff: Default::default(),
    };

    // Generate config file
    if args.ecosystem {
        generate_ecosystem_config(&app_config, args.instances).await?;
    } else {
        generate_bunctl_config(&app_config).await?;
    }

    println!("✔ Initialized app '{}'", name);
    println!("• Working dir: {}", cwd.display());
    println!("• Entry:       {}", entry.display());
    println!("• Runtime:     {}", args.runtime);
    println!("• Memory:      {}", args.memory);
    println!("• CPU:         {}%", args.cpu);
    if let Some(port) = args.port {
        println!("• Port:        {}", port);
    }

    if args.ecosystem {
        println!("• Config:      ecosystem.config.js");
        println!("\nStart with: bunctl start --config ecosystem.config.js");
    } else {
        println!("• Config:      bunctl.json");
        println!("\nStart with: bunctl start {}", name);
    }

    Ok(())
}

async fn import_from_ecosystem(path: &Path) -> anyhow::Result<()> {
    println!("Importing from {}...", path.display());

    let ecosystem = EcosystemConfig::load_from_js(path).await?;

    // Convert to bunctl format
    let config = Config {
        apps: ecosystem
            .apps
            .iter()
            .map(|app| app.to_app_config())
            .collect(),
    };

    // Write bunctl.json
    let json = serde_json::to_string_pretty(&config)?;
    tokio::fs::write("bunctl.json", json).await?;

    println!(
        "✔ Imported {} app(s) from ecosystem.config.js",
        ecosystem.apps.len()
    );
    for app in &ecosystem.apps {
        println!("  • {}", app.name);
    }
    println!("\nConfig saved to: bunctl.json");
    println!("Start with: bunctl start --config bunctl.json");

    Ok(())
}

async fn generate_bunctl_config(app: &AppConfig) -> anyhow::Result<()> {
    let config = Config {
        apps: vec![app.clone()],
    };

    let json = serde_json::to_string_pretty(&config)?;
    tokio::fs::write("bunctl.json", json).await?;

    Ok(())
}

async fn generate_ecosystem_config(app: &AppConfig, instances: usize) -> anyhow::Result<()> {
    let ecosystem_app = bunctl_core::config::ecosystem::EcosystemApp {
        name: app.name.clone(),
        script: app
            .command
            .split_whitespace()
            .last()
            .unwrap_or("index.ts")
            .to_string(),
        cwd: Some(app.cwd.to_string_lossy().to_string()),
        args: None,
        interpreter: Some("bun".to_string()),
        interpreter_args: None,
        instances: if instances > 1 { Some(instances) } else { None },
        exec_mode: if instances > 1 {
            Some("cluster".to_string())
        } else {
            None
        },
        watch: None,
        ignore_watch: None,
        max_memory_restart: app.max_memory.map(format_memory),
        env: Some(app.env.clone()),
        env_production: None,
        env_development: None,
        error_file: app
            .stderr_log
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        out_file: app
            .stdout_log
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        log_file: app
            .combined_log
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        log_date_format: None,
        merge_logs: None,
        autorestart: Some(matches!(
            app.restart_policy,
            bunctl_core::config::RestartPolicy::Always
        )),
        restart_delay: Some(app.backoff.base_delay_ms),
        min_uptime: None,
        max_restarts: app.backoff.max_attempts,
        kill_timeout: Some(app.stop_timeout.as_millis() as u64),
        wait_ready: None,
        listen_timeout: None,
    };

    let ecosystem = EcosystemConfig {
        apps: vec![ecosystem_app],
    };

    let js_content = format!(
        "module.exports = {};",
        serde_json::to_string_pretty(&ecosystem)?
    );

    tokio::fs::write("ecosystem.config.js", js_content).await?;

    Ok(())
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

fn format_memory(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{}G", bytes / (1024 * 1024 * 1024))
    } else if bytes >= 1024 * 1024 {
        format!("{}M", bytes / (1024 * 1024))
    } else if bytes >= 1024 {
        format!("{}K", bytes / 1024)
    } else {
        format!("{}", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_parse_memory_string() {
        assert_eq!(parse_memory_string(""), None);
        assert_eq!(parse_memory_string("100"), Some(100));
        assert_eq!(parse_memory_string("100K"), Some(100 * 1024));
        assert_eq!(parse_memory_string("100k"), Some(100 * 1024));
        assert_eq!(parse_memory_string("512M"), Some(512 * 1024 * 1024));
        assert_eq!(parse_memory_string("512m"), Some(512 * 1024 * 1024));
        assert_eq!(parse_memory_string("2G"), Some(2 * 1024 * 1024 * 1024));
        assert_eq!(parse_memory_string("2g"), Some(2 * 1024 * 1024 * 1024));
        assert_eq!(parse_memory_string("invalid"), None);
        assert_eq!(parse_memory_string("100X"), None);
    }

    #[test]
    fn test_format_memory() {
        assert_eq!(format_memory(100), "100");
        assert_eq!(format_memory(1024), "1K");
        assert_eq!(format_memory(100 * 1024), "100K");
        assert_eq!(format_memory(1024 * 1024), "1M");
        assert_eq!(format_memory(512 * 1024 * 1024), "512M");
        assert_eq!(format_memory(1024 * 1024 * 1024), "1G");
        assert_eq!(format_memory(2 * 1024 * 1024 * 1024), "2G");
    }

    #[tokio::test]
    #[serial]
    async fn test_generate_bunctl_config() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory for test");

        let app_config = AppConfig {
            name: "test-app".to_string(),
            command: "bun run server.ts".to_string(),
            args: vec!["--port".to_string(), "3000".to_string()],
            cwd: PathBuf::from("/app"),
            env: {
                let mut env = HashMap::new();
                env.insert("NODE_ENV".to_string(), "production".to_string());
                env.insert("PORT".to_string(), "3000".to_string());
                env
            },
            auto_start: true,
            restart_policy: bunctl_core::config::RestartPolicy::Always,
            max_memory: Some(512 * 1024 * 1024),
            max_cpu_percent: Some(50.0),
            ..Default::default()
        };

        // Save current dir and change to temp directory to write config there
        let original_dir = std::env::current_dir().expect("Failed to get current directory");
        std::env::set_current_dir(&temp_dir).expect("Failed to change to temp directory");

        generate_bunctl_config(&app_config)
            .await
            .expect("Failed to generate bunctl config");

        // The config should be written in the current directory (temp_dir)
        let config_path = temp_dir.path().join("bunctl.json");
        assert!(config_path.exists());

        let content = fs::read_to_string(&config_path).expect("Failed to read config file");
        let config: Config = serde_json::from_str(&content).expect("Failed to parse config JSON");

        assert_eq!(config.apps.len(), 1);
        assert_eq!(config.apps[0].name, "test-app");
        assert_eq!(config.apps[0].command, "bun run server.ts");
        assert_eq!(config.apps[0].args, vec!["--port", "3000"]);
        assert_eq!(
            config.apps[0].env.get("NODE_ENV"),
            Some(&"production".to_string())
        );
        assert_eq!(config.apps[0].env.get("PORT"), Some(&"3000".to_string()));
        assert!(config.apps[0].auto_start);
        assert_eq!(config.apps[0].max_memory, Some(512 * 1024 * 1024));
        assert_eq!(config.apps[0].max_cpu_percent, Some(50.0));

        // Restore original directory
        std::env::set_current_dir(original_dir).expect("Failed to restore original directory");
    }

    #[tokio::test]
    #[serial]
    async fn test_generate_ecosystem_config() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory for test");

        let app_config = AppConfig {
            name: "eco-app".to_string(),
            command: "bun run index.ts".to_string(),
            args: vec![],
            cwd: PathBuf::from("/app"),
            env: {
                let mut env = HashMap::new();
                env.insert("NODE_ENV".to_string(), "production".to_string());
                env
            },
            auto_start: false,
            restart_policy: bunctl_core::config::RestartPolicy::OnFailure,
            max_memory: Some(1024 * 1024 * 1024),
            max_cpu_percent: None,
            stdout_log: Some(PathBuf::from("logs/eco-app-out.log")),
            stderr_log: Some(PathBuf::from("logs/eco-app-error.log")),
            ..Default::default()
        };

        // Save current dir and change to temp directory to write config there
        let original_dir = std::env::current_dir().expect("Failed to get current directory");
        std::env::set_current_dir(&temp_dir).expect("Failed to change to temp directory");

        generate_ecosystem_config(&app_config, 1)
            .await
            .expect("Failed to generate ecosystem config");

        let config_path = temp_dir.path().join("ecosystem.config.js");
        assert!(config_path.exists());

        let content = fs::read_to_string(&config_path).expect("Failed to read config file");
        assert!(content.contains("module.exports"));
        assert!(content.contains("eco-app"));
        assert!(content.contains("index.ts"));
        assert!(content.contains("NODE_ENV"));
        assert!(content.contains("production"));

        // Restore original directory
        std::env::set_current_dir(original_dir).expect("Failed to restore original directory");
    }

    #[tokio::test]
    #[serial]
    async fn test_generate_ecosystem_config_cluster_mode() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory for test");

        let app_config = AppConfig {
            name: "cluster-app".to_string(),
            command: "bun run server.ts".to_string(),
            args: vec![],
            cwd: PathBuf::from("/app"),
            env: HashMap::new(),
            auto_start: true,
            restart_policy: bunctl_core::config::RestartPolicy::Always,
            ..Default::default()
        };

        // Save current dir and change to temp directory to write config there
        let original_dir = std::env::current_dir().expect("Failed to get current directory");
        std::env::set_current_dir(&temp_dir).expect("Failed to change to temp directory");

        generate_ecosystem_config(&app_config, 4)
            .await
            .expect("Failed to generate ecosystem config with cluster mode");

        let config_path = temp_dir.path().join("ecosystem.config.js");
        assert!(config_path.exists());

        let content = fs::read_to_string(&config_path).expect("Failed to read config file");
        // The content is JavaScript, but contains JSON inside module.exports
        // Check for the values without expecting exact JSON formatting
        assert!(content.contains("\"instances\"") && content.contains("4"));
        assert!(content.contains("\"exec_mode\"") && content.contains("\"cluster\""));
        assert!(content.contains("\"autorestart\"") && content.contains("true"));

        // Restore original directory
        std::env::set_current_dir(original_dir).expect("Failed to restore original directory");
    }

    #[test]
    fn test_memory_parsing_edge_cases() {
        // Test with whitespace
        assert_eq!(parse_memory_string("  100M  "), Some(100 * 1024 * 1024));
        assert_eq!(parse_memory_string("  1G  "), Some(1024 * 1024 * 1024));

        // Test case insensitive
        assert_eq!(parse_memory_string("100K"), Some(100 * 1024));
        assert_eq!(parse_memory_string("100k"), Some(100 * 1024));
        assert_eq!(parse_memory_string("100M"), Some(100 * 1024 * 1024));
        assert_eq!(parse_memory_string("100m"), Some(100 * 1024 * 1024));
        assert_eq!(parse_memory_string("100G"), Some(100 * 1024 * 1024 * 1024));
        assert_eq!(parse_memory_string("100g"), Some(100 * 1024 * 1024 * 1024));

        // Test invalid formats
        assert_eq!(parse_memory_string("100MB"), None);
        assert_eq!(parse_memory_string("100 M"), None);
        assert_eq!(parse_memory_string("M100"), None);
        assert_eq!(parse_memory_string("-100M"), None);
        assert_eq!(parse_memory_string("100.5M"), None);
    }

    #[test]
    fn test_format_memory_boundaries() {
        // Test exact boundaries
        assert_eq!(format_memory(1023), "1023");
        assert_eq!(format_memory(1024), "1K");
        assert_eq!(format_memory(1024 * 1023), "1023K");
        assert_eq!(format_memory(1024 * 1024 - 1), "1023K");
        assert_eq!(format_memory(1024 * 1024), "1M");
        assert_eq!(format_memory(1024 * 1024 * 1023), "1023M");
        assert_eq!(format_memory(1024 * 1024 * 1024 - 1), "1023M");
        assert_eq!(format_memory(1024 * 1024 * 1024), "1G");
    }
}
