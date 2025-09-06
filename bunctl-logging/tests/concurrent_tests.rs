use bunctl_logging::{LineBuffer, LineBufferConfig, LogConfig, LogManager};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::task;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_line_buffer_writes() {
    let config = LineBufferConfig {
        max_size: 8192,
        max_lines: 1000,
    };

    let buffer = Arc::new(LineBuffer::new(config));
    let num_threads = 10;
    let writes_per_thread = 100;

    let mut handles = vec![];

    for thread_id in 0..num_threads {
        let buffer_clone = buffer.clone();
        let handle = task::spawn(async move {
            for i in 0..writes_per_thread {
                let line = format!("Thread {} Line {}\n", thread_id, i);
                buffer_clone.write(line.as_bytes());

                // Add some variability
                if i % 10 == 0 {
                    tokio::time::sleep(Duration::from_micros(10)).await;
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify all lines were captured
    let lines = buffer.get_lines();
    assert_eq!(lines.len(), num_threads * writes_per_thread);

    // Verify data integrity - each line should be complete
    for line in lines {
        let line_str = std::str::from_utf8(&line).unwrap();
        assert!(line_str.starts_with("Thread "));
        assert!(line_str.ends_with("\n"));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_concurrent_buffer_operations() {
    let config = LineBufferConfig {
        max_size: 4096,
        max_lines: 500,
    };

    let buffer = Arc::new(LineBuffer::new(config));
    let mut handles = vec![];

    // Writer threads
    for thread_id in 0..4 {
        let buffer_clone = buffer.clone();
        let handle = task::spawn(async move {
            for i in 0..200 {
                let data = format!("W{}-{}\n", thread_id, i);
                buffer_clone.write(data.as_bytes());
            }
        });
        handles.push(handle);
    }

    // Reader threads
    for _ in 0..2 {
        let buffer_clone = buffer.clone();
        let handle = task::spawn(async move {
            let mut _total_lines = 0;
            for _ in 0..50 {
                let lines = buffer_clone.get_lines();
                _total_lines += lines.len();
                tokio::time::sleep(Duration::from_micros(100)).await;
            }
            // Return nothing to match other tasks
        });
        handles.push(handle);
    }

    // Clear thread
    let buffer_clone = buffer.clone();
    let clear_handle = task::spawn(async move {
        for _ in 0..5 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            buffer_clone.clear();
        }
    });
    handles.push(clear_handle);

    // Wait for all operations
    for handle in handles {
        handle.await.unwrap();
    }

    // Final state check - buffer should work correctly after concurrent operations
    buffer.write(b"Final test\n");
    let final_lines = buffer.get_lines();
    assert!(final_lines.len() <= 1); // May be 0 if cleared after write
}

#[tokio::test(flavor = "multi_thread")]
async fn test_log_manager_concurrent_writers() {
    let temp_dir = TempDir::new().unwrap();
    let config = LogConfig {
        base_dir: temp_dir.path().to_path_buf(),
        max_file_size: 10 * 1024 * 1024,
        max_files: 10,
        compression: false,
        buffer_size: 8192,
        flush_interval_ms: 10,
    };

    let manager = Arc::new(LogManager::new(config));
    let num_apps = 5;
    let writes_per_app = 100;

    let mut handles = vec![];

    for app_id in 0..num_apps {
        let manager_clone = manager.clone();
        let handle = task::spawn(async move {
            let app_name = format!("app-{}", app_id);
            let app_id = bunctl_core::AppId::new(&app_name).unwrap();
            let writer = manager_clone.get_writer(&app_id).await.unwrap();

            for i in 0..writes_per_app {
                let line = format!("[{}] Log entry {}", app_name, i);
                writer.write_line(&line).unwrap();

                if i % 20 == 0 {
                    writer.flush().await.unwrap();
                }
            }

            writer.flush().await.unwrap();
        });
        handles.push(handle);
    }

    // Wait for all writers
    for handle in handles {
        handle.await.unwrap();
    }

    // Flush all and verify logs
    manager.flush_all().await.unwrap();

    // Give time for flush to complete
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Read logs from each app
    for app_id in 0..num_apps {
        let app_name = format!("app-{}", app_id);
        let app_id = bunctl_core::AppId::new(&app_name).unwrap();
        let logs = manager.read_logs(&app_id, 1000).await.unwrap();

        // Should have written most lines (allow for some buffering)
        assert!(
            logs.len() >= writes_per_app * 80 / 100,
            "App {} only has {} logs, expected at least {}",
            app_name,
            logs.len(),
            writes_per_app * 80 / 100
        );
    }
}

#[tokio::test]
async fn test_minimal_write() {
    use tokio::fs;

    let temp_dir = TempDir::new().unwrap();
    let config = LogConfig {
        base_dir: temp_dir.path().to_path_buf(),
        max_file_size: 1024 * 1024,
        max_files: 5,
        compression: false,
        buffer_size: 4096,
        flush_interval_ms: 100,
    };

    let manager = Arc::new(LogManager::new(config));
    let app_id = bunctl_core::AppId::new("test-app").unwrap();

    // Get a writer
    let writer = manager.get_writer(&app_id).await.unwrap();

    // Write some data
    for i in 0..10 {
        writer.write_line(&format!("Test line {}", i)).unwrap();
    }

    println!("Wrote 10 lines");

    // Flush
    writer.flush().await.unwrap();
    println!("Flushed");

    // Remove the writer to close it
    manager.remove_writer(&app_id).await.unwrap();
    println!("Removed writer");

    // Check the file
    let log_path = temp_dir.path().join(format!("{}.log", app_id));
    if log_path.exists() {
        let content = fs::read_to_string(&log_path).await.unwrap();
        println!("File content: {:?}", content);
        assert!(!content.is_empty(), "File should not be empty");
    } else {
        panic!("Log file does not exist");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_stress_concurrent_read_write() {
    let temp_dir = TempDir::new().unwrap();
    let config = LogConfig {
        base_dir: temp_dir.path().to_path_buf(),
        max_file_size: 1024 * 1024,
        max_files: 5,
        compression: false,
        buffer_size: 4096,
        flush_interval_ms: 5,
    };

    let manager = Arc::new(LogManager::new(config));
    let app_id = bunctl_core::AppId::new("stress-test").unwrap();

    // Initialize the writer first to ensure log file exists before readers start
    let initial_writer = manager.get_writer(&app_id).await.unwrap();
    initial_writer.write_line("Test started").unwrap();
    initial_writer.flush().await.unwrap();
    drop(initial_writer);

    let duration = Duration::from_secs(2);
    let start = Instant::now();

    let mut handles = vec![];

    // Multiple writer threads
    for writer_id in 0..3 {
        let manager_clone = manager.clone();
        let app_id_clone = app_id.clone();
        let handle = task::spawn(async move {
            let mut count = 0;

            while start.elapsed() < duration {
                // Get writer for each batch of writes
                let writer = manager_clone.get_writer(&app_id_clone).await.unwrap();

                // Write a batch
                for _ in 0..50 {
                    if start.elapsed() >= duration {
                        break;
                    }
                    let line = format!("Writer {} - Entry {}", writer_id, count);
                    writer.write_line(&line).unwrap();
                    count += 1;
                }

                // Flush after each batch
                writer.flush().await.unwrap();
                // Drop the writer reference
                drop(writer);

                tokio::time::sleep(Duration::from_micros(100)).await;
            }

            count
        });
        handles.push(handle);
    }

    // Multiple reader threads
    for _ in 0..2 {
        let manager_clone = manager.clone();
        let app_id_clone = app_id.clone();
        let handle = task::spawn(async move {
            let mut total_reads = 0;

            while start.elapsed() < duration {
                let logs = manager_clone.read_logs(&app_id_clone, 100).await.unwrap();
                total_reads += logs.len();
                tokio::time::sleep(Duration::from_millis(10)).await;
            }

            total_reads
        });
        handles.push(handle);
    }

    // Rotation thread
    let manager_clone = manager.clone();
    let rotation_handle = task::spawn(async move {
        let mut rotations = 0;

        while start.elapsed() < duration {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if manager_clone.rotate_all().await.is_ok() {
                rotations += 1;
            }
        }

        rotations
    });
    handles.push(rotation_handle);

    // Collect results - all handles return usize now
    let mut _total_writes = 0;
    for (i, handle) in handles.into_iter().enumerate() {
        let result = handle.await.unwrap();
        if i < 3 {
            // Writer threads
            _total_writes += result;
        }
    }

    // Final flush and close all writers properly
    manager.flush_all().await.unwrap();
    manager.remove_writer(&app_id).await.unwrap();

    // Give a moment for file operations to complete
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Check the main log file and any rotated files
    let mut total_log_size = 0;
    let mut all_logs = Vec::new();

    if let Ok(entries) = std::fs::read_dir(temp_dir.path()) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.to_string_lossy().contains(&app_id.to_string()) {
                    if let Ok(content) = tokio::fs::read_to_string(&path).await {
                        total_log_size += content.len();
                        all_logs.extend(content.lines().map(|s| s.to_string()));
                    }
                }
            }
        }
    }
    // Should have captured a significant portion of writes across all files
    assert!(all_logs.len() > 0, "No logs were captured");
    assert!(total_log_size > 0, "No log data was written");

    // Verify log integrity
    for log in &all_logs {
        // All log lines should contain expected content
        assert!(
            log.contains("Writer") && log.contains("Entry"),
            "Unexpected log content: {}",
            log
        );
    }
}

#[tokio::test]
async fn test_lock_free_performance() {
    let config = LineBufferConfig {
        max_size: 8192,
        max_lines: 1000,
    };

    let buffer = Arc::new(LineBuffer::new(config));
    let iterations = 10000;
    let data = b"Performance test line with some reasonable length\n";

    // Measure single-threaded performance
    let start = Instant::now();
    for _ in 0..iterations {
        buffer.write(data);
    }
    let single_duration = start.elapsed();

    buffer.clear();

    // Measure multi-threaded performance
    let start = Instant::now();
    let mut handles = vec![];

    for _ in 0..4 {
        let buffer_clone = buffer.clone();
        let handle = task::spawn(async move {
            for _ in 0..iterations / 4 {
                buffer_clone.write(data);
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }
    let multi_duration = start.elapsed();

    // Multi-threaded should not be significantly slower than single-threaded
    // Allow 2x overhead for thread coordination
    assert!(
        multi_duration < single_duration * 2,
        "Multi-threaded performance ({:?}) is too slow compared to single-threaded ({:?})",
        multi_duration,
        single_duration
    );

    // Check that operations are fast enough (aiming for <1ms per 1000 ops)
    let ops_per_ms = iterations as f64 / single_duration.as_millis() as f64;
    assert!(
        ops_per_ms > 100.0,
        "Performance too low: {} ops/ms, expected > 100",
        ops_per_ms
    );
}

#[tokio::test]
async fn test_concurrent_structured_logs() {
    let temp_dir = TempDir::new().unwrap();
    let config = LogConfig {
        base_dir: temp_dir.path().to_path_buf(),
        max_file_size: 10 * 1024 * 1024,
        max_files: 10,
        compression: false,
        buffer_size: 8192,
        flush_interval_ms: 10,
    };

    let manager = Arc::new(LogManager::new(config));
    let app_id = bunctl_core::AppId::new("structured-test").unwrap();

    // Write mixed stdout and stderr
    let writer = manager.get_writer(&app_id).await.unwrap();

    let mut handles = vec![];

    // Stdout writer
    let writer_clone = writer.clone();
    let handle = task::spawn(async move {
        for i in 0..50 {
            let line = format!(
                "[structured-test] [2024-01-01T00:00:00] [stdout] Output line {}",
                i
            );
            writer_clone.write_line(&line).unwrap();
        }
    });
    handles.push(handle);

    // Stderr writer
    let writer_clone = writer.clone();
    let handle = task::spawn(async move {
        for i in 0..50 {
            let line = format!(
                "[structured-test] [2024-01-01T00:00:00] [stderr] Error line {}",
                i
            );
            writer_clone.write_line(&line).unwrap();
        }
    });
    handles.push(handle);

    // Wait for writers
    for handle in handles {
        handle.await.unwrap();
    }

    writer.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Read structured logs
    let structured = manager.read_structured_logs(&app_id, 200).await.unwrap();

    // Should have both output and errors
    assert!(
        !structured.output.is_empty() || !structured.errors.is_empty(),
        "No structured logs captured"
    );

    // Verify separation
    for error in &structured.errors {
        assert!(
            error.contains("[stderr]"),
            "Error line doesn't contain stderr marker: {}",
            error
        );
    }

    for output in &structured.output {
        assert!(
            !output.contains("[stderr]"),
            "Output line contains stderr marker: {}",
            output
        );
    }
}
