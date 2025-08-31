# bunctl 🚀

> **Production-grade process manager for Bun applications using systemd**

[![Version](https://img.shields.io/badge/version-2.2.0-blue.svg)](https://github.com/yourusername/bunctl)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Bun](https://img.shields.io/badge/bun-%E2%89%A51.0.0-f472b6.svg)](https://bun.sh)
[![systemd](https://img.shields.io/badge/systemd-required-orange.svg)](https://systemd.io/)

> ⚠️ **IMPORTANT DISCLAIMER**
> 
> This tool is **not fully tested** and is currently being used **internally by Evobug products only**. We use it in production for running our API and Discord bot, but many features may change without notice. Most of the code and documentation has been generated with AI assistance.
> 
> **Use at your own risk in production environments outside of Evobug.**

**bunctl** is a powerful, lightweight process manager designed specifically for Bun applications. It leverages systemd's robust process supervision while providing a developer-friendly interface similar to PM2. Perfect for production deployments on Linux servers.

## ✨ Why bunctl?

- **🚀 Blazing Fast**: Built for Bun's speed, no Node.js overhead
- **🛡️ Production Ready**: Leverages systemd's battle-tested process supervision
- **📊 Resource Control**: Built-in memory and CPU limits
- **📝 Smart Logging**: Automatic log rotation with timestamps
- **🔄 Zero Downtime**: Graceful restarts and updates
- **🎯 Developer Friendly**: Simple commands, intuitive workflow
- **🔐 Secure by Default**: Systemd sandboxing and security features
- **💾 Lightweight**: Single bash script, no dependencies except systemd

## 📑 Table of Contents

- [Quick Start](#-quick-start)
- [Installation](#-installation)
- [Features](#-features)
- [Commands](#-commands)
- [Configuration](#-configuration)
- [Advanced Usage](#-advanced-usage)
- [Architecture](#-architecture)
- [Comparison with Alternatives](#-comparison-with-alternatives)
- [Troubleshooting](#-troubleshooting)
- [Migration Guides](#-migration-guides)
- [Contributing](#-contributing)

## 🚀 Quick Start

```bash
# Install bunctl
sudo curl -o /usr/local/bin/bunctl https://raw.githubusercontent.com/yourusername/bunctl/main/bunctl
sudo chmod +x /usr/local/bin/bunctl

# Navigate to your app
cd /var/www/sites/my-app

# Generate configuration (recommended)
bunctl generate-config

# Initialize and start your app
bunctl init
bunctl start my-app

# Check status
bunctl status
```

## 📦 Installation

### Prerequisites

- **Linux** with systemd (Ubuntu 20.04+, Debian 10+, RHEL 8+, etc.)
- **Bun** runtime installed ([install Bun](https://bun.sh/docs/installation))
- **sudo** access for systemd service management
- **bash** 4.0 or higher

### Install Script

```bash
# Download and install in one command
curl -fsSL https://raw.githubusercontent.com/yourusername/bunctl/main/install.sh | bash

# Or manually
sudo wget -O /usr/local/bin/bunctl https://raw.githubusercontent.com/yourusername/bunctl/main/bunctl
sudo chmod +x /usr/local/bin/bunctl

# Verify installation
bunctl --version
# Output: bunctl version 2.2.0

# Install bash completion (optional but recommended)
bunctl install-completion
source /etc/bash_completion.d/bunctl
```

## 🎯 Features

### Core Features

- **🔄 Process Management**: Start, stop, restart, and monitor Bun applications
- **📊 Resource Limits**: Control memory and CPU usage per application
- **📝 Advanced Logging**: Timestamped logs with automatic rotation
- **🚦 Health Monitoring**: Detailed health reports and status checks
- **⚡ Auto-start on Boot**: Automatic service recovery after system restarts
- **🔐 Security Sandboxing**: Leverages systemd security features
- **📦 Zero Dependencies**: Just bash and systemd
- **🎨 Beautiful CLI**: Colored output and intuitive interface

### Advanced Features

- **📋 Configuration Files**: `.bunctl.json` for declarative app configuration
- **🔍 Pattern Matching**: Bulk operations with wildcards
- **💾 Backup/Restore**: Save and restore service configurations
- **🌍 Environment Management**: Easy environment variable configuration
- **📊 JSON Output**: Machine-readable output for automation
- **🔄 Hot Reload**: Update configurations without downtime
- **📈 Metrics**: Memory, CPU, restart count tracking
- **🎯 Smart Detection**: Auto-detects entry files and configurations

## 📚 Commands

### Basic Commands

#### `bunctl init [name] [entry] [port]`
Initialize a new application service.

```bash
# Auto-detect everything
bunctl init

# Specify name and entry
bunctl init api-server src/server.ts

# With port
bunctl init api-server src/server.ts 3000

# Using configuration file (recommended)
bunctl generate-config
# Edit .bunctl.json
bunctl init
```

#### `bunctl start <name>`
Start an application.

```bash
bunctl start my-app
# ✅ Started: my-app
```

#### `bunctl stop <name>`
Stop an application.

```bash
bunctl stop my-app
# ✅ Stopped: my-app
```

#### `bunctl restart <name>`
Restart an application.

```bash
bunctl restart my-app
# ✅ Restarted: my-app
```

#### `bunctl status [--json]`
Show status of all applications.

```bash
bunctl status

# ━━━ Bun Applications Status ━━━

#   my-api [boot]
#     Status:  ● RUNNING
#     PID:     12345
#     Memory:  45.2 MB
#     CPU:     1.2%

#   my-worker
#     Status:  ○ STOPPED
```

JSON output:
```bash
bunctl status --json | jq
```

#### `bunctl logs [name] [-n lines] [-f]`
View application logs.

```bash
# Show last 100 lines from all apps
bunctl logs

# Show last 200 lines from specific app
bunctl logs my-app -n 200

# Follow logs in real-time (like tail -f)
bunctl logs my-app -f

# Follow all apps
bunctl logs -f
```

### Advanced Commands

#### `bunctl health <name>`
Detailed health report for an application.

```bash
bunctl health my-app

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Health Report: my-app
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Status:       🟢 Healthy
# Boot startup: ✅ Enabled
# PID:          12345
# Uptime:       5d 3h 27m
# Memory:       152.3 MB
# Restarts:     0
```

#### `bunctl env <name> KEY=value`
Set environment variables for an application.

```bash
bunctl env my-app NODE_ENV=production
bunctl env my-app PORT=3000
bunctl env my-app DATABASE_URL=postgresql://...
```

#### `bunctl generate-config [--force]`
Generate a `.bunctl.json` configuration file.

```bash
bunctl generate-config
# ✅ Created .bunctl.json
# ℹ️ Configuration file generated with detected settings:
#   • Name: my-app
#   • Entry: src/server.ts
```

### Bulk Operations

#### `bunctl start-all`
Start all registered applications.

```bash
bunctl start-all
# ✅ Started: api-server
# ✅ Started: worker
# ✅ Started: websocket-server
```

#### `bunctl restart-group <pattern>`
Restart applications matching a pattern.

```bash
bunctl restart-group 'api-*'
# ✅ Restarted: api-server
# ✅ Restarted: api-gateway
# ✅ Restarted: api-worker
```

### System Commands

#### `bunctl backup [name]`
Backup all service configurations.

```bash
bunctl backup production-backup
# ✅ Backup created: 3 services backed up
# Backup location: ~/.config/bunctl/backups/production-backup_20240901_143022
```

#### `bunctl restore <backup>`
Restore services from backup.

```bash
bunctl restore production-backup_20240901_143022
# ⚠️ This will restore services from: ...
# Continue? (y/N): y
# ✅ Restored 3 services from backup
```

#### `bunctl update`
Update all service files with current bunctl version.

```bash
bunctl update
# ℹ️ Regenerating all service files with current configuration...
# ✅ Updated 3 services
```

## ⚙️ Configuration

### Configuration File (.bunctl.json)

The recommended way to configure applications is using a `.bunctl.json` file in your project root.

```json
{
  "name": "my-api",
  "entry": "src/server.ts",
  "port": 3000,
  "runtime": "bun",
  "memory": "1G",
  "cpu": 75,
  "autostart": true,
  "restart_delay": 10,
  "max_restarts": 3,
  "env": {
    "NODE_ENV": "production",
    "LOG_LEVEL": "info",
    "DATABASE_POOL_SIZE": "10"
  }
}
```

#### Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `name` | string | directory name | Application identifier |
| `entry` | string | auto-detected | Entry file path (relative to project root) |
| `port` | number | - | Port number (sets PORT env var) |
| `runtime` | string | "bun" | Runtime mode: "bun" or "node" |
| `memory` | string | "512M" | Memory limit (e.g., "512M", "1G", "2G") |
| `cpu` | number | 50 | CPU quota percentage (1-100) |
| `autostart` | boolean | true | Start on system boot |
| `restart_delay` | number | 10 | Seconds to wait before restart |
| `max_restarts` | number | 3 | Maximum restart attempts in 60 seconds |
| `env` | object | {} | Environment variables |

### Entry File Detection

bunctl automatically detects common entry file patterns:

1. Checks `.bunctl.json` for configured entry
2. Looks for common patterns:
   - `src/server.ts`, `src/index.ts`, `src/main.ts`, `src/app.ts`
   - `server.ts`, `index.ts`, `main.ts`, `app.ts`
   - `src/server.js`, `src/index.js`, `src/main.js`, `src/app.js`
   - `server.js`, `index.js`, `main.js`, `app.js`
3. Falls back to `index.ts`

### Environment Variables

Three ways to set environment variables:

1. **Configuration file** (`.bunctl.json`):
```json
{
  "env": {
    "NODE_ENV": "production",
    "API_KEY": "secret"
  }
}
```

2. **Environment file** (`.env` or `config/.env`):
```bash
NODE_ENV=production
API_KEY=secret
DATABASE_URL=postgresql://localhost/mydb
```

3. **Command line**:
```bash
bunctl env my-app NODE_ENV=production
bunctl env my-app API_KEY=secret
```

Priority: Command line > .bunctl.json > .env file

## 🔧 Advanced Usage

### Log Management

#### Log Locations
- **Application logs**: `{app_dir}/logs/app.log`
- **Error logs**: `{app_dir}/logs/error.log`
- **Rotated logs**: `{app_dir}/logs/app.{timestamp}.log`

#### Log Rotation
- Automatic rotation on service start/restart
- Keeps last 10 log files per type
- Timestamp format: `YYYYMMDD_HHMMSS`

#### Following Logs
```bash
# Follow single app
bunctl logs my-app -f

# Follow all apps with prefixes
bunctl logs -f

# Follow with initial lines
bunctl logs -f -n 500
```

### Resource Management

#### Memory Limits
Control memory usage to prevent resource exhaustion:

```json
{
  "memory": "512M"  // Options: "256M", "512M", "1G", "2G", etc.
}
```

Memory limit enforcement via systemd's `MemoryMax`.

#### CPU Limits
Prevent CPU monopolization:

```json
{
  "cpu": 50  // 50% of one CPU core
}
```

Uses systemd's `CPUQuota` for fair resource sharing.

#### Task Limits
Prevent fork bombs:
- Maximum tasks: 100 (configurable in service file)

### Security Features

bunctl leverages systemd's security features:

- **NoNewPrivileges**: Prevents privilege escalation
- **PrivateTmp**: Isolated /tmp directory
- **ProtectSystem**: Read-only system directories
- **ProtectHome**: Limited home directory access
- **ReadWritePaths**: Explicit write permissions

### Pattern-Based Operations

Use wildcards for bulk operations:

```bash
# Restart all API services
bunctl restart-group 'api-*'

# Restart all workers
bunctl restart-group '*-worker'

# Complex patterns
bunctl restart-group 'prod-api-*'
```

### Boot Management

#### Enable auto-start for all apps:
```bash
bunctl install-boot
# ✅ Boot autostart service installed and enabled
```

#### Disable auto-start:
```bash
bunctl uninstall-boot
# ✅ Boot autostart service removed
```

Individual app boot control:
```json
{
  "autostart": true  // or false
}
```

### Backup and Restore

#### Creating Backups
```bash
# Backup with custom name
bunctl backup production-2024

# Default backup (uses timestamp)
bunctl backup
```

Backups include:
- All service files
- Application database
- Configuration metadata

#### Restoring from Backup
```bash
# List available backups
ls ~/.config/bunctl/backups/

# Restore specific backup
bunctl restore production-2024_20240901_143022
```

### Health Monitoring

Detailed health checks provide:
- Process status (running/stopped/failed)
- Boot startup configuration
- Process ID
- Uptime calculation
- Memory usage
- CPU usage
- Restart count
- Recent error logs (if failed)

```bash
bunctl health my-app
```

### JSON API

For automation and monitoring integration:

```bash
# Get all apps status
bunctl status --json

# Parse with jq
bunctl status --json | jq '.apps[] | select(.status=="active")'

# Monitor memory usage
bunctl status --json | jq '.apps[] | {name: .name, memory: .memory}'
```

## 🏗️ Architecture

### How bunctl Works

bunctl bridges the gap between developer-friendly commands and systemd's powerful process management:

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Developer │────▶│    bunctl   │────▶│   systemd   │
└─────────────┘     └─────────────┘     └─────────────┘
                            │
                            ▼
                    ┌─────────────┐
                    │ Service File│
                    └─────────────┘
```

### Service File Generation

bunctl generates optimized systemd service files:

```ini
[Unit]
Description=Bun App - my-app
After=network.target
StartLimitIntervalSec=60
StartLimitBurst=3

[Service]
Type=simple
User=www-data
Group=www-data
WorkingDirectory=/var/www/sites/my-app
ExecStartPre=/bin/sh -c 'echo "\n===== Service started at $(date) =====" >> logs/app.log'
ExecStart=/bin/sh -c 'bun run src/server.ts 2>&1 | while read line; do echo "[$(date)] $line"; done >> logs/app.log'
ExecStopPost=/bin/sh -c 'echo "===== Service stopped at $(date) =====\n" >> logs/app.log'
Restart=always
RestartSec=10
Environment="NODE_ENV=production"

# Resource limits
MemoryMax=512M
CPUQuota=50%

# Security
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=/var/www/sites/my-app

[Install]
WantedBy=multi-user.target
```

### Directory Structure

```
/var/www/sites/my-app/
├── src/
│   └── server.ts
├── logs/
│   ├── app.log
│   ├── error.log
│   └── app.20240901_143022.log (rotated)
├── .bunctl.json
└── .env

~/.config/bunctl/
├── apps.db
└── backups/
    └── backup_20240901_143022/
        ├── bun-app-my-app.service
        ├── apps.db
        └── metadata.json
```

### Process Supervision

bunctl leverages systemd's supervision features:

1. **Automatic Restarts**: Configurable restart policies
2. **Restart Limits**: Prevents restart loops
3. **Dependencies**: Proper startup ordering
4. **Resource Control**: Memory and CPU limits
5. **Logging**: Integrated with journald and file logs

## 📊 Comparison with Alternatives

### bunctl vs PM2

| Feature | bunctl | PM2 |
|---------|--------|-----|
| **Runtime** | Bun-first | Node.js focused |
| **Memory Usage** | ~5MB | ~50-100MB |
| **Dependencies** | None (uses systemd) | Many npm packages |
| **Process Manager** | systemd (kernel-level) | JavaScript (userspace) |
| **Startup Speed** | Instant | 1-2 seconds |
| **Resource Limits** | Native (cgroups) | JavaScript implementation |
| **Log Rotation** | Built-in | Requires pm2-logrotate |
| **Clustering** | Via systemd instances | Built-in |
| **Monitoring UI** | No (use system tools) | pm2 monit |
| **System Integration** | Deep (systemd) | Surface-level |
| **Security** | systemd sandboxing | Basic |

### bunctl vs Docker Compose

| Feature | bunctl | Docker Compose |
|---------|--------|----------------|
| **Complexity** | Simple | Complex |
| **Resource Overhead** | Minimal | Container overhead |
| **Setup Time** | Seconds | Minutes |
| **Image Building** | Not needed | Required |
| **Network Isolation** | No | Yes |
| **Portability** | Linux with systemd | Any Docker host |
| **Direct Host Access** | Yes | Limited |
| **File System** | Direct | Volumes/Bind mounts |

### bunctl vs Direct systemd

| Feature | bunctl | Direct systemd |
|---------|--------|----------------|
| **Ease of Use** | Very easy | Complex |
| **Service File Creation** | Automatic | Manual |
| **App Discovery** | Automatic | Manual |
| **Log Management** | Integrated | Manual setup |
| **Bulk Operations** | Built-in | Script yourself |
| **Configuration** | JSON | INI format |
| **Developer Experience** | Excellent | Poor |

### When to Use What?

**Use bunctl when:**
- Running Bun applications in production
- You want PM2-like simplicity with systemd reliability
- Resource efficiency is important
- You're on a Linux server with systemd
- You need fine-grained resource control

**Use PM2 when:**
- Running Node.js applications
- You need the PM2 ecosystem (pm2.io monitoring)
- Cross-platform support is required
- You want built-in clustering

**Use Docker when:**
- You need complete isolation
- Running multiple different stacks
- Portability across different systems is critical
- You have a microservices architecture

**Use direct systemd when:**
- You need ultimate control
- Running non-JavaScript services
- You're comfortable with systemd

## 🔍 Troubleshooting

### Common Issues

#### Bun Not Found
```bash
# Error: Bun executable not found!

# Solution: Install Bun
curl -fsSL https://bun.sh/install | bash

# Add to PATH if needed
export PATH="$HOME/.bun/bin:$PATH"
```

#### Permission Denied
```bash
# Error: Permission denied

# Solution: Use sudo for systemd operations
sudo bunctl init my-app
```

#### Service Won't Start
```bash
# Check detailed status
sudo systemctl status bun-app-my-app

# View system logs
sudo journalctl -u bun-app-my-app -n 50

# Check app logs
bunctl logs my-app -n 100

# Verify entry file exists
ls -la /var/www/sites/my-app/src/server.ts
```

#### Port Already in Use
```bash
# Find process using port
sudo lsof -i :3000

# Kill process
sudo kill -9 <PID>

# Or change port
bunctl env my-app PORT=3001
bunctl restart my-app
```

#### Memory Limit Exceeded
```bash
# Increase memory limit in .bunctl.json
{
  "memory": "1G"  // Increase from 512M
}

# Update and restart
bunctl update
bunctl restart my-app
```

### Debugging Steps

1. **Check Status**:
```bash
bunctl health my-app
```

2. **View Logs**:
```bash
# Application logs
bunctl logs my-app -n 200

# System logs
sudo journalctl -u bun-app-my-app -f
```

3. **Verify Configuration**:
```bash
# Check service file
sudo cat /etc/systemd/system/bun-app-my-app.service

# Verify working directory
ls -la /var/www/sites/my-app/
```

4. **Test Manually**:
```bash
cd /var/www/sites/my-app
bun run src/server.ts
```

### FAQ

**Q: Can I use bunctl with Node.js?**
A: Yes! Set `"runtime": "node"` in `.bunctl.json` for Node.js compatibility mode.

**Q: How do I handle environment-specific configs?**
A: Use separate `.bunctl.json` files or environment variables:
```bash
# .bunctl.production.json
# .bunctl.development.json
cp .bunctl.production.json .bunctl.json
bunctl init
```

**Q: Can I run multiple instances of the same app?**
A: Yes, use different names:
```bash
bunctl init api-1 src/server.ts 3001
bunctl init api-2 src/server.ts 3002
```

**Q: How do I migrate from PM2?**
A: See [Migration from PM2](#migration-from-pm2) section.

**Q: Does bunctl support Windows?**
A: No, bunctl requires systemd which is Linux-specific. Use WSL2 on Windows.

**Q: Can I use bunctl in Docker?**
A: Technically yes, but it's not recommended. Docker has its own process management.

## 📦 Migration Guides

### Migration from PM2

#### 1. Export PM2 Configuration
```bash
pm2 prettylist > pm2-apps.json
```

#### 2. Convert Each App
For each PM2 app, create a `.bunctl.json`:

PM2 ecosystem.config.js:
```javascript
module.exports = {
  apps: [{
    name: 'api',
    script: './src/server.js',
    instances: 1,
    exec_mode: 'fork',
    env: {
      PORT: 3000,
      NODE_ENV: 'production'
    },
    max_memory_restart: '500M'
  }]
}
```

Equivalent `.bunctl.json`:
```json
{
  "name": "api",
  "entry": "src/server.js",
  "port": 3000,
  "memory": "500M",
  "env": {
    "NODE_ENV": "production"
  }
}
```

#### 3. Initialize Services
```bash
cd /path/to/app
bunctl init
```

#### 4. Migrate Commands

| PM2 Command | bunctl Equivalent |
|-------------|-------------------|
| `pm2 start app.js` | `bunctl init && bunctl start app` |
| `pm2 stop app` | `bunctl stop app` |
| `pm2 restart app` | `bunctl restart app` |
| `pm2 delete app` | `bunctl delete app` |
| `pm2 list` | `bunctl status` |
| `pm2 logs` | `bunctl logs` |
| `pm2 monit` | `bunctl status` + `htop` |
| `pm2 save` | Automatic with systemd |
| `pm2 startup` | `bunctl install-boot` |

### Migration from Docker Compose

#### 1. Identify Services
From docker-compose.yml:
```yaml
services:
  api:
    build: .
    ports:
      - "3000:3000"
    environment:
      NODE_ENV: production
    restart: always
    mem_limit: 512m
```

#### 2. Create bunctl Configuration
`.bunctl.json`:
```json
{
  "name": "api",
  "entry": "src/server.ts",
  "port": 3000,
  "memory": "512M",
  "env": {
    "NODE_ENV": "production"
  }
}
```

#### 3. Deploy
```bash
# Instead of: docker-compose up -d
bunctl init
bunctl start api
```

### Migration from Direct systemd

If you have existing systemd services, bunctl can import them:

1. **Backup existing service**:
```bash
sudo cp /etc/systemd/system/my-app.service ~/my-app.service.backup
```

2. **Create `.bunctl.json`** based on your service file

3. **Initialize with bunctl**:
```bash
bunctl init my-app
```

4. **Verify and remove old service**:
```bash
sudo systemctl disable my-app
sudo rm /etc/systemd/system/my-app.service
```

## 🤝 Contributing

We welcome contributions! Here's how to get started:

### Contributing Guidelines

1. **Fork the repository**
2. **Create a feature branch**: `git checkout -b feature/amazing-feature`
3. **Make your changes**
5. **Commit with clear messages**: `git commit -m 'Add amazing feature'`
6. **Push to your fork**: `git push origin feature/amazing-feature`
7. **Open a Pull Request**

### Areas for Contribution

- 🐛 Bug fixes
- ✨ New features
- 📚 Documentation improvements
- 🧪 Test coverage
- 🌐 Internationalization
- 🔧 Performance optimizations

### Code Style

- Use 4 spaces for indentation
- Follow existing naming conventions
- Add comments for complex logic
- Update help text for new commands
- Maintain backwards compatibility

## 📄 License

MIT License - see [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- The Bun team for creating an amazing JavaScript runtime
- The systemd project for robust process management
- PM2 for inspiration on developer experience
- The open-source community for feedback and contributions

## 📞 Support

- **Issues**: [GitHub Issues](https://github.com/yourusername/bunctl/issues)
- **Discussions**: [GitHub Discussions](https://github.com/yourusername/bunctl/discussions)
- **Security**: Report vulnerabilities privately via GitHub Security Advisories

---

<p align="center">
  Made with ❤️ for the Bun community
  <br>
  <a href="https://github.com/yourusername/bunctl">Star us on GitHub</a>
</p>