use crate::cli::StatusArgs;
use bunctl_ipc::{IpcClient, IpcMessage, IpcResponse, SubscriptionType};
use colored::*;
use std::path::PathBuf;

pub async fn execute(args: StatusArgs) -> anyhow::Result<()> {
    if args.watch {
        execute_watch_mode(args).await
    } else {
        execute_once(args).await
    }
}

async fn execute_once(args: StatusArgs) -> anyhow::Result<()> {
    let socket_path = get_socket_path();
    
    let mut client = match IpcClient::connect(&socket_path).await {
        Ok(client) => client,
        Err(_) => {
            if args.json {
                println!("[]");
            } else {
                println!("No daemon running");
            }
            return Ok(());
        }
    };
    
    let msg = IpcMessage::Status {
        name: args.name.clone(),
    };
    
    client.send(&msg).await?;
    
    match client.recv().await? {
        IpcResponse::Data { data } => {
            if args.json {
                println!("{}", serde_json::to_string_pretty(&data)?);
            } else {
                display_status(&data)?;
            }
            Ok(())
        }
        IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        _ => Ok(()),
    }
}

async fn execute_watch_mode(args: StatusArgs) -> anyhow::Result<()> {
    let socket_path = get_socket_path();
    
    let mut client = IpcClient::connect(&socket_path).await
        .map_err(|_| anyhow::anyhow!("Daemon not running. Cannot watch status."))?;
    
    // Subscribe to status events
    let subscription = SubscriptionType::StatusEvents {
        app_name: args.name.clone(),
    };
    
    let subscribe_msg = IpcMessage::Subscribe { subscription };
    client.send(&subscribe_msg).await?;
    
    // Wait for subscription confirmation
    match client.recv().await? {
        IpcResponse::Success { .. } => {
            if !args.json {
                println!("{}", "[Watching status - Ctrl+C to exit]".cyan());
            }
        }
        IpcResponse::Error { message } => {
            return Err(anyhow::anyhow!("Failed to subscribe to status events: {}", message));
        }
        _ => {
            return Err(anyhow::anyhow!("Unexpected response from daemon"));
        }
    }
    
    // Get initial status to show current state
    let initial_status_msg = IpcMessage::Status {
        name: args.name.clone(),
    };
    
    client.send(&initial_status_msg).await?;
    match client.recv().await? {
        IpcResponse::Data { data } => {
            if args.json {
                let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
                let watch_data = serde_json::json!({
                    "timestamp": timestamp.to_string(),
                    "type": "initial",
                    "data": data
                });
                println!("{}", serde_json::to_string(&watch_data)?);
            } else {
                // Clear screen and show initial status
                print!("\x1B[2J\x1B[1;1H");
                let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                println!("{} {}", "Last updated:".dimmed(), timestamp.white());
                println!();
                display_status(&data)?;
                println!("{}", "Press Ctrl+C to exit watch mode".dimmed());
            }
        }
        IpcResponse::Error { message } => {
            if args.json {
                let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
                let error_data = serde_json::json!({
                    "timestamp": timestamp.to_string(),
                    "error": message
                });
                println!("{}", serde_json::to_string(&error_data)?);
            } else {
                println!("{}: {}", "Error".red(), message);
            }
            return Ok(());
        }
        _ => {}
    }
    
    // Now listen for real-time status events
    loop {
        match client.recv().await? {
            IpcResponse::Event { event_type, data } => {
                if args.json {
                    let watch_data = serde_json::json!({
                        "timestamp": chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
                        "type": "event",
                        "event_type": event_type,
                        "event": data
                    });
                    println!("{}", serde_json::to_string(&watch_data)?);
                } else {
                    // For interactive mode, show each event as it comes without clearing screen
                    if matches!(event_type.as_str(), "status_change" | "process_started" | "process_exited" | "process_crashed" | "process_restarting") {
                        let timestamp = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
                        let event_desc = format_event_description(&event_type, &data);
                        
                        // Show event with timestamp
                        print!("[{}] {} - ", timestamp.dimmed(), event_desc.yellow());
                        
                        // Show state change based on event type and data
                        if let Some(app_name) = data.get("app").and_then(|v| v.as_str()) {
                            if let Some(state) = data.get("state").and_then(|v| v.as_str()) {
                                let state_display = match state {
                                    "crashed" => "CRASHED".red(),
                                    "starting" => "STARTING".yellow(), 
                                    "running" => "RUNNING".green(),
                                    "stopped" => "STOPPED".red(),
                                    "backoff_exhausted" => "BACKOFF EXHAUSTED".red(),
                                    _ => state.white(),
                                };
                                println!("{} → {}", app_name.cyan(), state_display);
                            } else if event_type == "process_started" {
                                if let Some(pid) = data.get("pid").and_then(|v| v.as_u64()) {
                                    println!("{} → {} (PID {})", app_name.cyan(), "RUNNING".green(), pid);
                                } else {
                                    println!("{} → {}", app_name.cyan(), "RUNNING".green());
                                }
                            } else if event_type == "process_exited" {
                                if let Some(exit_code) = data.get("exit_code") {
                                    println!("{} → {} (code {})", app_name.cyan(), "EXITED".red(), exit_code);
                                } else {
                                    println!("{} → {}", app_name.cyan(), "EXITED".red());
                                }
                            } else if event_type == "process_restarting" {
                                if let Some(attempt) = data.get("attempt").and_then(|v| v.as_u64()) {
                                    println!("{} → {} (attempt {})", app_name.cyan(), "RESTARTING".yellow(), attempt);
                                } else {
                                    println!("{} → {}", app_name.cyan(), "RESTARTING".yellow());
                                }
                            } else {
                                println!("{}", app_name.cyan());
                            }
                        } else {
                            println!();
                        }
                    }
                }
            }
            IpcResponse::Error { message } => {
                if args.json {
                    let error_data = serde_json::json!({
                        "timestamp": chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
                        "error": message
                    });
                    println!("{}", serde_json::to_string(&error_data)?);
                } else {
                    println!("{}: {}", "Error".red(), message);
                }
                break;
            }
            _ => {
                // Ignore other response types
            }
        }
    }
    
    Ok(())
}

