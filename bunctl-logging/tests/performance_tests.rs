use bunctl_logging::{
    AsyncLogWriter, LineBuffer, LineBufferConfig, LogWriterConfig, RotationConfig, RotationStrategy,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::task;

#[tokio::test]
async fn test_p99_latency() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("latency.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig {
            strategy: RotationStrategy::Never,
            max_files: 5,
            compression: false,
        },
        buffer_size: 8192,
        flush_interval: Duration::from_millis(100),
    };

    let writer = Arc::new(AsyncLogWriter::new(config).await.unwrap());

    // Warm up
    for _ in 0..100 {
        writer.write_line("Warmup").unwrap();
    }
    writer.flush().await.unwrap();

    // Measure latencies
    let mut latencies = Vec::new();
    let iterations = 10000;

    for i in 0..iterations {
        let line = format!("Latency test line {}", i);
        let start = Instant::now();
        writer.write_line(&line).unwrap();
        let elapsed = start.elapsed();
        latencies.push(elapsed.as_micros());

        // Occasionally flush to simulate real usage
        if i % 100 == 0 {
            writer.flush().await.unwrap();
        }
    }

    // Sort latencies for percentile calculation
    latencies.sort_unstable();

    // Calculate percentiles
    let p50_idx = latencies.len() / 2;
    let p90_idx = (latencies.len() as f64 * 0.9) as usize;
    let p95_idx = (latencies.len() as f64 * 0.95) as usize;
    let p99_idx = (latencies.len() as f64 * 0.99) as usize;

    let p50 = latencies[p50_idx];
    let p90 = latencies[p90_idx];
    let p95 = latencies[p95_idx];
    let p99 = latencies[p99_idx];

    println!("Write Latencies (microseconds):");
    println!("  P50: {} µs", p50);
    println!("  P90: {} µs", p90);
    println!("  P95: {} µs", p95);
    println!("  P99: {} µs", p99);

    // Target: P99 < 1ms (1000 microseconds)
    assert!(p99 < 1000, "P99 latency ({} µs) exceeds 1ms target", p99);

    // Most operations should be very fast
    assert!(p50 < 100, "P50 latency ({} µs) is too high", p50);
}

