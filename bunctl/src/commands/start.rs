use crate::cli::StartArgs;
use bunctl_core::config::ConfigLoader;
use bunctl_core::{AppConfig, AppId};
use bunctl_ipc::{IpcClient, IpcMessage, IpcResponse};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub async fn execute(args: StartArgs) -> anyhow::Result<()> {
    // If config file is specified, load from it
    if let Some(config_path) = args.config {
        return start_from_config(&config_path, args.name).await;
    }

    // If no name and no command/script, try to auto-discover config
    if args.name.is_none() && args.command.is_none() && args.script.is_none() {
        let loader = ConfigLoader::new();
        if let Ok(config) = loader.load().await
            && !config.apps.is_empty()
        {
            println!("━━━ Starting Bun Applications ━━━");
            println!();
            
            for app in config.apps {
                println!("  {} [{}]", 
                    app.name,
                    match app.restart_policy {
                        bunctl_core::config::RestartPolicy::Always => "auto",
                        bunctl_core::config::RestartPolicy::OnFailure => "onfailure", 
                        bunctl_core::config::RestartPolicy::UnlessStopped => "unless-stopped",
                        bunctl_core::config::RestartPolicy::No => "manual",
                    }
                );
                println!("    Command: {} {}", app.command, app.args.join(" "));
                println!("    Dir:     {}", app.cwd.display());
                if let Some(memory) = app.max_memory {
                    println!("    Memory:  {} MB (limit)", memory / 1024 / 1024);
                }
                if let Some(cpu) = app.max_cpu_percent {
                    println!("    CPU:     {}% (limit)", cpu);
                }
                
                // Show key environment variables
                let important_env_vars = ["NODE_ENV", "PORT", "DATABASE_URL"];
                for env_var in &important_env_vars {
                    if let Some(value) = app.env.get(*env_var) {
                        let display_value = if *env_var == "DATABASE_URL" {
                            "[hidden]".to_string()
                        } else {
                            value.clone()
                        };
                        println!("    {}: {}", env_var, display_value);
                    }
                }
                
                print!("    Status:  ");
                match send_to_daemon(app).await {
                    Ok(_) => println!("● STARTING"),
                    Err(e) => {
                        println!("○ FAILED ({})", e);
                        return Err(e);
                    }
                }
                println!();
            }
            return Ok(());
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

    println!("━━━ Starting Bun Application ━━━");
    println!();
    
    println!("  {} [{}]", 
        app_id,
        match config.restart_policy {
            bunctl_core::config::RestartPolicy::Always => "auto",
            bunctl_core::config::RestartPolicy::OnFailure => "onfailure", 
            bunctl_core::config::RestartPolicy::UnlessStopped => "unless-stopped",
            bunctl_core::config::RestartPolicy::No => "manual",
        }
    );
    println!("    Command: {} {}", config.command, config.args.join(" "));
    println!("    Dir:     {}", config.cwd.display());
    if let Some(memory) = config.max_memory {
        println!("    Memory:  {} MB (limit)", memory / 1024 / 1024);
    }
    if let Some(cpu) = config.max_cpu_percent {
        println!("    CPU:     {}% (limit)", cpu);
    }
    
    // Show key environment variables
    let important_env_vars = ["NODE_ENV", "PORT", "DATABASE_URL"];
    for env_var in &important_env_vars {
        if let Some(value) = config.env.get(*env_var) {
            let display_value = if *env_var == "DATABASE_URL" {
                "[hidden]".to_string()
            } else {
                value.clone()
            };
            println!("    {}: {}", env_var, display_value);
        }
    }
    
    print!("    Status:  ");
    match send_to_daemon(config).await {
        Ok(_) => println!("● STARTING"),
        Err(e) => {
            println!("○ FAILED ({})", e);
            return Err(e);
        }
    }
    
    Ok(())
}

async fn start_from_config(config_path: &Path, app_name: Option<String>) -> anyhow::Result<()> {
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

    println!("━━━ Starting Bun Application{} ━━━", 
        if apps_to_start.len() == 1 { "" } else { "s" }
    );
    println!();
    
    for app in apps_to_start {
        println!("  {} [{}]", 
            app.name,
            match app.restart_policy {
                bunctl_core::config::RestartPolicy::Always => "auto",
                bunctl_core::config::RestartPolicy::OnFailure => "onfailure", 
                bunctl_core::config::RestartPolicy::UnlessStopped => "unless-stopped",
                bunctl_core::config::RestartPolicy::No => "manual",
            }
        );
        println!("    Command: {} {}", app.command, app.args.join(" "));
        println!("    Dir:     {}", app.cwd.display());
        if let Some(memory) = app.max_memory {
            println!("    Memory:  {} MB (limit)", memory / 1024 / 1024);
        }
        if let Some(cpu) = app.max_cpu_percent {
            println!("    CPU:     {}% (limit)", cpu);
        }
        
        // Show key environment variables
        let important_env_vars = ["NODE_ENV", "PORT", "DATABASE_URL"];
        for env_var in &important_env_vars {
            if let Some(value) = app.env.get(*env_var) {
                let display_value = if *env_var == "DATABASE_URL" {
                    "[hidden]".to_string()
                } else {
                    value.clone()
                };
                println!("    {}: {}", env_var, display_value);
            }
        }
        
        print!("    Status:  ");
        match send_to_daemon(app).await {
            Ok(_) => println!("● STARTING"),
            Err(e) => {
                println!("○ FAILED ({})", e);
                return Err(e);
            }
        }
        println!();
    }

    Ok(())
}

async fn send_to_daemon(config: AppConfig) -> anyhow::Result<()> {
    let socket_path = get_socket_path();
    
    // Try to connect to existing daemon
    let mut client = match IpcClient::connect(&socket_path).await {
        Ok(client) => client,
        Err(_) => {
            // Daemon not running, start it
            println!("🔧 Starting daemon...");
            start_daemon().await?;
            
            // Wait for daemon to be ready, with retries
            print!("⏳ Waiting for daemon to initialize");
            let mut retry_count = 0;
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                print!(".");
                std::io::Write::flush(&mut std::io::stdout()).unwrap_or(());
                
                match IpcClient::connect(&socket_path).await {
                    Ok(client) => {
                        println!(" ✅");
                        break client;
                    },
                    Err(_) if retry_count < 10 => {
                        retry_count += 1;
                        continue;
                    }
                    Err(e) => {
                        println!(" ❌");
                        return Err(anyhow::anyhow!("Failed to connect to daemon after starting: {}", e));
                    }
                }
            }
        }
    };
    
    let config_json = serde_json::to_string(&config)?;
    let msg = IpcMessage::Start {
        name: config.name.clone(),
        config: config_json,
    };
    
    client.send(&msg).await?;
    
    match client.recv().await? {
        IpcResponse::Success { message: _ } => Ok(()),
        IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        _ => Ok(()),
    }
}

fn get_socket_path() -> PathBuf {
    bunctl_core::config::default_socket_path()
}

async fn start_daemon() -> anyhow::Result<()> {
    use std::process::Command;
    
    let exe = std::env::current_exe()?;
    
    #[cfg(windows)]
    {
        // Use Windows-specific process creation to detach the daemon
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        
        Command::new(&exe)
            .arg("daemon")
            .creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS | CREATE_NO_WINDOW)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
    }
    
    #[cfg(unix)]
    {
        Command::new(&exe)
            .arg("daemon")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
    }
    
    // Wait longer for daemon to initialize
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    
    Ok(())
}