fn display_status(data: &serde_json::Value) -> anyhow::Result<()> {
    if let Some(arr) = data.as_array() {
        if arr.is_empty() {
            println!("{}", "No applications running".yellow());
            return Ok(());
        }
        
        // Display beautiful header
        println!();
        println!("{}", "━━━ Bun Applications Status ━━━".bold().cyan());
        println!();
        
        for app in arr {
            display_app_status(app)?;
        }
    } else {
        // Single app status
        println!();
        println!("{}", "━━━ Application Status ━━━".bold().cyan());
        println!();
        display_app_status(data)?;
    }
    
    Ok(())
}

fn display_app_status(app: &serde_json::Value) -> anyhow::Result<()> {
    let name = app.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
    let state = app.get("state").and_then(|v| v.as_str()).unwrap_or("unknown");
    let pid = app.get("pid").and_then(|v| v.as_u64());
    let restarts = app.get("restarts").and_then(|v| v.as_u64()).unwrap_or(0);
    let memory_bytes = app.get("memory_bytes").and_then(|v| v.as_u64());
    let cpu_percent = app.get("cpu_percent").and_then(|v| v.as_f64());
    let auto_start = app.get("auto_start").and_then(|v| v.as_bool()).unwrap_or(false);
    let command = app.get("command").and_then(|v| v.as_str()).unwrap_or("");
    let args = app.get("args").and_then(|v| v.as_array()).map(|arr| {
        arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(" ")
    }).unwrap_or_default();
    let cwd = app.get("cwd").and_then(|v| v.as_str()).unwrap_or("");
    let restart_policy = app.get("restart_policy").and_then(|v| v.as_str()).unwrap_or("");
    let uptime_seconds = app.get("uptime_seconds").and_then(|v| v.as_u64());
    let last_exit_code = app.get("last_exit_code").and_then(|v| v.as_i64());
    let max_memory = app.get("max_memory").and_then(|v| v.as_u64());
    let max_cpu_percent = app.get("max_cpu_percent").and_then(|v| v.as_f64());
    let max_restart_attempts = app.get("max_restart_attempts").and_then(|v| v.as_u64());
    let backoff_exhausted = app.get("backoff_exhausted").and_then(|v| v.as_bool()).unwrap_or(false);
    
    // Format the app name with autostart indicator
    let boot_indicator = if auto_start { "[boot]".green() } else { "[manual]".dimmed() };
    let app_header = format!("  {} {}", name.bold().white(), boot_indicator);
    println!("{}", app_header);
    
    // Format status with colored indicator
    let (status_icon, status_text, status_color) = format_status_display(state);
    println!("    Status:  {} {}", status_icon, status_text.color(status_color));
    
    // PID
    if let Some(pid) = pid {
        println!("    PID:     {}", pid.to_string().white());
    } else {
        println!("    PID:     {}", "N/A".dimmed());
    }
    
    // Uptime
    if let Some(uptime) = uptime_seconds {
        println!("    Uptime:  {}", format_uptime(uptime).cyan());
    }
    
    // Memory usage and limits
    if let Some(memory) = memory_bytes {
        let memory_display = format_memory_size(memory);
        if let Some(limit) = max_memory {
            let usage_percent = (memory as f64 / limit as f64 * 100.0) as u32;
            let limit_display = format_memory_size(limit);
            println!("    Memory:  {} / {} ({}%)", 
                memory_display.green(), 
                limit_display.dimmed(), 
                format!("{}", usage_percent).color(if usage_percent > 80 { Color::Red } else { Color::Yellow })
            );
        } else {
            println!("    Memory:  {}", memory_display.green());
        }
    } else if let Some(limit) = max_memory {
        println!("    Memory:  {} / {} {}", "N/A".dimmed(), format_memory_size(limit).dimmed(), "(limit)".dimmed());
    } else {
        println!("    Memory:  {}", "N/A".dimmed());
    }
    
    // CPU usage and limits
    if let Some(cpu) = cpu_percent {
        if let Some(limit) = max_cpu_percent {
            println!("    CPU:     {:.1}% / {:.1}% {}", 
                cpu, 
                limit, 
                if cpu > limit { "(over limit)".red() } else { "(limit)".dimmed() }
            );
        } else {
            println!("    CPU:     {}", format!("{:.1}%", cpu).yellow());
        }
    } else if let Some(limit) = max_cpu_percent {
        println!("    CPU:     {} / {:.1}% {}", "N/A".dimmed(), limit, "(limit)".dimmed());
    } else {
        println!("    CPU:     {}", "N/A".dimmed());
    }
    
    // Command line
    let full_command = if args.is_empty() {
        command.to_string()
    } else {
        format!("{} {}", command, args)
    };
    if !full_command.is_empty() {
        println!("    Command: {}", full_command.white());
    }
    
    // Working directory
    if !cwd.is_empty() {
        println!("    Dir:     {}", cwd.blue());
    }
    
    // Restart policy
    if !restart_policy.is_empty() {
        let policy_color = match restart_policy {
            "Always" => Color::Green,
            "OnFailure" => Color::Yellow,
            "No" => Color::Red,
            _ => Color::White,
        };
        println!("    Restart: {}", restart_policy.to_lowercase().color(policy_color));
    }
    
    // Environment variables (important ones)
    if let Some(env) = app.get("env").and_then(|v| v.as_object()) {
        for (key, value) in env {
            if let Some(val_str) = value.as_str() {
                println!("    {}: {}", 
                    key.green(), 
                    if key.to_uppercase().contains("PASSWORD") || key.to_uppercase().contains("SECRET") || key.to_uppercase().contains("TOKEN") {
                        "***".dimmed().to_string()
                    } else {
                        val_str.white().to_string()
                    }
                );
            }
        }
    }
    
    // Restarts and exit info with limits
    if restarts > 0 || max_restart_attempts.is_some() {
        let mut restart_parts = Vec::new();
        
        if let Some(limit) = max_restart_attempts {
            let restart_display = format!("{}/{}", restarts, limit);
            let restart_color = if backoff_exhausted {
                Color::Red
            } else if restarts >= (limit as f64 * 0.8) as u64 {
                Color::Yellow
            } else {
                Color::Green
            };
            restart_parts.push(restart_display.color(restart_color).to_string());
        } else {
            restart_parts.push(restarts.to_string().red().to_string());
        }
        
        if let Some(exit_code) = last_exit_code {
            let exit_display = format!("(last exit: {})", 
                if exit_code == 0 { exit_code.to_string().green() } else { exit_code.to_string().red() }
            );
            restart_parts.push(exit_display);
        }
        
        if backoff_exhausted {
            restart_parts.push("EXHAUSTED".red().bold().to_string());
        }
        
        println!("    Restarts: {}", restart_parts.join(" "));
    }
    
    println!(); // Spacing between apps
    
    Ok(())
}

