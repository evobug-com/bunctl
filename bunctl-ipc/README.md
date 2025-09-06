# bunctl-ipc

Inter-process communication (IPC) library for bunctl-rs, enabling secure and efficient communication between the CLI client and daemon process.

## Overview

The `bunctl-ipc` crate provides cross-platform IPC capabilities for bunctl, utilizing platform-specific transport mechanisms to ensure optimal performance and security. The library implements a client-server architecture where:

- **CLI commands** act as IPC clients, sending commands to the daemon
- **Daemon process** acts as an IPC server, processing commands and managing applications
- **Message protocol** uses JSON serialization with length-prefixed binary frames

## Architecture

### Transport Mechanisms

The crate provides platform-specific IPC implementations:

#### Unix/Linux (Unix Domain Sockets)
- **Transport**: Unix domain sockets via `tokio::net::UnixStream`
- **Socket Path**: 
  - `$XDG_RUNTIME_DIR/bunctl.sock` (preferred)
  - `/tmp/bunctl.sock` (fallback)
- **Security**: File system permissions control access

#### Windows (Named Pipes)
- **Transport**: Windows named pipes via `tokio::net::windows::named_pipe`
- **Pipe Name**: `\\.\pipe\bunctl_{path_basename}` or `\\.\pipe\bunctl_default`
- **Security**: Windows security descriptors control access
- **Multi-client**: Automatic next instance creation for concurrent connections

### Protocol Design

The IPC protocol uses a simple length-prefixed message format:

```
[ 4-byte length (little-endian) ][ JSON payload ]
```

- **Length**: 32-bit unsigned integer indicating payload size
- **Payload**: JSON-serialized message using serde_json
- **Encoding**: UTF-8 for JSON strings

## Message Types

### Request Messages (IpcMessage)

The client can send the following command messages:

```rust
pub enum IpcMessage {
    // Process management
    Start { name: String, config: String },
    Stop { name: String },
    Restart { name: String },
    Delete { name: String },
    
    // Information queries
    Status { name: Option<String> },  // None = all apps
    List,                             // List all apps
    Logs { name: Option<String>, lines: usize },
    
    // Event subscriptions
    Subscribe { subscription: SubscriptionType },
    Unsubscribe,
}
```

### Response Messages (IpcResponse)

The daemon responds with one of these message types:

```rust
pub enum IpcResponse {
    Success { message: String },           // Command succeeded
    Error { message: String },             // Command failed
    Data { data: serde_json::Value },      // Query results
    Event { event_type: String, data: serde_json::Value }, // Subscribed events
}
```

### Subscription Types

Clients can subscribe to real-time events:

```rust
pub enum SubscriptionType {
    StatusEvents { app_name: Option<String> },  // Process lifecycle events
    LogEvents { app_name: Option<String> },     // Application log lines
    AllEvents { app_name: Option<String> },     // All event types
}
```

Event types include:
- `status_change` - Application state transitions
- `process_started` - Process successfully started
- `process_exited` - Process exited normally
- `process_crashed` - Process crashed or failed
- `process_restarting` - Process restart initiated
- `log_line` - New log output from application

## API Documentation

### Server-side (Daemon)

#### Creating an IPC Server

```rust
use bunctl_ipc::{IpcServer, IpcConnection};

// Create server bound to default socket path
let mut server = IpcServer::bind("/tmp/bunctl.sock").await?;

// Accept client connections
let mut connection = server.accept().await?;
```

#### Processing Messages

```rust
use bunctl_ipc::{IpcMessage, IpcResponse};

// Receive command from client
let message = connection.recv().await?;

// Process the command
let response = match message {
    IpcMessage::List => {
        // Implementation logic here
        IpcResponse::Data { 
            data: serde_json::json!({ "apps": apps_list })
        }
    },
    IpcMessage::Start { name, config } => {
        // Implementation logic here
        IpcResponse::Success { 
            message: format!("Started application: {}", name) 
        }
    },
    // ... other commands
};

// Send response back to client
connection.send(&response).await?;
```

### Client-side (CLI)

#### Connecting to Daemon

```rust
use bunctl_ipc::{IpcClient, IpcMessage, IpcResponse};

// Connect to daemon
let mut client = IpcClient::connect("/tmp/bunctl.sock").await?;
```

#### Sending Commands

```rust
// Send a command
let message = IpcMessage::Status { name: Some("myapp".to_string()) };
client.send(&message).await?;

// Receive response
let response = client.recv().await?;
match response {
    IpcResponse::Data { data } => {
        println!("Status: {}", data);
    },
    IpcResponse::Error { message } => {
        eprintln!("Error: {}", message);
    },
    _ => {}
}
```

#### Event Subscriptions

