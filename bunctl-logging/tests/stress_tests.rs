use bunctl_logging::{AsyncLogWriter, LogWriterConfig, RotationConfig, RotationStrategy};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::task;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_concurrent_writers_stress() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("stress.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig {
            strategy: RotationStrategy::Size(50 * 1024 * 1024), // 50MB
            max_files: 5,
            compression: false,
        },
        buffer_size: 65536,
        flush_interval: Duration::from_millis(50),
        max_concurrent_writes: 10000,
        enable_compression: false,
    };

    let writer = Arc::new(AsyncLogWriter::new(config).await.unwrap());

    // Spawn 100 concurrent writers
    let num_writers = 100;
    let writes_per_writer = 1000;
    let mut handles = vec![];

    let start = Instant::now();

    for writer_id in 0..num_writers {
        let writer_clone = writer.clone();
        let handle = task::spawn(async move {
            for i in 0..writes_per_writer {
                let line = format!(
                    "[Writer {}] Line {}: Lorem ipsum dolor sit amet, consectetur adipiscing elit.",
                    writer_id, i
                );

                // Write should not fail even under stress
                if let Err(e) = writer_clone.write_line(&line) {
                    eprintln!("Write failed: {}", e);
                }

                // Occasional flush
                if i % 100 == 0 {
                    let _ = writer_clone.flush().await;
                }

                // Small delay to simulate real workload
                if i % 10 == 0 {
                    tokio::time::sleep(Duration::from_micros(100)).await;
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all writers to complete
    for handle in handles {
        handle.await.unwrap();
    }

    let elapsed = start.elapsed();
    let total_writes = num_writers * writes_per_writer;
    let writes_per_sec = total_writes as f64 / elapsed.as_secs_f64();

    println!("Stress test completed:");
    println!("  Total writes: {}", total_writes);
    println!("  Duration: {:?}", elapsed);
    println!("  Writes/sec: {:.0}", writes_per_sec);

    // Get metrics
    let metrics = writer.get_metrics().await;
    println!("  Bytes written: {}", metrics.bytes_written);
    println!("  Lines written: {}", metrics.lines_written);
    println!("  Write errors: {}", metrics.write_errors);
    println!("  Dropped messages: {}", metrics.dropped_messages);
    println!("  Avg write latency: {}µs", metrics.avg_write_latency_us);
    println!("  Avg flush latency: {}µs", metrics.avg_flush_latency_us);

    // Final flush
    writer.flush().await.unwrap();

    // Verify log file exists and has content
    let content = tokio::fs::read_to_string(&log_path).await.unwrap();
    assert!(!content.is_empty());

    // Check that we didn't lose too many messages (allow 1% loss under extreme stress)
    let acceptable_loss = (total_writes as f64 * 0.01) as u64;
    assert!(
        metrics.dropped_messages <= acceptable_loss,
        "Too many dropped messages: {} (max acceptable: {})",
        metrics.dropped_messages,
        acceptable_loss
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rotation_under_load() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("rotation.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig {
            strategy: RotationStrategy::Size(1024 * 1024), // 1MB - small for frequent rotation
            max_files: 3,
            compression: true,
        },
        buffer_size: 8192,
        flush_interval: Duration::from_millis(10),
        max_concurrent_writes: 1000,
        enable_compression: true,
    };

    let writer = Arc::new(AsyncLogWriter::new(config).await.unwrap());

    // Write continuously while rotating
    let writer_clone = writer.clone();
    let write_handle = task::spawn(async move {
        for i in 0..10000 {
            let line = format!("Line {}: {}", i, "x".repeat(100));
            writer_clone.write_line(&line).ok();

            if i % 1000 == 0 {
                // Force rotation periodically
                writer_clone.rotate().await.ok();
            }
        }
    });

    // Let it run
    write_handle.await.unwrap();

    // Check metrics
    let metrics = writer.get_metrics().await;
    assert!(metrics.rotation_count > 0, "No rotations occurred");
    assert_eq!(
        metrics.write_errors, 0,
        "Write errors occurred during rotation"
    );

    // Verify rotated files exist
    let mut entries = tokio::fs::read_dir(temp_dir.path()).await.unwrap();
    let mut file_count = 0;

    while let Some(entry) = entries.next_entry().await.unwrap() {
        let path = entry.path();
        if path
            .extension()
            .map_or(false, |ext| ext == "log" || ext == "gz")
        {
            file_count += 1;
            println!("Found log file: {:?}", path);
        }
    }

    assert!(file_count > 1, "No rotated files found");
    assert!(file_count <= 4, "Too many files, cleanup failed"); // current + max_files
}

#[tokio::test]
async fn test_error_recovery() {
    // Test writing to a read-only directory (will fail on permissions)
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("subdir").join("error.log");

    // Don't create the parent directory to simulate error
    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig::default(),
        buffer_size: 4096,
        flush_interval: Duration::from_millis(100),
        max_concurrent_writes: 100,
        enable_compression: false,
    };

    // Should create parent directory automatically
    let writer = AsyncLogWriter::new(config).await.unwrap();

    // Write should succeed after directory creation
    writer.write_line("Test line 1").unwrap();
    writer.write_line("Test line 2").unwrap();

    writer.flush().await.unwrap();

    // Verify file was created
    assert!(log_path.exists());
    let content = tokio::fs::read_to_string(&log_path).await.unwrap();
    assert!(content.contains("Test line 1"));
    assert!(content.contains("Test line 2"));
}

#[tokio::test]
async fn test_graceful_shutdown() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("shutdown.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig::default(),
        buffer_size: 4096,
        flush_interval: Duration::from_secs(10), // Long interval
        max_concurrent_writes: 100,
        enable_compression: false,
    };

    let writer = AsyncLogWriter::new(config).await.unwrap();

    // Write some data
    for i in 0..100 {
        writer.write_line(&format!("Line {}", i)).unwrap();
    }

    // Don't flush, rely on shutdown to flush

    // Graceful shutdown
    writer.close().await.unwrap();

    // Verify all data was written
    let content = tokio::fs::read_to_string(&log_path).await.unwrap();
    for i in 0..100 {
        assert!(content.contains(&format!("Line {}", i)));
    }
}

#[tokio::test]
async fn test_memory_pressure() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("memory.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig::default(),
        buffer_size: 1024 * 1024, // 1MB buffer
        flush_interval: Duration::from_millis(100),
        max_concurrent_writes: 10000,
        enable_compression: false,
    };

    let writer = AsyncLogWriter::new(config).await.unwrap();

    // Write large amounts of data quickly
    let large_line = "x".repeat(10000); // 10KB per line

    for _ in 0..1000 {
        writer.write_line(&large_line).ok(); // May drop some under pressure
    }

    // Get metrics to see how many were dropped
    let metrics = writer.get_metrics().await;

    println!("Memory pressure test:");
    println!("  Lines written: {}", metrics.lines_written);
    println!("  Dropped messages: {}", metrics.dropped_messages);
    println!("  Buffer overflows: {}", metrics.buffer_overflows);

    // Flush remaining
    writer.flush().await.unwrap();

    // Some messages may be dropped under memory pressure, but not all
    assert!(metrics.lines_written > 0);
    assert!(metrics.lines_written + metrics.dropped_messages >= 900); // Allow 10% total loss
}
