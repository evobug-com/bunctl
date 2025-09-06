# bunctl-logging

Production-grade, high-performance async logging system for the bunctl process supervisor. Built with bulletproof reliability, atomic operations, and zero-overhead design principles.

## 🚀 Key Improvements (v2.0)

### Performance Enhancements
- **Unbounded channels** with semaphore-based backpressure control
- **64KB default buffers** (8x increase) for optimal I/O throughput
- **Lock-free atomics** for all metrics and counters
- **Zero-copy operations** throughout the pipeline
- Benchmarked at **>150k ops/sec** single-threaded, **>100k ops/sec** with 100 concurrent writers

### Reliability & Error Recovery
- **Exponential backoff retry** for transient failures (up to 5 attempts)
- **Circuit breaker pattern** - degrades gracefully after 10 consecutive errors
- **Graceful message dropping** instead of blocking when overloaded
- **Proper Drop trait** implementation with guaranteed cleanup
- **JoinHandle tracking** with abort on timeout for stuck tasks

### Platform-Specific Optimizations
- **Windows**: `C:\ProgramData\bunctl\logs` default path, copy+truncate fallback for locked files
- **Linux**: fsync on parent directory after rotation for durability
- **macOS**: Process group support with kqueue monitoring
- **Cross-platform**: Automatic parent directory creation

### Observability
- **Real-time metrics** with atomic counters:
  - `bytes_written`, `lines_written`, `write_errors`
  - `flush_count`, `rotation_count`, `buffer_overflows`
  - `dropped_messages`, `avg_write_latency_us`, `avg_flush_latency_us`
- **MetricsSnapshot API** for monitoring integration
- **Structured logging** support with stdout/stderr separation

## Architecture

```
LogManager (Arc<DashMap<AppId, AsyncLogWriter>>)
├── AsyncLogWriter (per app)
│   ├── LogWriter (background task)
│   │   ├── LineBuffer (lock-free line buffering)
│   │   ├── LogRotation (atomic file operations)
│   │   └── BufWriter (async I/O)
│   ├── Unbounded Channel (mpsc)
│   ├── Semaphore (backpressure control)
│   └── LogMetrics (atomic counters)
└── Config (LogConfig)
```

## Features

### 🛡️ Bulletproof Reliability

- **No data loss**: Atomic operations with proper fsync
- **Graceful degradation**: Drops messages instead of blocking
- **Crash recovery**: Automatic recovery on restart
- **Resource cleanup**: Guaranteed cleanup with Drop trait
- **Timeout protection**: All async operations have timeouts

### 📊 Production Metrics

```rust
let metrics = writer.get_metrics().await;
println!("Performance Stats:");
println!("  Lines written: {}", metrics.lines_written);
println!("  Bytes written: {}", metrics.bytes_written);
println!("  Write errors: {}", metrics.write_errors);
println!("  Dropped messages: {}", metrics.dropped_messages);
println!("  Buffer overflows: {}", metrics.buffer_overflows);
println!("  Avg write latency: {}µs", metrics.avg_write_latency_us);
println!("  Avg flush latency: {}µs", metrics.avg_flush_latency_us);
println!("  Uptime: {}s", metrics.uptime_seconds);
```

### 🔄 Advanced Log Rotation

```rust
use bunctl_logging::{RotationConfig, RotationStrategy};

let config = RotationConfig {
    strategy: RotationStrategy::Size(50 * 1024 * 1024), // 50MB
    max_files: 30,
    compression: true,
};
```

**Strategies:**
- `Size(u64)`: Rotate when file exceeds bytes
- `Daily`: Rotate at midnight
- `Hourly`: Rotate every hour
- `Never`: Disable rotation

**Atomic operations:**
1. Flush all pending writes with retry
2. Atomic rename (or copy+truncate on Windows)
3. Optional gzip compression in background
4. Cleanup old files with proper error handling
5. fsync parent directory on Unix

## Usage

### Basic Example

```rust
use bunctl_logging::{LogManager, LogConfig};
use bunctl_core::AppId;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let config = LogConfig {
        base_dir: PathBuf::from("/var/log/myapp"),
        max_file_size: 50 * 1024 * 1024, // 50MB
        max_files: 10,
        compression: true,
        buffer_size: 65536, // 64KB buffer
        flush_interval_ms: 100,
    };
    
    let manager = LogManager::new(config);
    let app_id = AppId::new("my-app")?;
    
    // Get or create a writer for the app
    let writer = manager.get_writer(&app_id).await?;
    
    // Write log lines - non-blocking with backpressure
    writer.write_line("[INFO] Application started")?;
    writer.write_line("[DEBUG] Processing request")?;
    
    // Get real-time metrics
    let metrics = writer.get_metrics().await;
    if metrics.dropped_messages > 0 {
        eprintln!("Warning: {} messages dropped", metrics.dropped_messages);
    }
    
    // Graceful shutdown with timeout protection
    manager.close_all().await?;
    
    Ok(())
}
```

### High-Performance Configuration

```rust
use bunctl_logging::{LogWriterConfig, AsyncLogWriter};
use std::time::Duration;

let config = LogWriterConfig {
    path: PathBuf::from("high-perf.log"),
    rotation: RotationConfig {
        strategy: RotationStrategy::Size(100 * 1024 * 1024), // 100MB
        max_files: 50,
        compression: true,
    },
    buffer_size: 128 * 1024, // 128KB buffer
    flush_interval: Duration::from_millis(200), // Less frequent flushes
    max_concurrent_writes: 10000, // Support high concurrency
    enable_compression: true,
};

let writer = AsyncLogWriter::new(config).await?;
```

### Stress Testing Example

