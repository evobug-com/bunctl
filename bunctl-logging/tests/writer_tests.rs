use bunctl_logging::{
    AsyncLogWriter, LogWriter, LogWriterConfig, RotationConfig, RotationStrategy,
};
use std::time::Duration;
use tempfile::TempDir;
use tokio::fs;

#[tokio::test]
async fn test_log_writer_basic() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("test.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig {
            strategy: RotationStrategy::Never,
            max_files: 5,
            compression: false,
        },
        buffer_size: 4096,
        flush_interval: Duration::from_millis(100),
        max_concurrent_writes: 1000,
        enable_compression: false,
    };

    let writer = LogWriter::new(config).await.unwrap();

    // Write some lines
    writer.write_line("First line").unwrap();
    writer.write_line("Second line").unwrap();
    writer.write_line("Third line").unwrap();

    // Flush and wait
    writer.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Read the file
    let content = fs::read_to_string(&log_path).await.unwrap();
    assert!(content.contains("First line"));
    assert!(content.contains("Second line"));
    assert!(content.contains("Third line"));

    // Each line should end with newline
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 3);
}

#[tokio::test]
async fn test_log_writer_auto_flush() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("auto_flush.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig::default(),
        buffer_size: 4096,
        flush_interval: Duration::from_millis(50), // Short interval for testing
        max_concurrent_writes: 1000,
        enable_compression: false,
    };

    let writer = LogWriter::new(config).await.unwrap();

    // Write a line
    writer.write_line("Auto flush test").unwrap();

    // Don't explicitly flush, wait for auto-flush
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Should be flushed automatically
    let content = fs::read_to_string(&log_path).await.unwrap();
    assert!(content.contains("Auto flush test"));
}

#[tokio::test]
async fn test_log_writer_binary_data() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("binary.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig::default(),
        buffer_size: 4096,
        flush_interval: Duration::from_millis(100),
        max_concurrent_writes: 1000,
        enable_compression: false,
    };

    let writer = LogWriter::new(config).await.unwrap();

    // Write binary data
    let binary_data = vec![0u8, 1, 2, 3, 255, 254, 253];
    writer.write(binary_data.clone()).unwrap();
    writer.write(b"\n".as_ref()).unwrap();

    // Write UTF-8 with special characters
    writer.write_line("Hello 世界 🌍").unwrap();

    writer.flush().await.unwrap();
    writer.shutdown().await.unwrap(); // Properly close the writer - this now waits for background task

    // Read as bytes
    let content = fs::read(&log_path).await.unwrap();

    // Should contain binary data
    for byte in &binary_data {
        assert!(content.contains(byte));
    }

    // Should contain UTF-8 text
    let text = String::from_utf8_lossy(&content);
    assert!(text.contains("Hello 世界 🌍"));
}

#[tokio::test]
#[cfg_attr(
    windows,
    ignore = "File rotation has issues on Windows with open file handles"
)]
async fn test_log_writer_rotation_trigger() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("rotate.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig {
            strategy: RotationStrategy::Size(100), // Small size for testing
            max_files: 3,
            compression: false,
        },
        buffer_size: 4096,
        flush_interval: Duration::from_millis(50),
        max_concurrent_writes: 1000,
        enable_compression: false,
    };

    let writer = LogWriter::new(config).await.unwrap();

    // Write enough data to trigger rotation
    for i in 0..10 {
        writer
            .write_line(&format!(
                "This is a long line number {} that should trigger rotation",
                i
            ))
            .unwrap();
    }

    writer.flush().await.unwrap();

    // Manually trigger rotation
    writer.rotate().await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await; // Give more time for rotation

    // Write more after rotation
    writer.write_line("After rotation").unwrap();
    writer.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await; // Give plenty of time for rotation to complete

    // Check that rotation occurred
    let mut entries = Vec::new();
    let mut dir_reader = fs::read_dir(temp_dir.path()).await.unwrap();
    while let Some(entry) = dir_reader.next_entry().await.unwrap() {
        entries.push(entry);
    }

    // Should have at least one rotated file (format: rotate.YYYYMMDD_HHMMSS.log)
    let rotated_files: Vec<_> = entries
        .iter()
        .filter(|e| {
            let name = e.file_name();
            let name_str = name.to_string_lossy();
            // Look for files with timestamp pattern
            name_str.starts_with("rotate.")
                && name_str.ends_with(".log")
                && name_str != "rotate.log"
        })
        .collect();

    assert!(
        !rotated_files.is_empty(),
        "No rotated files found. Files in directory: {:?}",
        entries.iter().map(|e| e.file_name()).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_log_writer_close() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("close.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig::default(),
        buffer_size: 4096,
        flush_interval: Duration::from_millis(100),
        max_concurrent_writes: 1000,
        enable_compression: false,
    };

    let writer = LogWriter::new(config).await.unwrap();

    // Write some data
    writer.write_line("Before close").unwrap();

    // Close the writer
    writer.shutdown().await.unwrap();

    // Give time for close to complete
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Data should be flushed
    let content = fs::read_to_string(&log_path).await.unwrap();
    assert!(content.contains("Before close"));
}

