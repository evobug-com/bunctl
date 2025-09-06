# bunctl-core

Core abstractions, configuration management, and process supervision traits for the bunctl process manager.

## Overview

`bunctl-core` provides the foundational types, traits, and configuration system that power the bunctl process manager. This crate contains platform-agnostic process management abstractions, configuration loading logic, and the core types used throughout the bunctl ecosystem.

## Key Features

- **Multi-format Configuration**: Support for bunctl.json, PM2 ecosystem.config.js, and package.json configurations
- **Process Management Abstractions**: Platform-agnostic traits and types for process supervision
- **App Lifecycle Management**: State tracking and lifecycle management for managed applications
- **Exponential Backoff**: Configurable retry strategies with jitter and exhaustion handling
- **Type-Safe Configuration**: Serde-based configuration parsing with validation
- **Cross-Platform Support**: Windows, Linux, and macOS compatibility

## Core Types

### App Management

```rust
use bunctl_core::{App, AppId, AppState, AppConfig};

// Create an app ID (automatically sanitized)
let app_id = AppId::new("My App Name")?; // becomes "my-app-name"

// App state tracking
let app = App::new(app_id, config);
app.set_state(AppState::Running);
app.set_pid(Some(1234));

// Check app status
if app.get_state().is_running() {
    println!("App uptime: {:?}", app.uptime());
}
```

### Configuration System

The configuration system supports three formats with auto-discovery:

1. **bunctl.json** - Native format with full feature support
2. **ecosystem.config.js** - PM2 compatible format
3. **package.json** - Simple apps with bunctl section

```rust
use bunctl_core::config::{ConfigLoader, Config, AppConfig, RestartPolicy};
use std::time::Duration;

// Auto-discover configuration
let loader = ConfigLoader::new();
let config = loader.load().await?;

// Load specific config file
let config = loader.load_file("ecosystem.config.js").await?;

// Create config programmatically
let app_config = AppConfig {
    name: "my-app".to_string(),
    command: "bun".to_string(),
    args: vec!["run".to_string(), "server.ts".to_string()],
    restart_policy: RestartPolicy::Always,
    stop_timeout: Duration::from_secs(10),
    ..Default::default()
};
```

### Process Management

```rust
use bunctl_core::{ProcessBuilder, ProcessHandle, ProcessSupervisor};

// Build and spawn a process
let mut child = ProcessBuilder::new("bun")
    .args(&["run", "server.ts"])
    .current_dir("/app")
    .env("NODE_ENV", "production")
    .spawn()
    .await?;

// Create process handle for supervision
let app_id = AppId::new("my-app")?;
let handle = ProcessHandle::new(child.id().unwrap(), app_id, child);

// Process supervision (implemented by platform-specific crates)
async fn supervise_process<S: ProcessSupervisor>(supervisor: &S, config: &AppConfig) {
    let handle = supervisor.spawn(config).await?;
    let exit_status = supervisor.wait(&mut handle).await?;
    
    if exit_status.should_restart(&config.restart_policy) {
        // Restart the process
    }
}
```

### Exponential Backoff

```rust
use bunctl_core::BackoffStrategy;
use std::time::Duration;

let mut backoff = BackoffStrategy::new()
    .with_base_delay(Duration::from_millis(100))
    .with_max_delay(Duration::from_secs(30))
    .with_multiplier(2.0)
    .with_jitter(0.3)
    .with_max_attempts(5);

// Get next delay (with exponential backoff + jitter)
while let Some(delay) = backoff.next_delay() {
    tokio::time::sleep(delay).await;
    
    // Attempt restart...
    if restart_successful {
        backoff.reset();
        break;
    }
}

if backoff.is_exhausted() {
    // Handle exhaustion based on configured action
}
```

## Configuration Formats

### bunctl.json (Native Format)

