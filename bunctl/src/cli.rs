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
    /// Application name or "all" (auto-discovers from config if not provided)
    pub name: Option<String>,

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

    /// Watch mode - continuously update status
    #[arg(short, long)]
    pub watch: bool,
}

#[derive(Parser)]
pub struct LogsArgs {
    /// Application name (optional - shows all apps if not specified)
    pub name: Option<String>,

    /// Number of lines to show
    #[arg(short, long, default_value = "20")]
    pub lines: usize,

    /// Show timestamps
    #[arg(short, long)]
    pub timestamps: bool,

    /// Show errors first, then output (PM2 style)
    #[arg(long)]
    pub errors_first: bool,

    /// Disable colored output
    #[arg(long)]
    pub no_colors: bool,

    /// Output as JSON
    #[arg(short, long)]
    pub json: bool,

    /// Watch mode - continuously stream new logs in real-time
    #[arg(short, long)]
    pub watch: bool,
}

#[derive(Parser)]
pub struct DeleteArgs {
    /// Application name or "all"
    pub name: String,

    /// Force delete without confirmation
    #[arg(short, long)]
    pub force: bool,
}

#[derive(Parser, Debug)]
pub struct DaemonArgs {
    /// Config file path
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Socket path
    #[arg(short, long)]
    pub socket: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_cli_init_defaults() {
        let cli = Cli::parse_from(["bunctl", "init"]);
        match cli.command {
            Command::Init(args) => {
                assert_eq!(args.name, None);
                assert_eq!(args.entry, None);
                assert_eq!(args.script, None);
                assert_eq!(args.port, None);
                assert_eq!(args.cwd, None);
                assert_eq!(args.memory, "512M");
                assert_eq!(args.cpu, 50.0);
                assert_eq!(args.runtime, "bun");
                assert!(!args.autostart);
                assert_eq!(args.instances, 1);
                assert!(!args.ecosystem);
                assert_eq!(args.from_ecosystem, None);
            }
            _ => panic!("Expected Init command"),
        }
    }

    #[test]
    fn test_cli_init_with_all_args() {
        let cli = Cli::parse_from([
            "bunctl",
            "init",
            "--name",
            "test-app",
            "--entry",
            "server.ts",
            "--port",
            "3000",
            "--cwd",
            "/app",
            "--memory",
            "1G",
            "--cpu",
            "75",
            "--runtime",
            "node",
            "--autostart",
            "--instances",
            "4",
            "--ecosystem",
        ]);

        match cli.command {
            Command::Init(args) => {
                assert_eq!(args.name, Some("test-app".to_string()));
                assert_eq!(args.entry, Some(PathBuf::from("server.ts")));
                assert_eq!(args.port, Some(3000));
                assert_eq!(args.cwd, Some(PathBuf::from("/app")));
                assert_eq!(args.memory, "1G");
                assert_eq!(args.cpu, 75.0);
                assert_eq!(args.runtime, "node");
                assert!(args.autostart);
                assert_eq!(args.instances, 4);
                assert!(args.ecosystem);
            }
            _ => panic!("Expected Init command"),
        }
    }

    #[test]
    fn test_cli_start_simple() {
        let cli = Cli::parse_from(["bunctl", "start", "myapp"]);
        match cli.command {
            Command::Start(args) => {
                assert_eq!(args.name, Some("myapp".to_string()));
                assert_eq!(args.config, None);
                assert_eq!(args.command, None);
                assert_eq!(args.script, None);
                assert_eq!(args.cwd, None);
                assert!(args.env.is_empty());
                assert!(!args.auto_restart);
                assert_eq!(args.max_memory, None);
                assert_eq!(args.max_cpu, None);
                assert_eq!(args.uid, None);
                assert_eq!(args.gid, None);
            }
            _ => panic!("Expected Start command"),
        }
    }

    #[test]
    fn test_cli_start_with_config() {
        let cli = Cli::parse_from(["bunctl", "start", "--config", "app.json", "myapp"]);
        match cli.command {
            Command::Start(args) => {
                assert_eq!(args.name, Some("myapp".to_string()));
                assert_eq!(args.config, Some(PathBuf::from("app.json")));
            }
            _ => panic!("Expected Start command"),
        }
    }

    #[test]
    fn test_cli_stop_defaults() {
        let cli = Cli::parse_from(["bunctl", "stop", "myapp"]);
        match cli.command {
            Command::Stop(args) => {
                assert_eq!(args.name, Some("myapp".to_string()));
                assert_eq!(args.timeout, 10);
            }
            _ => panic!("Expected Stop command"),
        }
    }

    #[test]
    fn test_cli_restart_defaults() {
        let cli = Cli::parse_from(["bunctl", "restart", "myapp"]);
        match cli.command {
            Command::Restart(args) => {
                assert_eq!(args.name, "myapp");
                assert!(!args.parallel);
                assert_eq!(args.wait, 0);
            }
            _ => panic!("Expected Restart command"),
        }
    }

    #[test]
    fn test_cli_status_no_args() {
        let cli = Cli::parse_from(["bunctl", "status"]);
        match cli.command {
            Command::Status(args) => {
                assert_eq!(args.name, None);
                assert!(!args.json);
                assert!(!args.watch);
            }
            _ => panic!("Expected Status command"),
        }
    }

    #[test]
    fn test_cli_logs_defaults() {
        let cli = Cli::parse_from(["bunctl", "logs"]);
        match cli.command {
            Command::Logs(args) => {
                assert_eq!(args.name, None);
                assert_eq!(args.lines, 20);
                assert!(!args.timestamps);
                assert!(!args.errors_first);
                assert!(!args.no_colors);
                assert!(!args.json);
                assert!(!args.watch);
            }
            _ => panic!("Expected Logs command"),
        }
    }

    #[test]
    fn test_cli_list() {
        let cli = Cli::parse_from(["bunctl", "list"]);
        match cli.command {
            Command::List => {}
            _ => panic!("Expected List command"),
        }
    }

    #[test]
    fn test_cli_delete_simple() {
        let cli = Cli::parse_from(["bunctl", "delete", "myapp"]);
        match cli.command {
            Command::Delete(args) => {
                assert_eq!(args.name, "myapp");
                assert!(!args.force);
            }
            _ => panic!("Expected Delete command"),
        }
    }

    #[test]
    fn test_cli_daemon_no_args() {
        let cli = Cli::parse_from(["bunctl", "daemon"]);
        match cli.command {
            Command::Daemon(args) => {
                assert_eq!(args.config, None);
                assert_eq!(args.socket, None);
            }
            _ => panic!("Expected Daemon command"),
        }
    }
}
