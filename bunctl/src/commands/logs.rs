use crate::cli::LogsArgs;
use bunctl_ipc::{IpcClient, IpcMessage, IpcResponse, SubscriptionType};
use colored::*;
use std::path::PathBuf;

pub async fn execute(args: LogsArgs) -> anyhow::Result<()> {
    if args.watch {
        execute_watch_mode(args).await
    } else {
        execute_once(args).await
    }
}

async fn execute_once(args: LogsArgs) -> anyhow::Result<()> {
    let socket_path = get_socket_path();

    let mut client = IpcClient::connect(&socket_path)
        .await
        .map_err(|_| anyhow::anyhow!("Daemon not running. No logs available."))?;

    let msg = IpcMessage::Logs {
        name: args.name.clone(),
        lines: args.lines,
    };

    client.send(&msg).await?;

    match client.recv().await? {
        IpcResponse::Data { data } => {
            if args.json {
                println!("{}", serde_json::to_string_pretty(&data)?);
            } else {
                display_logs(&data, &args)?;
            }
            Ok(())
        }
        IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        _ => Ok(()),
    }
}

async fn execute_watch_mode(args: LogsArgs) -> anyhow::Result<()> {
    let socket_path = get_socket_path();

    let mut client = IpcClient::connect(&socket_path)
        .await
        .map_err(|_| anyhow::anyhow!("Daemon not running. Cannot watch logs."))?;

    // Subscribe to log events
    let subscription = SubscriptionType::LogEvents {
        app_name: args.name.clone(),
    };

    let subscribe_msg = IpcMessage::Subscribe { subscription };
    client.send(&subscribe_msg).await?;

    // Wait for subscription confirmation
    match client.recv().await? {
        IpcResponse::Success { .. } => {
            if !args.json {
                println!("{}", "[Watching logs - Ctrl+C to exit]".cyan());
            }
        }
        IpcResponse::Error { message } => {
            return Err(anyhow::anyhow!(
                "Failed to subscribe to log events: {}",
                message
            ));
        }
        _ => {
            return Err(anyhow::anyhow!("Unexpected response from daemon"));
        }
    }

    // Get initial logs to show recent history
    let initial_logs_msg = IpcMessage::Logs {
        name: args.name.clone(),
        lines: if args.watch { 10 } else { args.lines },
    };

    client.send(&initial_logs_msg).await?;
    if let IpcResponse::Data { data } = client.recv().await? {
        if args.json {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let watch_data = serde_json::json!({
                "timestamp": timestamp.to_string(),
                "type": "initial",
                "logs": data
            });
            println!("{}", serde_json::to_string(&watch_data)?);
        } else {
            display_logs(&data, &args)?;
        }
    }

    // Now listen for real-time log events
    loop {
        match client.recv().await? {
            IpcResponse::Event { event_type, data } if event_type == "log_line" => {
                if args.json {
                    let watch_data = serde_json::json!({
                        "timestamp": chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
                        "type": "event",
                        "event": data
                    });
                    println!("{}", serde_json::to_string(&watch_data)?);
                } else {
                    // Extract log line data and display it
                    if let (Some(app), Some(stream), Some(line)) = (
                        data.get("app").and_then(|v| v.as_str()),
                        data.get("stream").and_then(|v| v.as_str()),
                        data.get("line").and_then(|v| v.as_str()),
                    ) {
                        let formatted_line = if stream == "stderr" {
                            format_error_line_simple(app, line)
                        } else {
                            format_output_line_simple(app, line)
                        };
                        println!("{}", formatted_line);
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
                // Ignore other event types
            }
        }
    }

    Ok(())
}

fn display_logs(data: &serde_json::Value, args: &LogsArgs) -> anyhow::Result<()> {
    // Check if colors should be disabled
    let use_colors = !args.no_colors && atty::is(atty::Stream::Stdout);

    if use_colors {
        colored::control::set_override(true);
    } else {
        colored::control::set_override(false);
    }

    let log_type = data
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("unknown");

    match log_type {
        "single" => {
            let app_name = data
                .get("app")
                .and_then(|a| a.as_str())
                .unwrap_or("unknown");
            let logs = data
                .get("logs")
                .ok_or_else(|| anyhow::anyhow!("No logs data"))?;

            display_single_app_logs(app_name, logs, args)?;
        }
        "all" => {
            let apps = data
                .get("apps")
                .and_then(|a| a.as_array())
                .ok_or_else(|| anyhow::anyhow!("No apps data"))?;

            display_all_apps_logs(apps, args)?;
        }
        _ => {
            // Fallback for old format
            if let Some(logs) = data.as_array() {
                for log in logs {
                    if let Some(line) = log.as_str() {
                        println!("{}", format_log_line(line, args));
                    }
                }
            }
        }
    }

    Ok(())
}

fn display_single_app_logs(
    app_name: &str,
    logs: &serde_json::Value,
    args: &LogsArgs,
) -> anyhow::Result<()> {
    let empty_vec = vec![];
    let errors = logs
        .get("errors")
        .and_then(|e| e.as_array())
        .unwrap_or(&empty_vec);
    let output = logs
        .get("output")
        .and_then(|o| o.as_array())
        .unwrap_or(&empty_vec);

    let app_header = format!("=== {} ===", app_name).bold().cyan();
    println!("{}", app_header);

    if args.errors_first && !errors.is_empty() {
        println!("\n{}", "Error logs:".red().bold());
        for error in errors {
            if let Some(line) = error.as_str() {
                println!("{}", format_error_line(line, args));
            }
        }

        if !output.is_empty() {
            println!("\n{}", "Output logs:".green().bold());
            for out in output {
                if let Some(line) = out.as_str() {
                    println!("{}", format_output_line(line, args));
                }
            }
        }
    } else {
        // Show all logs mixed (traditional format) with better grouping
        let mut all_logs: Vec<String> = Vec::new();

        for error in errors {
            if let Some(line) = error.as_str() {
                all_logs.push(line.to_string());
            }
        }

        for out in output {
            if let Some(line) = out.as_str() {
                all_logs.push(line.to_string());
            }
        }

        // Sort by timestamp if possible (basic sort by line content)
        all_logs.sort();

        // Group consecutive lines with same timestamp for better readability
        let mut prev_timestamp = String::new();
        let mut group_count = 0;

        for line in all_logs {
            // Extract timestamp for grouping
            let current_timestamp = if let Some(start) = line.find("] [") {
                if let Some(end) = line[start + 3..].find(']') {
                    line[start + 3..start + 3 + end].to_string()
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            // Add separator for different timestamps in long traces
            if !prev_timestamp.is_empty() && current_timestamp != prev_timestamp && group_count > 3
            {
                println!("{}", "  ...".dimmed());
                group_count = 0;
            }

            if current_timestamp == prev_timestamp {
                group_count += 1;
            } else {
                group_count = 1;
                prev_timestamp = current_timestamp;
            }

            println!("{}", format_log_line(&line, args));
        }
    }

    Ok(())
}

fn display_all_apps_logs(apps: &[serde_json::Value], args: &LogsArgs) -> anyhow::Result<()> {
    if apps.is_empty() {
        println!("{}", "No applications with logs found".yellow());
        return Ok(());
    }

    println!("{}", "=== All Applications Logs ===".bold().cyan());

    for (i, app_data) in apps.iter().enumerate() {
        if i > 0 {
            println!(); // Add spacing between apps
        }

        if let (Some(app_name), Some(logs)) =
            (app_data.get(0).and_then(|n| n.as_str()), app_data.get(1))
        {
            display_single_app_logs(app_name, logs, args)?;
        }
    }

    Ok(())
}

fn format_log_line(line: &str, _args: &LogsArgs) -> String {
    // Parse format: [app_name] [timestamp] [stream_type] message
    if line.contains("[stderr]") {
        format_error_line(line, _args)
    } else {
        format_output_line(line, _args)
    }
}

fn format_error_line(line: &str, _args: &LogsArgs) -> String {
    // Extract parts and format with red color for stderr
    if let Some(message_start) = line.rfind("] ") {
        let (prefix, message) = line.split_at(message_start + 2);

        // Handle indented stack trace lines better
        let formatted_message = if message.starts_with("    ") || message.starts_with("  ") {
            // Preserve indentation for stack traces but make them more readable
            message.red()
        } else {
            message.red().bold()
        };

        format!("{}{}", prefix.red().dimmed(), formatted_message)
    } else {
        line.red().to_string()
    }
}

fn format_output_line(line: &str, _args: &LogsArgs) -> String {
    // Extract parts and format with default color for stdout
    if let Some(message_start) = line.rfind("] ") {
        let (prefix, message) = line.split_at(message_start + 2);
        format!("{}{}", prefix.dimmed(), message)
    } else {
        line.to_string()
    }
}

fn format_error_line_simple(app: &str, line: &str) -> String {
    format!("[{}] {}", app.red(), line.red())
}

fn format_output_line_simple(app: &str, line: &str) -> String {
    format!("[{}] {}", app.cyan(), line)
}

fn get_socket_path() -> PathBuf {
    bunctl_core::config::default_socket_path()
}
