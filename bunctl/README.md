# bunctl - CLI Process Manager

A production-grade process manager for Bun applications, designed as a zero-overhead replacement for PM2. This is the main CLI binary for the bunctl workspace.

## Overview

`bunctl` is the command-line interface for managing Bun applications in production environments. It provides a comprehensive set of commands for process lifecycle management, monitoring, and logging with advanced features like exponential backoff, resource limits, and real-time event streaming.

**Key Features:**
- 🚀 **Zero-overhead design** - Single-threaded tokio runtime with minimal memory footprint
- 📊 **Advanced monitoring** - Real-time process metrics, resource usage, and event streaming  
- 🔄 **Intelligent restart policies** - Exponential backoff with configurable limits and strategies
- 📝 **Structured logging** - Async log management with rotation and compression
- 🎯 **Resource management** - Memory and CPU limits with OS-specific enforcement
- 🔧 **Multiple config formats** - Native bunctl.json, PM2-compatible ecosystem.config.js, and package.json
- 🖥️ **Cross-platform** - Native support for Linux and Windows
- 📡 **IPC communication** - Daemon architecture with real-time command and event handling

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/evobug/bunctl-rs
cd bunctl-rs

# Build the CLI binary
cargo build --release -p bunctl

# The binary will be available at target/release/bunctl
```

### Prerequisites

- **Rust**: 1.89.0 or later (2024 Edition)
- **Bun**: Latest version for running applications
- **Operating System**: Linux or Windows 10+

## Quick Start

### 1. Initialize a New Application

```bash
# Initialize with auto-detected settings
bunctl init

# Initialize with custom settings
bunctl init --name myapp \
           --entry src/server.ts \
           --port 3000 \
           --memory 1G \
           --cpu 75 \
           --autostart

# Generate PM2-compatible format
bunctl init --ecosystem
```

### 2. Start Your Application

```bash
# Start from configuration file
bunctl start

# Start specific app
bunctl start myapp

# Start with inline command
bunctl start myapp --command "bun run src/server.ts" --auto-restart
```

### 3. Monitor and Manage

```bash
# View status
bunctl status

# Watch real-time status changes
bunctl status --watch

# View logs
bunctl logs myapp

# Stream logs in real-time
bunctl logs myapp --watch

# Restart application
bunctl restart myapp

# Stop application
bunctl stop myapp
```

## Command Reference

### `init` - Initialize Application Configuration

Creates configuration files for your application with intelligent defaults.

```bash
bunctl init [OPTIONS]

Options:
  --name <NAME>              Application name (defaults to directory name)
  --entry <FILE>             Entry file to execute
  --script <FILE>            Script file (alias for --entry)
  --port <PORT>              Port number
  -d, --cwd <DIR>            Working directory
  --memory <SIZE>            Memory limit (e.g., 512M, 1G) [default: 512M]
  --cpu <PERCENT>            CPU limit percentage [default: 50]
  --runtime <RUNTIME>        Runtime (bun or node) [default: bun]
  --autostart                Enable auto-start on boot
  --instances <COUNT>        Number of instances [default: 1]
  --ecosystem                Generate ecosystem.config.js format
  --from-ecosystem <FILE>    Import from existing ecosystem.config.js
```

**Examples:**
```bash
# Basic initialization
bunctl init --name webapp

# Advanced configuration
bunctl init --name api-server \
           --entry src/app.ts \
           --port 8080 \
           --memory 2G \
           --cpu 80 \
           --autostart \
           --instances 4

# PM2-compatible format
bunctl init --ecosystem --name legacy-app
```

### `start` - Start Applications

Starts applications from configuration files or with inline parameters.

```bash
bunctl start [NAME] [OPTIONS]

Arguments:
  [NAME]                     Application name or "all"

Options:
  -c, --config <FILE>        Config file to load
  --command <CMD>            Command to execute (ad-hoc start)
  -s, --script <FILE>        Script file to execute
  -d, --cwd <DIR>            Working directory
  -e, --env <KEY=VALUE>      Environment variables
  --auto-restart             Auto-restart on exit
  --max-memory <BYTES>       Maximum memory limit
  --max-cpu <PERCENT>        Maximum CPU percentage
  --uid <UID>                User ID to run as (Unix)
  --gid <GID>                Group ID to run as (Unix)
```

**Examples:**
```bash
# Start from bunctl.json
bunctl start

# Start specific application
bunctl start api-server