```json
{
  "apps": [
    {
      "name": "my-app",
      "command": "bun run server.ts",
      "cwd": "/app",
      "env": {
        "NODE_ENV": "production",
        "PORT": "3000"
      },
      "auto_start": true,
      "restart_policy": "always",
      "max_memory": 536870912,
      "max_cpu_percent": 80.0,
      "health_check": {
        "type": "http",
        "url": "http://localhost:3000/health",
        "expected_status": 200,
        "interval": "30s",
        "timeout": "10s",
        "retries": 3,
        "start_period": "60s"
      },
      "backoff": {
        "base_delay_ms": 1000,
        "max_delay_ms": 30000,
        "multiplier": 2.0,
        "jitter": 0.3,
        "max_attempts": 5,
        "exhausted_action": "stop"
      }
    }
  ]
}
```

### ecosystem.config.js (PM2 Compatible)

```javascript
module.exports = {
  apps: [
    {
      name: 'my-app',
      script: 'server.ts',
      interpreter: 'bun',
      cwd: '/app',
      instances: 2,
      exec_mode: 'cluster',
      watch: true,
      ignore_watch: ['node_modules', 'logs'],
      max_memory_restart: '512M',
      env: {
        NODE_ENV: 'development',
        PORT: 3000
      },
      env_production: {
        NODE_ENV: 'production',
        PORT: 8000
      },
      error_file: 'logs/error.log',
      out_file: 'logs/out.log',
      log_file: 'logs/combined.log',
      autorestart: true,
      restart_delay: 1000,
      max_restarts: 10
    }
  ]
};
```

### package.json Integration

```json
{
  "name": "my-app",
  "scripts": {
    "start": "bun run server.ts"
  },
  "bunctl": {
    "apps": [
      {
        "name": "my-app",
        "command": "bun run start",
        "auto_start": true
      }
    ]
  }
}
```

## App States

Apps progress through well-defined states:

```rust
use bunctl_core::AppState;

match app.get_state() {
    AppState::Stopped => println!("App is stopped"),
    AppState::Starting => println!("App is starting..."),
    AppState::Running => println!("App is running (PID: {})", app.get_pid().unwrap()),
    AppState::Stopping => println!("App is stopping..."),
    AppState::Crashed => println!("App crashed"),
    AppState::Backoff { attempt, next_retry } => {
        println!("App in backoff, attempt {} (retry at {:?})", attempt, next_retry);
    }
}
```

## Restart Policies

Control when apps should restart:

- **`RestartPolicy::No`** - Never restart
- **`RestartPolicy::Always`** - Always restart regardless of exit code
- **`RestartPolicy::OnFailure`** - Only restart on non-zero exit codes
- **`RestartPolicy::UnlessStopped`** - Restart unless explicitly stopped

```rust
use bunctl_core::{ExitStatus, RestartPolicy};

let exit_status = ExitStatus::from_std(std_status);
let should_restart = exit_status.should_restart(RestartPolicy::OnFailure);
```

## Health Checks

Support for HTTP, TCP, and exec health checks:

```rust
use bunctl_core::config::{HealthCheck, HealthCheckType};
use std::time::Duration;

let health_check = HealthCheck {
    check_type: HealthCheckType::Http {
        url: "http://localhost:3000/health".to_string(),
        expected_status: 200,
    },
    interval: Duration::from_secs(30),
    timeout: Duration::from_secs(10),
    retries: 3,
    start_period: Duration::from_secs(60),
};
```

## Process Supervision Trait

Platform-specific supervisors implement the `ProcessSupervisor` trait:

```rust
use bunctl_core::{ProcessSupervisor, SupervisorEvent};
use async_trait::async_trait;

#[async_trait]
impl ProcessSupervisor for MyPlatformSupervisor {
    async fn spawn(&self, config: &AppConfig) -> bunctl_core::Result<ProcessHandle> {
        // Platform-specific process spawning
    }

    async fn kill_tree(&self, handle: &ProcessHandle) -> bunctl_core::Result<()> {
        // Kill entire process tree
    }

    async fn wait(&self, handle: &mut ProcessHandle) -> bunctl_core::Result<ExitStatus> {
        // Wait for process exit
    }

    async fn get_process_info(&self, pid: u32) -> bunctl_core::Result<ProcessInfo> {
        // Gather process statistics
    }

    async fn set_resource_limits(
        &self,
        handle: &ProcessHandle,
        config: &AppConfig,
    ) -> bunctl_core::Result<()> {
        // Apply memory/CPU limits
    }

    fn events(&self) -> mpsc::Receiver<SupervisorEvent> {
        // Return event stream
    }
}
```