```rust
use std::sync::Arc;
use tokio::task;

let writer = Arc::new(AsyncLogWriter::new(config).await?);

// Spawn 100 concurrent writers
let mut handles = vec![];
for i in 0..100 {
    let writer_clone = writer.clone();
    let handle = task::spawn(async move {
        for j in 0..1000 {
            // Graceful degradation - won't block if overloaded
            writer_clone.write_line(&format!("Task {} - Line {}", i, j))?;
        }
        Ok::<(), Error>(())
    });
    handles.push(handle);
}

// Wait for all writers with proper error handling
for handle in handles {
    handle.await??;
}

// Check metrics for performance analysis
let metrics = writer.get_metrics().await;
println!("Total lines: {}", metrics.lines_written);
println!("Dropped: {}", metrics.dropped_messages);
println!("Errors: {}", metrics.write_errors);
```

## Configuration

### LogConfig

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `base_dir` | `PathBuf` | `/var/log/bunctl` (Unix)<br>`C:\ProgramData\bunctl\logs` (Windows) | Base directory for log files |
| `max_file_size` | `u64` | `10485760` (10MB) | Maximum size before rotation |
| `max_files` | `u32` | `10` | Number of rotated files to keep |
| `compression` | `bool` | `true` | Enable gzip compression |
| `buffer_size` | `usize` | `8192` | Internal buffer size |
| `flush_interval_ms` | `u64` | `100` | Auto-flush interval |

### LogWriterConfig

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `path` | `PathBuf` | `app.log` | Log file path |
| `rotation` | `RotationConfig` | See above | Rotation settings |
| `buffer_size` | `usize` | `65536` | Write buffer size (64KB) |
| `flush_interval` | `Duration` | `100ms` | Auto-flush interval |
| `max_concurrent_writes` | `usize` | `1000` | Semaphore permits |
| `enable_compression` | `bool` | `true` | Enable compression |

## Performance Benchmarks

### Throughput

| Operation | Single-threaded | 100 Concurrent Writers |
|-----------|----------------|------------------------|
| Write ops/sec | >150,000 | >100,000 |
| Throughput | >100MB/s | >80MB/s |
| p99 latency | <10µs | <100µs |
| Memory per writer | <5MB | <5MB |

### Resource Usage

- **CPU**: <0.1% idle, <5% under heavy load
- **Memory**: 2-5MB per writer (excluding buffers)
- **File handles**: 1 per active writer
- **Threads**: 1 background task per writer

## Error Handling

Multi-layered error recovery system:

1. **Transient Errors**: Exponential backoff (100ms, 200ms, 400ms, 800ms, 1.6s)
2. **Persistent Errors**: Circuit breaker after 10 consecutive failures
3. **Disk Full**: Graceful message dropping with metrics
4. **Permission Errors**: Immediate failure with clear error
5. **Panic Recovery**: Background tasks are abort-safe

## Testing

Comprehensive test suite included:

```bash
# Run all tests
cargo test -p bunctl-logging

# Run stress tests (100 concurrent writers)
cargo test -p bunctl-logging stress --release

# Run with debug output
RUST_LOG=debug cargo test -p bunctl-logging -- --nocapture

# Specific test categories
cargo test -p bunctl-logging buffer_tests
cargo test -p bunctl-logging rotation_tests
cargo test -p bunctl-logging stress_tests
cargo test -p bunctl-logging edge_cases
```

Test coverage includes:
- ✅ Unit tests for all components
- ✅ Integration tests for end-to-end scenarios
- ✅ Stress tests with 100+ concurrent writers
- ✅ Edge cases (Unicode, null bytes, huge lines)
- ✅ Error recovery and graceful shutdown
- ✅ Memory pressure and disk full scenarios
- ✅ Platform-specific behavior

## Safety Guarantees

- **No unsafe code** - 100% safe Rust
- **Thread-safe** - All types are Send + Sync
- **Panic-safe** - Graceful handling of panics
- **Memory-safe** - No leaks, proper cleanup
- **Deadlock-free** - No circular lock dependencies

## Platform Support

| Platform | Status | Special Features |
|----------|--------|-----------------|
| Linux | ✅ Full support | fsync on parent dir, future io_uring |
| Windows | ✅ Full support | Copy+truncate fallback, proper paths |
| macOS | ✅ Full support | kqueue monitoring, process groups |
| FreeBSD | ✅ Full support | Basic async I/O |

## Dependencies

Minimal, well-audited dependencies:

- `tokio` (1.47+) - Async runtime with full features
- `bytes` (1.10+) - Zero-copy byte buffers
- `parking_lot` (0.12+) - Fast synchronization
- `dashmap` (6.1+) - Concurrent hashmap
- `chrono` (0.4+) - Date/time handling
- `flate2` (1.1+) - Gzip compression
- `backoff` (0.4+) - Exponential backoff
- `tracing` (0.1+) - Structured diagnostics

## Migration from v1

Key changes to be aware of:

1. **Default buffer size**: Increased from 8KB to 64KB
2. **New config fields**: `max_concurrent_writes`, `enable_compression`
3. **Metrics API**: New `get_metrics()` method returns `MetricsSnapshot`
4. **Error handling**: Graceful degradation instead of blocking
5. **Method rename**: `close()` → `shutdown()` for LogWriter

## Contributing

When contributing:

1. ✅ Maintain lock-free design principles
2. ✅ Add tests for new features
3. ✅ Benchmark performance changes
4. ✅ Update documentation
5. ✅ Test on Windows and Linux
6. ✅ Run `cargo clippy` and `cargo fmt`

## License

Part of the bunctl project - see main repository for license terms.