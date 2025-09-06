# bunctl-ipc

Inter-process communication library for bunctl, providing secure and efficient communication between the CLI client and daemon process.

## Features

- **Platform-specific optimized transports**
  - Linux: Unix domain sockets
  - Windows: Named pipes
- **Security hardening**
  - Message size validation (10MB limit)
  - DoS attack prevention through size limits
  - OS-level access control
- **Reliability features**
  - Configurable timeouts (30s default)
  - Comprehensive error handling with tracing
  - Graceful connection cleanup
- **Performance optimized**
  - Lock-free async operations
  - ~1KB memory per connection
  - Sub-millisecond local IPC latency

## Architecture

### Message Protocol

The IPC protocol uses length-prefixed JSON messages:
```
[4-byte length (little-endian)][JSON payload]
```

- **Maximum size**: 10MB (configurable via `MAX_MESSAGE_SIZE`)
- **Encoding**: UTF-8 JSON via serde_json
- **Validation**: Size checked before allocation to prevent DoS

### Message Types

**Client → Server (IpcMessage)**
```rust
pub enum IpcMessage {
    Start { name: String, config: String },
    Stop { name: String },
    Restart { name: String },
    Delete { name: String },
    Status { name: Option<String> },
    List,
    Logs { name: Option<String>, lines: usize },
    Subscribe { subscription: SubscriptionType },
    Unsubscribe,
}
```

**Server → Client (IpcResponse)**
```rust
pub enum IpcResponse {
    Success { message: String },
    Error { message: String },
    Data { data: serde_json::Value },
    Event { event_type: String, data: serde_json::Value },
}
```

**Event Subscriptions**
```rust
pub enum SubscriptionType {
    StatusEvents { app_name: Option<String> },
    LogEvents { app_name: Option<String> },
    AllEvents { app_name: Option<String> },
}
```

## Usage

### Server Example

```rust
use bunctl_ipc::{IpcServer, IpcMessage, IpcResponse};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Bind to platform-specific IPC endpoint
    let mut server = IpcServer::bind("/tmp/bunctl.sock").await?;
    
    loop {
        // Accept client connection
        let mut connection = server.accept().await?;
        
        // Optional: Set custom timeout for this connection
        connection.set_timeout(Duration::from_secs(60));
        
        // Handle connection in separate task
        tokio::spawn(async move {
            while let Ok(message) = connection.recv().await {
                let response = match message {
                    IpcMessage::Status { name } => {
                        IpcResponse::Data {
                            data: serde_json::json!({
                                "status": "running",
                                "pid": 1234
                            })
                        }
                    }
                    _ => IpcResponse::Error {
                        message: "Not implemented".to_string()
                    }
                };
                
                if connection.send(&response).await.is_err() {
                    break; // Client disconnected
                }
            }
        });
    }
}
```

### Client Example

```rust
use bunctl_ipc::{IpcClient, IpcMessage, IpcResponse};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to server
    let mut client = IpcClient::connect("/tmp/bunctl.sock").await?;
    
    // Optional: Set custom timeout
    client.set_timeout(Duration::from_secs(10));
    
    // Send message
    let message = IpcMessage::Status { 
        name: Some("my-app".to_string()) 
    };
    client.send(&message).await?;
    
    // Receive response
    match client.recv().await? {
        IpcResponse::Data { data } => {
            println!("Status: {}", data);
        }
        IpcResponse::Error { message } => {
            eprintln!("Error: {}", message);
        }
        _ => {}
    }
    
    Ok(())
}
```

## Platform Support

### Linux

- **Transport**: Unix domain sockets via `tokio::net::UnixStream`
- **Socket Path**: `$XDG_RUNTIME_DIR/bunctl.sock` or `/tmp/bunctl.sock`
- **Features**:
  - Automatic cleanup of stale socket files
  - File system permissions for access control
  - Warning on socket removal failures

### Windows

