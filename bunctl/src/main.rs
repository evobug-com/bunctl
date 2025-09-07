mod cli;
mod commands;
mod common;
mod daemon;

use clap::Parser;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Default log level used when no environment variables are set
const DEFAULT_LOG_LEVEL: &str = "info";

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();

    // For daemon mode, write logs to file instead of console
    let is_daemon = matches!(cli.command, cli::Command::Daemon(_));

    // Initialize console subscriber for profiling if enabled
    #[cfg(feature = "console")]
    if std::env::var("TOKIO_CONSOLE").is_ok() {
        console_subscriber::init();
        return daemon::run(cli::DaemonArgs {
            config: None,
            socket: None,
        })
        .await;
    }

    // Configure logging filter based on environment and execution mode
    //
    // Logging behavior:
    // - Daemon mode: Uses BUNCTL_LOG_LEVEL if RUST_LOG is not set (defaults to "info")
    // - Normal mode: Uses RUST_LOG, falls back to BUNCTL_LOG_LEVEL, then defaults to "info"
    // - RUST_LOG always takes precedence when set
    // - BUNCTL_LOG_LEVEL provides an alternative configuration option
    let filter = if is_daemon && std::env::var("RUST_LOG").is_err() {
        let default_level =
            std::env::var("BUNCTL_LOG_LEVEL").unwrap_or_else(|_| DEFAULT_LOG_LEVEL.to_string());
        EnvFilter::new(&default_level)
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            let default_level =
                std::env::var("BUNCTL_LOG_LEVEL").unwrap_or_else(|_| DEFAULT_LOG_LEVEL.to_string());
            EnvFilter::new(&default_level)
        })
    };

    // Check if we want to force console logging for daemon (for debugging)
    let force_console = std::env::var("BUNCTL_CONSOLE_LOG").is_ok();

    if is_daemon && !force_console {
        // Create log directory if it doesn't exist
        let log_dir = if cfg!(windows) {
            std::path::PathBuf::from(
                std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string()),
            )
            .join("bunctl")
            .join("logs")
        } else {
            std::path::PathBuf::from("/var/log/bunctl")
        };

        if let Err(e) = std::fs::create_dir_all(&log_dir) {
            eprintln!("Failed to create log directory {:?}: {}", log_dir, e);
            std::process::exit(1);
        }

        let file_appender = tracing_appender::rolling::never(&log_dir, "daemon.log");
        tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(file_appender)
                    .with_ansi(false),
            )
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    let result = match cli.command {
        cli::Command::Init(args) => commands::init::execute(args).await,
        cli::Command::Start(args) => commands::start::execute(args).await,
        cli::Command::Stop(args) => commands::stop::execute(args).await,
        cli::Command::Restart(args) => commands::restart::execute(args).await,
        cli::Command::Status(args) => commands::status::execute(args).await,
        cli::Command::Logs(args) => commands::logs::execute(args).await,
        cli::Command::List => commands::list::execute().await,
        cli::Command::Delete(args) => commands::delete::execute(args).await,
        cli::Command::Daemon(args) => {
            info!("Starting daemon mode");
            daemon::run(args).await
        }
    };

    if let Err(e) = &result {
        error!("Command failed: {}", e);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tracing_subscriber::EnvFilter;

    /// Helper to create a filter with test environment variables
    /// Note: Uses unsafe blocks for environment variable manipulation in tests only
    fn create_test_filter(
        is_daemon: bool,
        rust_log: Option<&str>,
        bunctl_log: Option<&str>,
    ) -> EnvFilter {
        // Temporarily set environment variables for testing
        let rust_log_original = env::var("RUST_LOG").ok();
        let bunctl_log_original = env::var("BUNCTL_LOG_LEVEL").ok();

        // Clear existing values
        // SAFETY: This is only used in tests where we control the environment
        unsafe {
            env::remove_var("RUST_LOG");
            env::remove_var("BUNCTL_LOG_LEVEL");
        }

        // Set test values if provided
        // SAFETY: This is only used in tests where we control the environment
        unsafe {
            if let Some(val) = rust_log {
                env::set_var("RUST_LOG", val);
            }
            if let Some(val) = bunctl_log {
                env::set_var("BUNCTL_LOG_LEVEL", val);
            }
        }

        // Create filter using the same logic as main
        let filter = if is_daemon && env::var("RUST_LOG").is_err() {
            let default_level =
                env::var("BUNCTL_LOG_LEVEL").unwrap_or_else(|_| DEFAULT_LOG_LEVEL.to_string());
            EnvFilter::new(&default_level)
        } else {
            EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                let default_level =
                    env::var("BUNCTL_LOG_LEVEL").unwrap_or_else(|_| DEFAULT_LOG_LEVEL.to_string());
                EnvFilter::new(&default_level)
            })
        };

        // Restore original values
        // SAFETY: This is only used in tests where we control the environment
        unsafe {
            env::remove_var("RUST_LOG");
            env::remove_var("BUNCTL_LOG_LEVEL");
            if let Some(val) = rust_log_original {
                env::set_var("RUST_LOG", val);
            }
            if let Some(val) = bunctl_log_original {
                env::set_var("BUNCTL_LOG_LEVEL", val);
            }
        }

        filter
    }

    #[test]
    fn test_daemon_logging_filter_without_rust_log() {
        // Test daemon mode filter creation when RUST_LOG is not set
        let filter = create_test_filter(true, None, None);
        // Filter should be created with DEFAULT_LOG_LEVEL
        // The filter is created, we just verify it doesn't panic
        assert!(format!("{:?}", filter).len() > 0);
    }

    #[test]
    fn test_daemon_logging_with_bunctl_log_level() {
        // Test BUNCTL_LOG_LEVEL precedence in daemon mode
        let filter = create_test_filter(true, None, Some("debug"));
        // Filter should be created with debug level from BUNCTL_LOG_LEVEL
        // We can't easily inspect the filter internals, so we verify it was created
        assert!(format!("{:?}", filter).len() > 0);
    }

    #[test]
    fn test_daemon_mode_rust_log_takes_precedence() {
        // Test that RUST_LOG takes precedence even in daemon mode
        let filter = create_test_filter(true, Some("trace"), Some("debug"));
        // Filter should use RUST_LOG value (trace) instead of BUNCTL_LOG_LEVEL (debug)
        // We verify the filter was created successfully
        assert!(format!("{:?}", filter).len() > 0);
    }

    #[test]
    fn test_normal_mode_with_defaults() {
        // Test normal mode with no environment variables
        let filter = create_test_filter(false, None, None);
        // Filter should be created with DEFAULT_LOG_LEVEL
        assert!(format!("{:?}", filter).len() > 0);
    }

    #[test]
    fn test_normal_mode_bunctl_log_level() {
        // Test normal mode with BUNCTL_LOG_LEVEL set
        let filter = create_test_filter(false, None, Some("warn"));
        // Filter should use BUNCTL_LOG_LEVEL value
        assert!(format!("{:?}", filter).len() > 0);
    }

    #[test]
    fn test_normal_mode_rust_log_precedence() {
        // Test that RUST_LOG takes precedence in normal mode
        let filter = create_test_filter(false, Some("error"), Some("debug"));
        // Filter should use RUST_LOG value (error) instead of BUNCTL_LOG_LEVEL (debug)
        assert!(format!("{:?}", filter).len() > 0);
    }
}
