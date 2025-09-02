# bunctl-rs

A production-grade process manager for Bun applications written in Rust, designed as a zero-overhead replacement for PM2.

## Features

- **Minimal Resource Usage**: Near-zero memory and CPU overhead
- **OS-Native Process Management**:
    - Linux: cgroups v2 for resource limits and process tree management
    - Windows: Job Objects for process groups
    - macOS: Process groups with kqueue monitoring
- **Efficient Logging**: Lock-free async logging with atomic rotation
- **Smart Crash Recovery**: Exponential backoff with jitter
- **Configuration Hot-Reload**: File watching without polling
- **Graceful Shutdown**: Proper signal handling and log draining
- **PM2 Compatibility**: Supports ecosystem.config.js format
- **Config Auto-Discovery**: Automatically finds bunctl.json, ecosystem.config.js, or package.json

## Architecture

```
bunctl-rs/
├── bunctl/              # CLI binary
├── bunctl-core/         # Core process management traits
├── bunctl-supervisor/   # OS-specific supervisors
├── bunctl-logging/      # Async logging system
└── bunctl-ipc/          # IPC for control messages
```

## Key Design Principles

1. **Single-threaded tokio runtime** per supervisor for minimal overhead
2. **Event-driven architecture** using OS primitives (epoll, IOCP, kqueue)
3. **Zero polling** - pure event-based with OS notifications
4. **Lock-free logging** with line buffering and async writes
5. **Atomic log rotation** using rename() + fsync()

## Usage

### Initialize Configuration

```bash
# Create bunctl.json with auto-detected entry point
bunctl init --name myapp

# Create ecosystem.config.js (PM2 compatible)
bunctl init --name myapp --ecosystem

# Import from existing ecosystem.config.js
bunctl init --from-ecosystem ecosystem.config.js

# With custom settings
bunctl init --name myapp --entry src/server.ts --port 3000 --memory 1G --cpu 75
```

### Start Applications

```bash
# Start from config file (auto-discovers bunctl.json or ecosystem.config.js)
bunctl start

# Start specific app from config
bunctl start myapp

# Start all apps from config
bunctl start all

# Start with specific config file
bunctl start --config ecosystem.config.js

# Ad-hoc start without config
bunctl start myapp --script app.ts

# With resource limits
bunctl start myapp --script app.ts --max-memory 512000000 --max-cpu 50
```

### Other Commands

```bash
# View status
bunctl status

# View logs
bunctl logs myapp --follow

# Stop application
bunctl stop myapp

# Restart with parallel mode
bunctl restart myapp --parallel
```

## Configuration Formats

### bunctl.json
```json
{
  "apps": [
    {
      "name": "api",
      "command": "bun run src/server.ts",
      "cwd": "/app",
      "env": {
        "PORT": "3000",
        "NODE_ENV": "production"
      },
      "max_memory": 536870912,
      "max_cpu_percent": 50,
      "restart_policy": "always"
    }
  ]
}
```

### ecosystem.config.js (PM2 Compatible)
```javascript
module.exports = {
  apps: [{
    name: 'api',
    script: 'src/server.ts',
    interpreter: 'bun',
    instances: 1,
    autorestart: true,
    watch: false,
    max_memory_restart: '512M',
    env: {
      PORT: 3000,
      NODE_ENV: 'production'
    }
  }]
}
```

### package.json
```json
{
  "name": "myapp",
  "scripts": {
    "start": "bun run src/server.ts"
  },
  "bunctl": {
    "apps": [{
      "name": "myapp",
      "command": "bun run start"
    }]
  }
}
```

## Performance Targets

- Memory: <5MB per supervisor
- CPU: <0.1% idle
- Startup: <50ms
- Log latency: <1ms p99

## Building

```bash
cargo build --release
```

## OS-Specific Features

### Linux
- cgroups v2 for resource management
- signalfd for signal handling
- inotify for file watching

### Windows
- Job Objects for process trees
- IOCP for async I/O
- Named pipes for IPC

### macOS
- Process groups with setpgid()
- kqueue for event monitoring
- FSEvents for file watching

## Security

- Process isolation via OS primitives
- Capability dropping on Linux (optional)
- No shell execution - direct process spawn
- Restricted tokens on Windows (future)