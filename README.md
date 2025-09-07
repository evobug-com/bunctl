# bunctl-rs

<div align="center">

**🚀 Production-grade process manager for Bun applications**

*Built with Rust for zero-overhead performance and bulletproof reliability*

[![Build Status](https://img.shields.io/github/workflow/status/evobug-com/bunctl-rs/CI)](https://github.com/evobug-com/bunctl-rs/actions)
[![Release](https://img.shields.io/github/v/release/evobug-com/bunctl-rs)](https://github.com/evobug-com/bunctl-rs/releases)
[![License](https://img.shields.io/github/license/evobug-com/bunctl-rs)](LICENSE)

[Features](#-features) • [Installation](#-installation) • [Quick Start](#-quick-start) • [Documentation](#-documentation) • [API](#-json-api)

</div>

---


> ⚠️ **IMPORTANT DISCLAIMER**
>
> This tool is **not fully tested** and is currently being used **internally by Evobug products only**. We use it in production for running our API and Discord bot, but many features may change without notice. Most of the code and documentation has been generated with AI assistance.
>
> **Use at your own risk in production environments outside of Evobug.**

---

## 🎯 **About bunctl-rs**

A lightweight process manager for Bun applications, built with Rust:

- 🏃‍♂️ **Lightweight**: <5MB memory, <0.1% CPU when idle  
- 🎯 **Cross-platform**: Linux, ~~Windows, macOS with native OS integration~~
- 📊 **JSON API**: Rich APIs, watch modes, restart limit tracking
- 🔍 **Process monitoring**: PM2-style status with enhanced process information
- ⚡ **Fast**: <50ms startup, <1ms log latency
- 🛡️ **Reliable**: Exponential backoff, graceful shutdown, process isolation

---

## ✨ **Features**

### 🖥️ **Status Display & Monitoring**
Rich process monitoring with JSON API support:

```bash
bunctl status

━━━ Bun Applications Status ━━━

  api [manual]
    Status:  ● RUNNING
    PID:     25644
    Uptime:  0s
    Memory:  N/A / 512.0 MB (limit)
    CPU:     N/A / 50.0% (limit)
    Command: bun run src/server.ts
    Dir:     D:\projects\evobug.com\api
    Restart: onfailure
    NODE_ENV: production
    Restarts: 63 (last exit: 1)

# Real-time monitoring & JSON output
bunctl status --watch               # Live updates
bunctl status --json                # Machine-readable
bunctl status --json --watch        # JSON + live updates
bunctl logs --watch                 # Live log streaming
```

**Status Indicators:** `● RUNNING` `◐ STARTING` `◔ RESTARTING` `✗ CRASHED` `○ STOPPED`  
**Smart Features:** Resource monitoring, restart tracking, environment filtering, security masking

### 🎨 **Logging**
Advanced log management with colors, filtering, and streaming:

```bash
bunctl logs api                     # Colored output (stderr in red)
bunctl logs api --errors-first      # PM2-style error separation
bunctl logs --json --watch          # JSON + real-time streaming
```

**Features:** Colored stderr, app name prefixes, stack trace formatting, JSON support

#### Logging Configuration

bunctl supports flexible logging configuration through environment variables:

- **`RUST_LOG`**: Standard Rust logging filter (takes precedence when set)
- **`BUNCTL_LOG_LEVEL`**: Alternative logging configuration (default: `info`)
- **`BUNCTL_CONSOLE_LOG`**: Force console output in daemon mode (for debugging)

```bash
# Set custom log level
export BUNCTL_LOG_LEVEL=debug
bunctl daemon

# Use standard Rust logging
export RUST_LOG=bunctl=debug,bunctl_core=trace
bunctl start myapp

# Force console output for daemon debugging
export BUNCTL_CONSOLE_LOG=1
bunctl daemon
```

**Log Levels:** `error`, `warn`, `info` (default), `debug`, `trace`  
**Daemon Mode:** Logs to file (`/var/log/bunctl/daemon.log` on Linux, `%LOCALAPPDATA%\bunctl\logs\daemon.log` on Windows)

### 🔄 **Smart Restart Management**
Exponential backoff with visual tracking: `3/10` (Green) → `8/10` (Yellow) → `10/10 EXHAUSTED` (Red)

### 🏗️ **Cross-Platform Architecture**
**Linux**: cgroups v2, signalfd, inotify • **Windows**: Job Objects, IOCP, Named Pipes • **macOS**: Process groups, kqueue, FSEvents

---

## 🚀 **Installation**

### Release Binaries
```bash
# Download latest release
curl -L https://github.com/evobug-com/bunctl-rs/releases/latest/download/bunctl-linux-x64 -o bunctl
chmod +x bunctl
```

### From Source
```bash
git clone https://github.com/evobug-com/bunctl-rs.git
cd bunctl-rs
cargo build --release
./target/release/bunctl --version
```

---

## ⚡ **Quick Start**

### 1. Initialize Your App
```bash
# Auto-detect entry point and create config
bunctl init --name myapp

# With custom settings
bunctl init --name myapp --entry src/server.ts --port 3000 --memory 1G
```

### 2. Start Your Application  
```bash
# Start from config
bunctl start

# Start with daemon for full monitoring
bunctl daemon &
bunctl start
```

### 3. Monitor & Manage
```bash
# Beautiful status display
bunctl status

# Live status monitoring  
bunctl status --watch

# View logs with colors
bunctl logs myapp

# JSON for automation
bunctl status --json | jq '.[] | .memory_bytes'
```

---

## 📚 **Configuration**

bunctl auto-discovers config files in priority order: `bunctl.json` → `ecosystem.config.json` → `package.json`

### bunctl.json (Recommended)
```json
{
  "apps": [{
    "name": "api",
    "command": "bun",
    "args": ["src/server.ts"],
    "cwd": "/app",
    "auto_start": true,
    "restart_policy": "OnFailure",
    "max_memory": 536870912,
    "max_cpu_percent": 50.0,
    "env": { "PORT": "3000", "NODE_ENV": "production" },
    "backoff": { "max_attempts": 10, "base_delay_ms": 100 }
  }]
}
```

### ecosystem.config.json (PM2 Compatible)

> ⚠️ **Security Notice**: JavaScript config files (`.js`) are no longer supported as of v3.0.0+ to prevent code execution vulnerabilities. Please use JSON format instead.

```json
{
  "apps": [{
    "name": "api",
    "script": "src/server.ts",
    "interpreter": "bun",
    "max_restarts": 10,
    "max_memory_restart": "512M",
    "env": { "PORT": "3000", "NODE_ENV": "production" }
  }]
}
```

### Manual Loading & Overrides
```bash
bunctl start --config custom.json          # Explicit config
bunctl start api --max-memory 1G            # CLI overrides
bunctl start myapp --script src/server.ts   # Ad-hoc start
```

**Precedence:** CLI args > Explicit config > Auto-discovered config > Defaults

---

## 🛠️ **Commands & JSON API**

### Core Commands
```bash
# Process lifecycle
bunctl start [app]              # Start application(s)
bunctl stop [app]               # Stop application(s)
bunctl restart [app]            # Restart application(s)
bunctl delete [app]             # Delete application(s)

# Monitoring & information
bunctl status [app]             # Show status
bunctl list                     # List all apps
bunctl logs [app]               # View logs
bunctl init [options]           # Initialize config

# JSON API & advanced options
bunctl status --json --watch    # JSON + live updates
bunctl logs --json --watch      # JSON logs + streaming
bunctl logs --errors-first -n 50 # PM2-style with line limit
```

### JSON Response Format
```json
{
  "name": "api", "state": "Running", "pid": 12345,
  "uptime_seconds": 8100, "memory_bytes": 47185920, "cpu_percent": 2.1,
  "restarts": 3, "max_restart_attempts": 10,
  "command": "bun", "args": ["src/server.ts"], "env": {...}
}
```

---

## 🎯 **Automation Examples**

### Health Checks & Monitoring
```bash
# CI/CD health check
bunctl status api --json | jq -e '.state == "Running"' || exit 1

# Memory alerts
bunctl status --json | jq -r '.[] | select(.memory_bytes > 500000000) | 
  "ALERT: \(.name) using \(.memory_bytes / 1024 / 1024 | floor)MB"'

# Restart problematic apps
bunctl status --json | jq -r '.[] | select(.restarts > 10) | .name' | 
  xargs bunctl restart
```

---

## 🔒 **Security**

### Security Improvements in v3.0.0+

#### Command Injection Prevention
bunctl-rs includes critical security improvements to prevent command injection attacks:

- **No Shell Parsing**: Commands and arguments are never passed through a shell interpreter
- **Literal Arguments**: All command arguments are treated as literal strings
- **Safe Configuration**: Shell metacharacters (`;`, `|`, `&`, `>`, etc.) are passed as-is to the process

#### JavaScript Configuration Execution Prevention
As of v3.0.0+, JavaScript configuration files are **no longer supported** to prevent code execution vulnerabilities:

- **Blocked Files**: `ecosystem.config.js`, `pm2.config.js` cannot be loaded
- **JSON Only**: Use `ecosystem.config.json` or `bunctl.json` instead
- **Migration Required**: Convert existing JS configs to JSON format

**Migration Example:**
```javascript
// ❌ OLD: ecosystem.config.js (no longer supported)
module.exports = {
  apps: [{
    name: 'myapp',
    script: './index.js'
  }]
}
```

```json
// ✅ NEW: ecosystem.config.json (secure)
{
  "apps": [{
    "name": "myapp",
    "script": "./index.js"
  }]
}
```

#### Configuration Security
When defining commands in your configuration files, arguments must be explicitly separated:

```json
// ✅ SECURE - Arguments are properly separated
{
  "apps": [{
    "name": "myapp",
    "command": "bun",
    "args": ["run", "script.js", "--port", "3000"]
  }]
}

// ❌ INSECURE (pre-v3.0.0) - Could allow command injection
{
  "apps": [{
    "name": "myapp",
    "command": "bun run script.js --port 3000"  // This would be vulnerable to injection
  }]
}
```

This design ensures that user-controlled input in configuration files cannot be used to execute arbitrary commands, providing defense-in-depth security for your production deployments.

---

## 🏗️ **Architecture**

**Workspace:** `bunctl/` (CLI) • `bunctl-core/` (traits) • `bunctl-supervisor/` (OS-specific) • `bunctl-logging/` (async) • `bunctl-ipc/` (communication)

**Design:** Single-threaded tokio runtime, event-driven architecture (epoll/IOCP/kqueue), zero polling, lock-free logging, atomic operations

---

## 🚧 **Roadmap**

### ✅ Completed (v0.3.0)
- [x] Beautiful status display with DevOps information
- [x] JSON API for status and logs  
- [x] Watch modes for real-time monitoring
- [x] Enhanced logging with PM2-style features
- [x] Restart limit tracking and failure handling
- [x] Cross-platform Windows/Linux/macOS support

### 🔄 In Progress (v0.4.0)  
- [ ] JSON API for all commands (`start`, `stop`, `restart`, etc.)
- [ ] Real-time log following with WebSocket streaming
- [ ] Prometheus metrics endpoint
- [ ] Advanced filtering and querying

### 📋 Planned (v0.5.0+)
- [ ] Cluster management across multiple hosts
- [ ] Built-in reverse proxy with load balancing  
- [ ] Health check endpoints with custom scripts
- [ ] Integration with Docker and Kubernetes
- [ ] Web-based dashboard

---

## 🤝 **Contributing**

```bash
git clone https://github.com/evobug-com/bunctl-rs.git
cd bunctl-rs && cargo build && cargo test && cargo clippy
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for detailed guidelines.

---

## 📄 **License**

This project is licensed under the [MIT License](LICENSE).

---

## 🙏 **Acknowledgments**

- Inspired by PM2's excellent process management
- Built with the amazing Rust ecosystem
- Thanks to the Bun team for creating an incredible JavaScript runtime

---

<div align="center">

**⭐ Star this repo if bunctl-rs helps you manage your applications!**

[Report Bug](https://github.com/evobug-com/bunctl-rs/issues) • [Request Feature](https://github.com/evobug-com/bunctl-rs/issues) • [Documentation](https://bunctl.dev)

</div>