- **Transport**: Named pipes via `tokio::net::windows::named_pipe`
- **Pipe Name**: `\\.\pipe\bunctl_{identifier}`
- **Features**:
  - Automatic next instance creation for concurrent connections
  - Windows security descriptors for access control
  - Enhanced debug logging for troubleshooting

### macOS Support

macOS is not currently supported. The crate is designed for Linux and Windows only.

## Security

### Built-in Protections

1. **Message Size Validation**: All messages are validated against `MAX_MESSAGE_SIZE` (10MB) before allocation
2. **Timeout Protection**: Default 30-second timeout prevents resource exhaustion
3. **Input Validation**: JSON deserialization provides type safety
4. **Local-only Communication**: No network exposure

### OS-Level Security

- **Linux**: Unix socket file permissions
- **Windows**: Named pipe security descriptors

### Recommendations

- Use `XDG_RUNTIME_DIR` on Linux for user-specific secure sockets
- Implement application-level authentication if needed
- Consider encryption for sensitive data (not built-in)

## Configuration

### Constants

```rust
/// Maximum allowed message size (10MB)
pub const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024;

/// Default timeout for IPC operations (30 seconds)
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
```

### Timeout Configuration

```rust
// Server connection timeout
let mut connection = server.accept().await?;
connection.set_timeout(Duration::from_secs(60));

// Client timeout
let mut client = IpcClient::connect(path).await?;
client.set_timeout(Duration::from_secs(10));
```

## Error Handling

All operations return `bunctl_core::Result<T>`:

- `Error::Io` - Network/IO errors with context
- `Error::Other` - Serialization errors, timeouts, validation failures

### Logging

Comprehensive logging via `tracing`:
- `trace!` - Protocol-level details (message sizes, operations)
- `debug!` - Connection lifecycle (connect, accept, send, receive)
- `error!` - Error conditions with full context
- `warn!` - Non-fatal issues (e.g., socket cleanup failures)

Example:
```bash
RUST_LOG=bunctl_ipc=debug cargo run
```

## Testing

```bash
# Run all tests
cargo test -p bunctl-ipc

# Run library tests only
cargo test -p bunctl-ipc --lib

# Run integration tests
cargo test -p bunctl-ipc --test integration_test
```

### Test Coverage

- Message serialization/deserialization
- Large message handling (up to 1MB)
- Message size limit enforcement
- Timeout behavior
- Concurrent client connections
- All message types
- Error conditions

## Performance

### Benchmarks

- **Latency**: Sub-millisecond for local IPC
- **Throughput**: Limited by JSON serialization (~100MB/s)
- **Memory**: ~1KB per connection + message buffers
- **CPU**: Near-zero when idle

### Optimization Tips

1. Batch multiple operations in single messages when possible
2. Use subscription events instead of polling
3. Set appropriate timeouts for your use case
4. Consider message size vs serialization overhead

## Development

### Building

```bash
# Debug build
cargo build -p bunctl-ipc

# Release build
cargo build --release -p bunctl-ipc
```

### Code Quality

```bash
# Type checking
cargo check -p bunctl-ipc

# Linting
cargo clippy -p bunctl-ipc

# Formatting
cargo fmt -p bunctl-ipc
```

## Dependencies

- `bunctl-core` - Core types and error handling
- `tokio` - Async runtime with platform networking
- `serde` / `serde_json` - Message serialization
- `tracing` - Structured logging
- `bytes` - Efficient byte buffers
- `anyhow` / `thiserror` - Error handling

### Dev Dependencies

- `tempfile` - Temporary files for testing
- `rand` - Random test data generation

## Changelog

### Recent Improvements

- Added message size validation to prevent DoS attacks
- Implemented configurable timeouts for all operations
- Unified error handling and logging across platforms
- Removed code duplication in Windows implementation
- Added comprehensive integration tests
- Removed macOS support (Linux and Windows only)
- Enhanced documentation with examples

## License

See the workspace root for license information.