#[tokio::test]
#[cfg_attr(windows, ignore = "May hang on Windows CI")]
async fn test_throughput() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("throughput.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig {
            strategy: RotationStrategy::Never,
            max_files: 5,
            compression: false,
        },
        buffer_size: 16384,
        flush_interval: Duration::from_millis(100),
    };

    let writer = Arc::new(AsyncLogWriter::new(config).await.unwrap());

    let line = "This is a typical log line with timestamp and some data: [2024-01-01 00:00:00] INFO: Processing request";
    let line_size = line.len() + 1; // +1 for newline

    let duration = Duration::from_secs(1);
    let start = Instant::now();
    let mut total_bytes = 0u64;
    let mut total_lines = 0u64;

    while start.elapsed() < duration {
        writer.write_line(line).unwrap();
        total_bytes += line_size as u64;
        total_lines += 1;

        // Flush periodically
        if total_lines % 1000 == 0 {
            writer.flush().await.unwrap();
        }
    }

    writer.flush().await.unwrap();
    let elapsed = start.elapsed();

    let throughput_mbps = (total_bytes as f64 / 1024.0 / 1024.0) / elapsed.as_secs_f64();
    let lines_per_sec = total_lines as f64 / elapsed.as_secs_f64();

    println!("Throughput Performance:");
    println!("  Total bytes written: {} MB", total_bytes / 1024 / 1024);
    println!("  Total lines written: {}", total_lines);
    println!("  Duration: {:?}", elapsed);
    println!("  Throughput: {:.2} MB/s", throughput_mbps);
    println!("  Lines/sec: {:.0}", lines_per_sec);

    // Should achieve reasonable throughput
    assert!(
        throughput_mbps > 10.0,
        "Throughput ({:.2} MB/s) is too low",
        throughput_mbps
    );

    assert!(
        lines_per_sec > 10000.0,
        "Lines per second ({:.0}) is too low",
        lines_per_sec
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_throughput() {
    let temp_dir = TempDir::new().unwrap();

    let num_writers = 4;
    let duration = Duration::from_secs(3);
    let mut handles = vec![];

    for writer_id in 0..num_writers {
        let log_path = temp_dir
            .path()
            .join(format!("concurrent_{}.log", writer_id));

        let config = LogWriterConfig {
            path: log_path,
            rotation: RotationConfig {
                strategy: RotationStrategy::Never,
                max_files: 5,
                compression: false,
            },
            buffer_size: 8192,
            flush_interval: Duration::from_millis(50),
        };

        let handle = task::spawn(async move {
            let writer = AsyncLogWriter::new(config).await.unwrap();
            let line = format!("Writer {} - Log line with some typical content", writer_id);

            let start = Instant::now();
            let mut count = 0u64;

            while start.elapsed() < duration {
                writer.write_line(&line).unwrap();
                count += 1;

                if count % 500 == 0 {
                    writer.flush().await.unwrap();
                }
            }

            writer.flush().await.unwrap();
            count
        });

        handles.push(handle);
    }

    let mut total_lines = 0u64;
    for handle in handles {
        total_lines += handle.await.unwrap();
    }

    let lines_per_sec = total_lines as f64 / duration.as_secs_f64();

    println!("Concurrent Throughput:");
    println!("  Writers: {}", num_writers);
    println!("  Total lines: {}", total_lines);
    println!("  Lines/sec: {:.0}", lines_per_sec);
    println!(
        "  Lines/sec/writer: {:.0}",
        lines_per_sec / num_writers as f64
    );

    // Should scale reasonably with multiple writers
    assert!(
        lines_per_sec > 20000.0,
        "Concurrent throughput ({:.0} lines/sec) is too low",
        lines_per_sec
    );
}

#[tokio::test]
async fn test_buffer_memory_usage() {
    let configs = vec![(1024, 100), (4096, 500), (8192, 1000), (16384, 2000)];

    for (max_size, max_lines) in configs {
        let config = LineBufferConfig {
            max_size,
            max_lines,
        };

        let buffer = LineBuffer::new(config);

        // Fill buffer to capacity
        let line = "x".repeat(100);
        for _ in 0..max_lines {
            buffer.write(format!("{}\n", line).as_bytes());
        }

        // Memory usage should be bounded
        let lines = buffer.get_lines();
        let total_size: usize = lines.iter().map(|l| l.len()).sum();

        // Should not exceed reasonable bounds
        let max_expected = max_size + (max_lines * 110); // Some overhead per line
        assert!(
            total_size <= max_expected,
            "Buffer memory usage {} exceeds expected maximum {}",
            total_size,
            max_expected
        );
    }
}

#[tokio::test]
async fn test_rotation_performance() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("rotation_perf.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig {
            strategy: RotationStrategy::Size(1024 * 1024), // 1MB
            max_files: 5,
            compression: false,
        },
        buffer_size: 8192,
        flush_interval: Duration::from_millis(50),
    };

    let writer = AsyncLogWriter::new(config).await.unwrap();

    // Write enough data to trigger multiple rotations
    let line = "x".repeat(1000);
    let mut rotation_times = Vec::new();

    for rotation_num in 0..5 {
        let start = Instant::now();

        // Write 1MB of data
        for _ in 0..1024 {
            writer.write_line(&line).unwrap();
        }

        writer.flush().await.unwrap();
        writer.rotate().await.unwrap();

        let elapsed = start.elapsed();
        rotation_times.push(elapsed);

        println!("Rotation {} took {:?}", rotation_num, elapsed);
    }

    // Rotations should be fast
    for (i, time) in rotation_times.iter().enumerate() {
        assert!(
            time.as_millis() < 500,
            "Rotation {} took too long: {:?}",
            i,
            time
        );
    }
}