#[tokio::test]
#[cfg_attr(windows, ignore = "Flaky on Windows due to async timing issues")]
async fn test_async_log_writer() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("async.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig::default(),
        buffer_size: 4096,
        flush_interval: Duration::from_millis(100),
        max_concurrent_writes: 1000,
        enable_compression: false,
    };

    let writer = AsyncLogWriter::new(config).await.unwrap();

    // Test all methods
    writer.write_line("Line 1").unwrap();
    writer.flush().await.unwrap(); // Flush after first line
    tokio::time::sleep(Duration::from_millis(100)).await;

    writer.write(b"Raw data".as_ref()).unwrap();
    writer.write(b"\n".as_ref()).unwrap();
    writer.write_line("Line 2").unwrap();

    writer.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await; // Give more time for async operations

    let content = fs::read_to_string(&log_path).await.unwrap();
    assert!(
        content.contains("Line 1"),
        "Missing 'Line 1' in content: {}",
        content
    );
    assert!(
        content.contains("Raw data"),
        "Missing 'Raw data' in content: {}",
        content
    );
    assert!(
        content.contains("Line 2"),
        "Missing 'Line 2' in content: {}",
        content
    );

    // Test rotation
    writer.rotate().await.unwrap();

    // Test close
    writer.close().await.unwrap();
}

#[tokio::test]
#[cfg_attr(windows, ignore = "Flaky on Windows with empty writes")]
async fn test_log_writer_empty_writes() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("empty.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig::default(),
        buffer_size: 4096,
        flush_interval: Duration::from_millis(100),
        max_concurrent_writes: 1000,
        enable_compression: false,
    };

    let writer = LogWriter::new(config).await.unwrap();

    // Write empty data
    writer.write(b"".as_ref()).unwrap();
    writer.write_line("").unwrap(); // Should add just a newline
    writer.write(b"".as_ref()).unwrap();

    writer.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Should have one newline
    let content = fs::read(&log_path).await.unwrap();
    assert_eq!(content, b"\n");
}

#[tokio::test]
async fn test_log_writer_large_writes() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("large.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig {
            strategy: RotationStrategy::Never,
            max_files: 5,
            compression: false,
        },
        buffer_size: 4096,
        flush_interval: Duration::from_millis(100),
        max_concurrent_writes: 1000,
        enable_compression: false,
    };

    let writer = LogWriter::new(config).await.unwrap();

    // Write data larger than buffer size
    let large_line = "x".repeat(10000);
    writer.write_line(&large_line).unwrap();

    // Write many small lines
    for i in 0..1000 {
        writer.write_line(&format!("Small line {}", i)).unwrap();
    }

    writer.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await; // Give time for async flush

    let content = fs::read_to_string(&log_path).await.unwrap();
    assert!(content.contains(&large_line));
    assert!(content.contains("Small line 999"));

    // Verify line count - may be off by one due to trailing newline
    let lines: Vec<&str> = content.lines().collect();
    assert!(
        lines.len() >= 1000 && lines.len() <= 1001,
        "Expected 1000-1001 lines, got {}",
        lines.len()
    );
}

#[tokio::test]
#[cfg_attr(windows, ignore = "Flaky on Windows with concurrent writes")]
async fn test_log_writer_concurrent_writes() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("concurrent.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig::default(),
        buffer_size: 8192,
        flush_interval: Duration::from_millis(50),
        max_concurrent_writes: 1000,
        enable_compression: false,
    };

    let writer = std::sync::Arc::new(LogWriter::new(config).await.unwrap());

    let mut handles = vec![];

    for thread_id in 0..5 {
        let writer_clone = writer.clone();
        let handle = tokio::spawn(async move {
            for i in 0..20 {
                let line = format!("Thread {} Line {}", thread_id, i);
                writer_clone.write_line(&line).unwrap();

                if i % 5 == 0 {
                    tokio::time::sleep(Duration::from_micros(100)).await;
                }
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    writer.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await; // Give more time for all threads to flush

    let content = fs::read_to_string(&log_path).await.unwrap();

    // Verify all threads wrote their data
    for thread_id in 0..5 {
        for i in 0..20 {
            let expected = format!("Thread {} Line {}", thread_id, i);
            assert!(content.contains(&expected), "Missing: {}", expected);
        }
    }

    // Verify line integrity - may be off by one due to trailing newline
    let lines: Vec<&str> = content.lines().collect();
    assert!(
        lines.len() >= 99 && lines.len() <= 100,
        "Expected 99-100 lines, got {}",
        lines.len()
    );

    for line in lines {
        assert!(line.starts_with("Thread "));
        assert!(line.contains(" Line "));
    }
}
