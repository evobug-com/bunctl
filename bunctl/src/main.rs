mod cli;
mod commands;
mod common;
mod daemon;

use clap::Parser;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

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

    // Create filter with appropriate default level for daemon mode
    let filter = if is_daemon && std::env::var("RUST_LOG").is_err() {
        let default_level =
            std::env::var("BUNCTL_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
        EnvFilter::new(&default_level)
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            let default_level =
                std::env::var("BUNCTL_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
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