# Ad-hoc start with inline config
bunctl start myapp --command "bun run server.ts" \
                   --env NODE_ENV=production \
                   --env PORT=3000 \
                   --auto-restart

# Start from PM2 config
bunctl start --config ecosystem.config.js
```

### `status` - View Application Status

Display detailed status information for running applications.

```bash
bunctl status [NAME] [OPTIONS]

Arguments:
  [NAME]                     Application name (optional)

Options:
  -j, --json                 Output as JSON
  -w, --watch                Watch mode - continuously update status
```

**Status Information Includes:**
- Process state (RUNNING, STOPPED, CRASHED, RESTARTING)
- Process ID and uptime
- Memory and CPU usage with limits
- Restart count and policy
- Environment variables
- Exit codes and error information

**Examples:**
```bash
# View all applications
bunctl status

# View specific application
bunctl status myapp

# JSON output for scripting
bunctl status --json

# Real-time monitoring
bunctl status --watch
```

### `logs` - View Application Logs

Access structured logs with filtering and real-time streaming capabilities.

```bash
bunctl logs [NAME] [OPTIONS]

Arguments:
  [NAME]                     Application name (shows all if not specified)

Options:
  -n, --lines <COUNT>        Number of lines to show [default: 20]
  -t, --timestamps           Show timestamps
  --errors-first             Show errors first, then output
  --no-colors               Disable colored output
  -j, --json                 Output as JSON
  -w, --watch                Watch mode - stream logs in real-time
```

**Examples:**
```bash
# View recent logs
bunctl logs myapp

# Stream logs in real-time
bunctl logs myapp --watch

# View last 100 lines with timestamps
bunctl logs myapp --lines 100 --timestamps

# JSON output for log processing
bunctl logs --json
```

### `restart` - Restart Applications

Gracefully restart running applications with configurable strategies.

```bash
bunctl restart <NAME> [OPTIONS]

Arguments:
  <NAME>                     Application name or "all"

Options:
  -p, --parallel             Parallel restart (for "all")
  -w, --wait <MS>            Wait time between restarts [default: 0]
```

**Examples:**
```bash
# Restart single application
bunctl restart myapp

# Restart all applications
bunctl restart all

# Parallel restart with delay
bunctl restart all --parallel --wait 1000
```

### `stop` - Stop Applications

Gracefully stop running applications with configurable timeouts.

```bash
bunctl stop <NAME> [OPTIONS]

Arguments:
  <NAME>                     Application name or "all"

Options:
  -t, --timeout <SECONDS>    Timeout for graceful stop [default: 10]
```

**Examples:**
```bash
# Stop application
bunctl stop myapp

# Stop with custom timeout
bunctl stop myapp --timeout 30

# Stop all applications
bunctl stop all
```

### `list` - List Applications

Display a simple list of all managed applications.

```bash
bunctl list
```

### `delete` - Remove Applications

Remove applications from management (stops them first if running).

```bash
bunctl delete <NAME> [OPTIONS]

Arguments:
  <NAME>                     Application name or "all"

Options:
  -f, --force                Force delete without confirmation
```

**Examples:**
```bash
# Delete with confirmation
bunctl delete myapp

# Force delete without prompt
bunctl delete myapp --force

