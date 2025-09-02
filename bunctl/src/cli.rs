use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "bunctl")]
#[command(about = "Production-grade process manager for Bun applications", long_about = None)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Initialize a new application configuration
    Init(InitArgs),

    /// Start a new application or start a stopped application
    Start(StartArgs),

    /// Stop a running application
    Stop(StopArgs),

    /// Restart an application
    Restart(RestartArgs),

    /// Show status of applications
    Status(StatusArgs),

    /// View application logs
    Logs(LogsArgs),

    /// List all applications
    List,

    /// Delete an application
    Delete(DeleteArgs),

    /// Run daemon process (internal use)
    #[command(hide = true)]
    Daemon(DaemonArgs),
}

#[derive(Parser)]
pub struct InitArgs {
    /// Application name (defaults to directory name)
    #[arg(long)]
    pub name: Option<String>,

    /// Entry file to execute
    #[arg(long)]
    pub entry: Option<PathBuf>,

    /// Script file to execute (alias for entry)
    #[arg(short, long)]
    pub script: Option<PathBuf>,

    /// Port number
    #[arg(long)]
    pub port: Option<u16>,

    /// Working directory
    #[arg(short = 'd', long)]
    pub cwd: Option<PathBuf>,

    /// Memory limit (e.g., 512M, 1G)
    #[arg(long, default_value = "512M")]
    pub memory: String,

    /// CPU limit percentage
    #[arg(long, default_value = "50")]
    pub cpu: f32,

    /// Runtime (bun or node)
    #[arg(long, default_value = "bun")]
    pub runtime: String,

    /// Enable auto-start on boot
    #[arg(long)]
    pub autostart: bool,

    /// Number of instances (for cluster mode)
    #[arg(long, default_value = "1")]
    pub instances: usize,

    /// Generate ecosystem.config.js format
    #[arg(long)]
    pub ecosystem: bool,

    /// Generate from existing ecosystem.config.js
    #[arg(long)]
    pub from_ecosystem: Option<PathBuf>,
}

#[derive(Parser)]
pub struct StartArgs {
    /// Application name (or "all" to start all apps from config)
    pub name: Option<String>,

    /// Config file to load
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Command to execute (for ad-hoc start without config)
    #[arg(long)]
    pub command: Option<String>,

    /// Script file to execute (for ad-hoc start without config)
    #[arg(short = 's', long)]
    pub script: Option<PathBuf>,

    /// Working directory
    #[arg(short = 'd', long)]
    pub cwd: Option<PathBuf>,

    /// Environment variables (KEY=VALUE)
    #[arg(short, long)]
    pub env: Vec<String>,

    /// Auto-restart on exit
    #[arg(long)]
    pub auto_restart: bool,

    /// Maximum memory limit (bytes)
    #[arg(long)]
    pub max_memory: Option<u64>,

    /// Maximum CPU percentage
    #[arg(long)]
    pub max_cpu: Option<f32>,

    /// User ID to run as
    #[arg(long)]
    pub uid: Option<u32>,

    /// Group ID to run as
    #[arg(long)]
    pub gid: Option<u32>,
}

#[derive(Parser)]
pub struct StopArgs {
    /// Application name or "all"
    pub name: String,

    /// Timeout for graceful stop (seconds)
    #[arg(short, long, default_value = "10")]
    pub timeout: u64,
}

#[derive(Parser)]
pub struct RestartArgs {
    /// Application name or "all"
    pub name: String,

    /// Parallel restart
    #[arg(short, long)]
    pub parallel: bool,

    /// Wait time between restarts (milliseconds)
    #[arg(short, long, default_value = "0")]
    pub wait: u64,
}

#[derive(Parser)]
pub struct StatusArgs {
    /// Application name (optional)
    pub name: Option<String>,

    /// Output as JSON
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct LogsArgs {
    /// Application name
    pub name: String,

    /// Follow log output
    #[arg(short, long)]
    pub follow: bool,

    /// Number of lines to show
    #[arg(short, long, default_value = "20")]
    pub lines: usize,

    /// Show timestamps
    #[arg(short, long)]
    pub timestamps: bool,
}

#[derive(Parser)]
pub struct DeleteArgs {
    /// Application name or "all"
    pub name: String,

    /// Force delete without confirmation
    #[arg(short, long)]
    pub force: bool,
}

#[derive(Parser)]
pub struct DaemonArgs {
    /// Config file path
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Socket path
    #[arg(short, long)]
    pub socket: Option<PathBuf>,
}