```rust
use bunctl_ipc::SubscriptionType;

// Subscribe to status events for all apps
let subscription = IpcMessage::Subscribe { 
    subscription: SubscriptionType::StatusEvents { app_name: None } 
};
client.send(&subscription).await?;

// Listen for events
loop {
    match client.recv().await? {
        IpcResponse::Event { event_type, data } => {
            println!("Event {}: {:?}", event_type, data);
        },
        _ => {}
    }
}
```

## Platform-Specific Implementation Details

### Unix Domain Sockets (Unix/Linux)

**File**: `src/unix.rs`

- Uses `tokio::net::UnixListener` for server
- Uses `tokio::net::UnixStream` for client connections
- Socket file is removed on server bind to handle stale sockets
- Supports concurrent connections through async accept loop

### Named Pipes (Windows)

**File**: `src/windows.rs`

- Uses `tokio::net::windows::named_pipe::NamedPipeServer`
- Uses `tokio::net::windows::named_pipe::NamedPipeClient`
- Creates pipe instances with `first_pipe_instance(true)` for servers
- Automatically creates next server instance after each client connection
- Enhanced logging for debugging Windows pipe operations

## Security Considerations

### Unix Systems
- Socket files use filesystem permissions for access control
- Default socket path in `/tmp` may be world-readable; use `XDG_RUNTIME_DIR` when available
- Socket file is removed and recreated on daemon startup to prevent stale socket issues

### Windows Systems
- Named pipes use Windows security descriptors
- Pipe names are predictable but require appropriate Windows permissions
- Named pipe security depends on the user context running the daemon

### General Security
- No authentication mechanism beyond OS-level access controls
- All communication is local-only (no network exposure)
- JSON messages are validated using serde for type safety
- No encryption as communication is within same machine

## Performance Characteristics

### Message Throughput
- **Serialization**: JSON via serde_json (optimized for readability over speed)
- **Transport**: Native OS primitives provide minimal overhead
- **Buffering**: Uses tokio's buffered I/O for efficient reads/writes
- **Memory**: Length-prefixed protocol avoids unnecessary allocations

### Connection Handling
- **Single-threaded**: Each connection is handled in its own async task
- **Concurrent**: Multiple CLI clients can connect simultaneously
- **Persistent**: Daemon maintains long-lived connections for subscriptions
- **Cleanup**: Connections are automatically cleaned up when clients disconnect

### Scalability Limits
- Unix domain sockets: Limited by file descriptor limits
- Named pipes: Limited by Windows pipe instance limits
- Memory usage: Approximately 1KB per active connection
- CPU usage: Minimal when idle, scales with message frequency

## Integration with Daemon Mode

### Daemon Lifecycle

1. **Startup**: Daemon creates IPC server on configured socket path
2. **Discovery**: CLI commands attempt connection to default socket path
3. **Auto-start**: If daemon not running, CLI can auto-start daemon process
4. **Shutdown**: Daemon gracefully closes all IPC connections on exit

### Command Flow

```
CLI Command → IPC Client → Socket/Pipe → IPC Server → Daemon Handler
     ↑                                                        ↓
Response ← IPC Client ← Socket/Pipe ← IPC Server ← Command Result
```

### Event Broadcasting

The daemon maintains a registry of subscribed clients and broadcasts events:

1. **Subscription**: Client sends `Subscribe` message with filter criteria
2. **Registration**: Daemon adds client to subscriber list
3. **Event Generation**: Process events trigger notifications
4. **Broadcasting**: Daemon sends `Event` responses to matching subscribers
5. **Cleanup**: Dead connections are automatically removed from subscriber list

## Configuration

### Socket Path Configuration

Default socket paths are determined by `bunctl_core::config::default_socket_path()`:

- **Unix**: `$XDG_RUNTIME_DIR/bunctl.sock` or `/tmp/bunctl.sock`
- **Windows**: `\\.\pipe\bunctl`

Custom socket paths can be specified via:
- `--socket` CLI argument
- `socket_path` in daemon configuration
- `BUNCTL_SOCKET` environment variable (if implemented)

### Error Handling

The crate uses `bunctl_core::Result<T>` for consistent error handling:

- **IO Errors**: Network/socket failures map to `bunctl_core::Error::Io`
- **Serialization Errors**: JSON parsing failures map to `bunctl_core::Error::Other`
- **Connection Errors**: Transport-specific errors are wrapped appropriately

## Dependencies

- `bunctl-core`: Core types and error handling
- `tokio`: Async runtime and networking
- `serde`/`serde_json`: Message serialization
- `bytes`: Efficient byte buffer handling
- `anyhow`/`thiserror`: Error handling utilities
- `tracing`: Structured logging
- `windows-sys`: Windows-specific APIs (Windows only)

## Usage Examples

See the main `bunctl` CLI implementation for real-world usage examples:
- Command handlers in `bunctl/src/commands/`
- Daemon implementation in `bunctl/src/daemon.rs`
- Client connection patterns throughout the CLI codebase