# Delete all applications
bunctl delete all --force
```

## Configuration

### bunctl.json (Native Format)

The preferred configuration format with full feature support:

```json
{
  "apps": [
    {
      "name": "api-server",
      "command": "bun run src/server.ts",
      "args": [],
      "cwd": "/path/to/app",
      "env": {
        "NODE_ENV": "production",
        "PORT": "3000"
      },
      "auto_start": true,
      "restart_policy": "Always",
      "max_memory": 1073741824,
      "max_cpu_percent": 75.0,
      "stdout_log": "logs/api-server-out.log",
      "stderr_log": "logs/api-server-error.log",
      "log_max_size": 10485760,
      "log_max_files": 10,
      "stop_timeout": "10s",
      "kill_timeout": "5s",
      "backoff": {
        "max_attempts": 5,
        "base_delay_ms": 1000,
        "max_delay_ms": 30000,
        "multiplier": 2.0,
        "jitter": true,
        "exhausted_action": "Stop"
      }
    }
  ]
}
```

### ecosystem.config.js (PM2 Compatible)

For migration from PM2 or ecosystem compatibility:

```javascript
module.exports = {
  "apps": [
    {
      "name": "api-server",
      "script": "src/server.ts",
      "interpreter": "bun",
      "cwd": "/path/to/app",
      "instances": 1,
      "exec_mode": "fork",
      "env": {
        "NODE_ENV": "production",
        "PORT": "3000"
      },
      "max_memory_restart": "1G",
      "autorestart": true,
      "restart_delay": 1000,
      "max_restarts": 5,
      "error_file": "logs/api-server-error.log",
      "out_file": "logs/api-server-out.log"
    }
  ]
}
```

### package.json Integration

Simple configuration for single-app projects:

```json
{
  "name": "my-app",
  "bunctl": {
    "command": "bun run src/server.ts",
    "port": 3000,
    "memory": "512M",
    "autostart": true
  }
}
```

## Architecture

### Daemon Architecture

bunctl uses a client-daemon architecture for efficient process management:

```
┌─────────────────┐    IPC     ┌──────────────────┐
│   CLI Client    │◄──────────►│   Daemon Process │
│   (bunctl)      │   Socket    │   (background)   │
└─────────────────┘            └──────────────────┘
                                        │
                                        ▼
                               ┌──────────────────┐
                               │   Supervisor     │
                               │   (OS-specific)  │
                               └──────────────────┘
                                        │
                                        ▼
                               ┌──────────────────┐
                               │  Child Processes │
                               │  (your apps)     │
                               └──────────────────┘
```

**Key Components:**

1. **CLI Client**: Command-line interface that communicates with daemon
2. **Daemon Process**: Background service managing all applications
3. **Supervisor**: OS-specific process management and resource enforcement
4. **IPC Layer**: Named pipes (Windows) or Unix sockets for communication
5. **Log Manager**: Async logging with rotation and structured output

### Event System

Real-time event streaming for monitoring and integration:

```rust
// Event types
- ProcessStarted { app, pid }
- ProcessExited { app, status }
- ProcessCrashed { app, reason }
- ProcessRestarting { app, attempt, delay }
- StatusChange { app, state }
- ResourceLimitExceeded { app, resource, limit, current }
- HealthCheckFailed { app, reason }
- BackoffExhausted { app }
```

### Resource Management

OS-specific resource enforcement:

- **Linux**: cgroups v2 for memory and CPU limits
- **Windows**: Job Objects for process isolation and limits

## Advanced Features

### Exponential Backoff

Intelligent restart strategy with configurable parameters:

```json
{
  "backoff": {
    "max_attempts": 5,
    "base_delay_ms": 1000,
    "max_delay_ms": 30000,
    "multiplier": 2.0,
    "jitter": true,
    "exhausted_action": "Stop"
  }
}
```

**Backoff Sequence Example:**
- Attempt 1: 1s delay
- Attempt 2: 2s delay (+ jitter)
- Attempt 3: 4s delay (+ jitter)
- Attempt 4: 8s delay (+ jitter)
- Attempt 5: 16s delay (+ jitter)
- Exhausted: Stop or Remove based on policy

### Log Management

Advanced logging with structured output:

```bash
# Log format
[app-name] [2024-09-06 15:30:45.123] [stdout] Application started on port 3000
[app-name] [2024-09-06 15:30:45.124] [stderr] Warning: deprecated API usage

# Features
- Automatic log rotation
- Compression for old logs
- Structured JSON output
- Real-time streaming
- Configurable retention
```

### Health Monitoring

Built-in health checking and resource monitoring:

- Process lifecycle tracking
- Memory usage monitoring with limits
- CPU usage monitoring with limits
- Exit code analysis
- Crash detection and reporting

## Environment Variables

Configure bunctl behavior with environment variables:

```bash
# Logging level
export RUST_LOG=debug
export BUNCTL_LOG_LEVEL=info

# Force console logging for daemon
export BUNCTL_CONSOLE_LOG=1

# Custom socket path
export BUNCTL_SOCKET_PATH=/custom/path/bunctl.sock

# Tokio console integration (with 'console' feature)
export TOKIO_CONSOLE=127.0.0.1:6669
```

## Platform-Specific Features

### Linux
- **Resource Limits**: cgroups v2 integration
- **Process Isolation**: Secure process trees
- **File Watching**: inotify for config changes
- **Signal Handling**: Advanced signal management

### Windows
- **Job Objects**: Process isolation and limits
- **Named Pipes**: IPC communication
- **Service Integration**: Windows service compatibility
- **Process Trees**: Automatic child process management

## Development

### Building from Source

```bash
# Clone repository
git clone https://github.com/evobug/bunctl-rs
cd bunctl-rs