fn format_status_display(state: &str) -> (&'static str, String, Color) {
    // Parse the debug state format and convert to nice display
    if state == "Running" {
        ("●", "RUNNING".to_string(), Color::Green)
    } else if state == "Starting" {
        ("◐", "STARTING".to_string(), Color::Yellow)
    } else if state == "Stopping" {
        ("◑", "STOPPING".to_string(), Color::Yellow)
    } else if state == "Stopped" {
        ("○", "STOPPED".to_string(), Color::Red)
    } else if state == "Crashed" {
        ("✗", "CRASHED".to_string(), Color::Red)
    } else if state.starts_with("Backoff") {
        ("◔", "RESTARTING".to_string(), Color::Yellow)
    } else {
        ("?", state.to_string(), Color::White)
    }
}

fn format_memory_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn format_uptime(seconds: u64) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = MINUTE * 60;
    const DAY: u64 = HOUR * 24;
    
    if seconds >= DAY {
        let days = seconds / DAY;
        let hours = (seconds % DAY) / HOUR;
        format!("{}d {}h", days, hours)
    } else if seconds >= HOUR {
        let hours = seconds / HOUR;
        let minutes = (seconds % HOUR) / MINUTE;
        format!("{}h {}m", hours, minutes)
    } else if seconds >= MINUTE {
        let minutes = seconds / MINUTE;
        let secs = seconds % MINUTE;
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", seconds)
    }
}