## Error Handling

Comprehensive error types for different failure scenarios:

```rust
use bunctl_core::{Error, Result};

match result {
    Err(Error::ProcessNotFound(app)) => println!("Process {} not found", app),
    Err(Error::SpawnFailed(cmd)) => println!("Failed to spawn: {}", cmd),
    Err(Error::Config(msg)) => println!("Configuration error: {}", msg),
    Err(Error::InvalidAppName(name)) => println!("Invalid app name: {}", name),
    Err(Error::Timeout(op)) => println!("Timeout waiting for: {}", op),
    Ok(value) => println!("Success: {:?}", value),
}
```

## Configuration Watching

Monitor configuration files for changes:

```rust
use bunctl_core::config::ConfigWatcher;

let watcher = ConfigWatcher::new("bunctl.json").await?;

// Check for config changes
if watcher.check_reload().await? {
    println!("Configuration reloaded");
    let new_config = watcher.get();
}
```

## Integration with Other Crates

This core crate is designed to be used by:

- **bunctl** - Main CLI binary and command handlers
- **bunctl-supervisor** - OS-specific process supervision implementations  
- **bunctl-logging** - Async logging with the supervisor events
- **bunctl-ipc** - Inter-process communication using core types

### Example Integration

```rust
// In bunctl-supervisor
use bunctl_core::{ProcessSupervisor, AppConfig, ProcessHandle};

pub struct LinuxSupervisor {
    // Linux-specific fields (cgroups, etc.)
}

impl ProcessSupervisor for LinuxSupervisor {
    // Linux-specific implementation using cgroups v2
}

// In bunctl CLI
use bunctl_core::config::ConfigLoader;

let config = ConfigLoader::new()
    .with_search_path("./config")
    .load()
    .await?;

for app_config in config.apps {
    let app = App::new(AppId::new(&app_config.name)?, app_config);
    // Start supervision...
}
```

## Performance Considerations

- **Memory Usage**: Minimal allocations with Arc/RwLock for shared state
- **Lock-Free Operations**: Uses `arc-swap` for config hot-reloading
- **Efficient Serialization**: Zero-copy deserialization where possible
- **Backoff Jitter**: Prevents thundering herd issues during restarts

## Recent Improvements (v0.1.0)

### Fixed
- Corrected rand crate API usage (`gen_range` instead of `random_range`) for jitter calculation
- Enhanced command parsing to properly handle quoted arguments using `shell-words`

### Improved
- Better validation for CPU limits (should check against available cores)
- Consistent AppId sanitization (now properly handles all edge cases)
- Reduced lock contention with better synchronization patterns

### Testing & Code Quality
- Comprehensive test coverage with 115+ tests
- All tests passing on Windows, Linux, and macOS
- Edge case testing for backoff strategies, configuration loading, and state management
- **Zero clippy warnings** with `cargo clippy --all-features -- -D warnings`
- Clean code formatting with `cargo fmt`

## Known Areas for Enhancement

1. ~~**Configuration Dependencies**: Consider moving `rand` and `tempfile` to workspace dependencies for consistency~~ ✅ Fixed
2. ~~**AppId Sanitization**: Currently inconsistent with underscore handling - could be improved~~ ✅ Fixed
3. ~~**CPU Validation**: Should validate against `100.0 * num_cores` rather than just > 0~~ ✅ Fixed
4. ~~**Process Command Parsing**: Consider using `shell-words` consistently throughout~~ ✅ Fixed
5. **Concurrent State Access**: Multiple RwLocks could benefit from a single state struct (future optimization)

## License

This crate is part of the bunctl workspace and follows the same license terms.