# Build CLI binary
cargo build --release -p bunctl

# Run tests
cargo test -p bunctl

# Development build
cargo build -p bunctl
```

### Testing

The crate includes comprehensive test coverage:

```bash
# Unit tests
cargo test -p bunctl

# Integration tests
cargo test -p bunctl --test integration_test

# CLI tests
cargo test -p bunctl --test cli_tests

# Test specific functionality
cargo test -p bunctl config_tests
```

### Contributing

1. **Fork the repository**
2. **Create feature branch**: `git checkout -b feature/new-feature`
3. **Write tests** for new functionality
4. **Ensure all tests pass**: `cargo test --all`
5. **Run clippy**: `cargo clippy --all-targets`
6. **Format code**: `cargo fmt --all`
7. **Submit pull request**

**Code Standards:**
- Follow Rust idioms and best practices
- Add comprehensive tests for new features
- Document public APIs with rustdoc
- Handle errors appropriately with `anyhow`
- Use structured logging with `tracing`

### Dependencies

**Key Dependencies:**
- `tokio` (1.47+): Async runtime
- `clap` (4.5+): CLI argument parsing
- `serde`/`serde_json`: Configuration serialization
- `tracing`/`tracing-subscriber`: Structured logging
- `bunctl-core`: Core process management traits
- `bunctl-supervisor`: OS-specific process supervision
- `bunctl-ipc`: Inter-process communication
- `bunctl-logging`: Async logging system

## Performance

### Benchmarks

**Resource Usage:**
- Memory: <5MB per daemon process
- CPU: <0.1% when idle
- Startup: <50ms to ready state
- Log Latency: <1ms p99 for log writes

**Scalability:**
- Supports 100+ applications per daemon
- Minimal overhead per managed process
- Event-driven architecture prevents polling
- Lock-free logging for high throughput

### Optimization Features

- **Single-threaded runtime**: Eliminates context switching overhead
- **Lock-free logging**: Atomic operations for log writes
- **OS-native events**: epoll (Linux) / IOCP (Windows) for efficient monitoring  
- **Zero-copy IPC**: Efficient inter-process communication
- **Lazy initialization**: Resources allocated on demand

## Troubleshooting

### Common Issues

**Daemon won't start:**
```bash
# Check if daemon is already running
bunctl status

# Check logs
tail -f ~/.local/share/bunctl/logs/daemon.log  # Linux
type "%LOCALAPPDATA%\bunctl\logs\daemon.log"   # Windows

# Force restart daemon
pkill bunctl  # Unix
taskkill /f /im bunctl.exe  # Windows
```

**Application won't start:**
```bash
# Check configuration
bunctl status myapp

# Verify command and working directory
bunctl status myapp --json | jq '.command, .cwd'

# Check application logs
bunctl logs myapp --lines 50
```

**High resource usage:**
```bash
# Monitor resource usage
bunctl status --watch

# Check for memory leaks
bunctl status myapp --json | jq '.memory_bytes, .max_memory'

# Review restart patterns
bunctl logs myapp | grep -i restart
```

### Debug Mode

Enable detailed logging for troubleshooting:

```bash
# Enable debug logging
export RUST_LOG=bunctl=debug,bunctl_core=debug
bunctl start myapp

# Console logging for daemon
export BUNCTL_CONSOLE_LOG=1
bunctl daemon
```

## License

This project is licensed under the MIT License. See the [LICENSE](../../LICENSE) file for details.

## Related Projects

- **[bunctl-core](../bunctl-core/)**: Core process management traits and configuration
- **[bunctl-supervisor](../bunctl-supervisor/)**: OS-specific process supervision
- **[bunctl-logging](../bunctl-logging/)**: Async logging system
- **[bunctl-ipc](../bunctl-ipc/)**: Inter-process communication

## Contributing

We welcome contributions! Please see our [Contributing Guide](../../CONTRIBUTING.md) for details on:
- Setting up the development environment
- Running tests and benchmarks
- Code style and conventions
- Submitting pull requests

## Support

- **GitHub Issues**: [Report bugs and request features](https://github.com/evobug/bunctl-rs/issues)
- **Discussions**: [Community support and questions](https://github.com/evobug/bunctl-rs/discussions)
- **Documentation**: [Full API documentation](https://docs.rs/bunctl)

---

**bunctl** - Production-grade process management for the modern web.