fn format_event_description(event_type: &str, data: &serde_json::Value) -> String {
    match event_type {
        "process_started" => {
            if let Some(app) = data.get("app").and_then(|v| v.as_str()) {
                format!("Process {} started", app)
            } else {
                "Process started".to_string()
            }
        }
        "process_exited" => {
            if let (Some(app), Some(exit_code)) = (
                data.get("app").and_then(|v| v.as_str()),
                data.get("exit_code").and_then(|v| v.as_i64()),
            ) {
                format!("Process {} exited (code {})", app, exit_code)
            } else {
                "Process exited".to_string()
            }
        }
        "process_crashed" => {
            if let Some(app) = data.get("app").and_then(|v| v.as_str()) {
                format!("Process {} crashed", app)
            } else {
                "Process crashed".to_string()
            }
        }
        "process_restarting" => {
            if let (Some(app), Some(attempt)) = (
                data.get("app").and_then(|v| v.as_str()),
                data.get("attempt").and_then(|v| v.as_u64()),
            ) {
                format!("Process {} restarting (attempt {})", app, attempt)
            } else {
                "Process restarting".to_string()
            }
        }
        "status_change" => {
            if let (Some(app), Some(state)) = (
                data.get("app").and_then(|v| v.as_str()),
                data.get("state").and_then(|v| v.as_str()),
            ) {
                format!("Process {} changed to {}", app, state)
            } else {
                "Status changed".to_string()
            }
        }
        _ => format!("Event: {}", event_type),
    }
}

fn get_socket_path() -> PathBuf {
    bunctl_core::config::default_socket_path()
}