#[tokio::test]
#[cfg_attr(windows, ignore = "Compression test may hang on Windows")]
async fn test_compression_performance() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("compression_perf.log");

    // Test with compression enabled
    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig {
            strategy: RotationStrategy::Size(5 * 1024 * 1024), // 5MB
            max_files: 3,
            compression: true,
        },
        buffer_size: 16384,
        flush_interval: Duration::from_millis(100),
    };

    let writer = AsyncLogWriter::new(config).await.unwrap();

    // Write repetitive data (should compress well)
    let line = "This is a repetitive log line that should compress well. ".repeat(10);

    let start = Instant::now();

    // Write 5MB of data
    for _ in 0..10000 {
        writer.write_line(&line).unwrap();
    }

    writer.flush().await.unwrap();

    // Trigger rotation with compression
    let rotation_start = Instant::now();
    writer.rotate().await.unwrap();
    let rotation_elapsed = rotation_start.elapsed();

    let total_elapsed = start.elapsed();

    println!("Compression Performance:");
    println!("  Total time: {:?}", total_elapsed);
    println!("  Rotation with compression: {:?}", rotation_elapsed);

    // Compression should complete in reasonable time
    assert!(
        rotation_elapsed.as_secs() < 5,
        "Compression took too long: {:?}",
        rotation_elapsed
    );
}

#[tokio::test]
#[cfg_attr(windows, ignore = "Memory stress test may hang on Windows")]
async fn test_stress_test_memory_stability() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("memory_stress.log");

    let config = LogWriterConfig {
        path: log_path,
        rotation: RotationConfig {
            strategy: RotationStrategy::Size(10 * 1024 * 1024), // 10MB
            max_files: 2,
            compression: false,
        },
        buffer_size: 8192,
        flush_interval: Duration::from_millis(10),
    };

    let writer = Arc::new(AsyncLogWriter::new(config).await.unwrap());

    // Run for a fixed duration with multiple threads
    let duration = Duration::from_secs(1);
    let mut handles = vec![];

    for thread_id in 0..4 {
        let writer_clone = writer.clone();
        let handle = task::spawn(async move {
            let start = Instant::now();
            let mut count = 0u64;

            while start.elapsed() < duration {
                // Vary line sizes
                let line = if count % 100 == 0 {
                    // Occasionally write large lines
                    format!("Thread {} - {}", thread_id, "x".repeat(10000))
                } else {
                    format!("Thread {} - Line {}", thread_id, count)
                };

                writer_clone.write_line(&line).unwrap();
                count += 1;

                // Occasionally flush
                if count % 1000 == 0 {
                    writer_clone.flush().await.unwrap();
                }

                // Occasionally rotate
                if count % 5000 == 0 {
                    writer_clone.rotate().await.unwrap();
                }
            }

            count
        });
        handles.push(handle);
    }

    let mut total_writes = 0u64;
    for handle in handles {
        total_writes += handle.await.unwrap();
    }

    println!("Memory Stress Test:");
    println!("  Total writes: {}", total_writes);
    println!(
        "  Writes/sec: {:.0}",
        total_writes as f64 / duration.as_secs_f64()
    );

    // Should complete without crashes or hangs
    assert!(total_writes > 10000, "Too few writes completed");
}

#[tokio::test]
async fn benchmark_line_buffer_operations() {
    let config = LineBufferConfig {
        max_size: 8192,
        max_lines: 1000,
    };

    let buffer = LineBuffer::new(config);
    let iterations = 100000;

    // Benchmark writes
    let data = b"Benchmark line\n";
    let start = Instant::now();
    for _ in 0..iterations {
        buffer.write(data);
    }
    let write_duration = start.elapsed();
    let writes_per_sec = iterations as f64 / write_duration.as_secs_f64();

    // Benchmark reads
    let start = Instant::now();
    for _ in 0..1000 {
        let _ = buffer.get_lines();
    }
    let read_duration = start.elapsed();
    let reads_per_sec = 1000.0 / read_duration.as_secs_f64();

    // Benchmark clear
    let start = Instant::now();
    for _ in 0..1000 {
        buffer.clear();
        buffer.write(data);
    }
    let clear_duration = start.elapsed();
    let clears_per_sec = 1000.0 / clear_duration.as_secs_f64();

    println!("LineBuffer Benchmarks:");
    println!("  Writes/sec: {:.0}", writes_per_sec);
    println!("  Reads/sec: {:.0}", reads_per_sec);
    println!("  Clears/sec: {:.0}", clears_per_sec);

    // Performance targets
    assert!(writes_per_sec > 1_000_000.0, "Write performance too low");
    assert!(reads_per_sec > 10_000.0, "Read performance too low");
    assert!(clears_per_sec > 10_000.0, "Clear performance too low");
}
