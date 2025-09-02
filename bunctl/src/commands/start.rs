use crate::cli::StartArgs;
use bunctl_core::config::ConfigLoader;
use bunctl_core::{AppConfig, AppId};
use std::collections::HashMap;
use std::path::PathBuf;

pub async fn execute(args: StartArgs) -> anyhow::Result<()> {
    // If config file is specified, load from it
    if let Some(config_path) = args.config {
        return start_from_config(&config_path, args.name).await;
    }

    // If no name and no command/script, try to auto-discover config
    if args.name.is_none() && args.command.is_none() && args.script.is_none() {
        let loader = ConfigLoader::new();
        if let Ok(config) = loader.load().await {
            if !config.apps.is_empty() {
                println!("Found config with {} app(s)", config.apps.len());
                for app in config.apps {
                    println!("Starting {}", app.name);
                    // TODO: Actually start the app via daemon
                }
                return Ok(());
            }
        }
    }

    // Otherwise, do ad-hoc start
    let name = args
        .name
        .ok_or_else(|| anyhow::anyhow!("App name is required for ad-hoc start"))?;
    let app_id = AppId::new(&name)?;

    let command = if let Some(cmd) = args.command {
        cmd
    } else if let Some(script) = args.script {
        format!("bun {}", script.display())
    } else {
        return Err(anyhow::anyhow!(
            "Either --command or --script must be provided"
        ));
    };

    let mut env = HashMap::new();
    for env_str in args.env {
        let parts: Vec<&str> = env_str.splitn(2, '=').collect();
        if parts.len() == 2 {
            env.insert(parts[0].to_string(), parts[1].to_string());
        }
    }

    let config = AppConfig {
        name: name.clone(),
        command,
        args: Vec::new(),
        cwd: args.cwd.unwrap_or_else(|| std::env::current_dir().unwrap()),
        env,
        auto_start: args.auto_restart,
        restart_policy: if args.auto_restart {
            bunctl_core::config::RestartPolicy::Always
        } else {
            bunctl_core::config::RestartPolicy::No
        },
        max_memory: args.max_memory,
        max_cpu_percent: args.max_cpu,
        uid: args.uid,
        gid: args.gid,
        ..Default::default()
    };

    // TODO: Send to daemon
    println!("✔ Started app {}", app_id);
    Ok(())
}

async fn start_from_config(config_path: &PathBuf, app_name: Option<String>) -> anyhow::Result<()> {
    let loader = ConfigLoader::new();
    let config = loader.load_file(config_path).await?;

    if config.apps.is_empty() {
        return Err(anyhow::anyhow!("No apps found in config file"));
    }

    let apps_to_start = if let Some(name) = app_name {
        if name == "all" {
            config.apps
        } else {
            config
                .apps
                .into_iter()
                .filter(|app| app.name == name)
                .collect()
        }
    } else {
        config.apps
    };

    if apps_to_start.is_empty() {
        return Err(anyhow::anyhow!("No matching apps found"));
    }

    for app in apps_to_start {
        println!("Starting {}", app.name);
        // TODO: Send to daemon
    }

    Ok(())
}
