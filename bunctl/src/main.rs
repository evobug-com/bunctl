mod cli;
mod commands;
mod daemon;

use clap::Parser;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = cli::Cli::parse();

    match cli.command {
        cli::Command::Init(args) => commands::init::execute(args).await,
        cli::Command::Start(args) => commands::start::execute(args).await,
        cli::Command::Stop(args) => commands::stop::execute(args).await,
        cli::Command::Restart(args) => commands::restart::execute(args).await,
        cli::Command::Status(args) => commands::status::execute(args).await,
        cli::Command::Logs(args) => commands::logs::execute(args).await,
        cli::Command::List => commands::list::execute().await,
        cli::Command::Delete(args) => commands::delete::execute(args).await,
        cli::Command::Daemon(args) => daemon::run(args).await,
    }
}
