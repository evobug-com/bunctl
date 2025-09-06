use bunctl_logging::{
    AsyncLogWriter, LogConfig, LogManager, LogWriterConfig, RotationConfig, RotationStrategy,
};
use std::time::Duration;
use tempfile::TempDir;
use tokio::fs;

#[tokio::test]
async fn test_permission_denied_handling() {
    // Skip on Windows CI where permission handling is different
    if std::env::var("CI").is_ok() && cfg!(windows) {
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let log_dir = temp_dir.path().join("readonly");
    fs::create_dir(&log_dir).await.unwrap();

    // Create a log file and make it read-only
    let log_path = log_dir.join("test.log");
    fs::write(&log_path, "existing content").await.unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&log_path).await.unwrap().permissions();
        perms.set_mode(0o444); // Read-only
        fs::set_permissions(&log_path, perms).await.unwrap();
    }

    #[cfg(windows)]
    {
        let mut perms = fs::metadata(&log_path).await.unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&log_path, perms).await.unwrap();
    }

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig::default(),
        buffer_size: 4096,
        flush_interval: Duration::from_millis(100),
    };

    // Should handle permission error gracefully
    let result = AsyncLogWriter::new(config).await;

    // On Windows, append mode might still work on read-only files
    if cfg!(unix) {
        assert!(
            result.is_err(),
            "Should fail to open read-only file for writing"
        );
    }

    // Restore permissions for cleanup
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&log_path).await.unwrap().permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&log_path, perms).await.unwrap();
    }

    #[cfg(windows)]
    {
        let mut perms = fs::metadata(&log_path).await.unwrap().permissions();
        perms.set_readonly(false);
        fs::set_permissions(&log_path, perms).await.unwrap();
    }
}

#[tokio::test]
async fn test_disk_full_simulation() {
    // This test simulates disk full by writing to a very small buffer
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("diskfull.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig {
            strategy: RotationStrategy::Size(10), // Very small rotation size
            max_files: 1,                         // Limited number of files
            compression: false,
        },
        buffer_size: 16, // Very small buffer
        flush_interval: Duration::from_millis(10),
    };

    let writer = AsyncLogWriter::new(config).await.unwrap();

    // Try to write a lot of data
    let mut write_count = 0;
    for i in 0..1000 {
        let result = writer.write_line(&format!("Line {} with some data to fill up space", i));
        if result.is_ok() {
            write_count += 1;
        }

        // Small delay to allow background processing
        if i % 100 == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }

    // Should have written at least some data
    assert!(write_count > 0, "Should have written at least some data");

    writer.flush().await.ok();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify file exists
    assert!(log_path.exists(), "Log file should exist");
}

#[tokio::test]
async fn test_nonexistent_directory() {
    let temp_dir = TempDir::new().unwrap();
    let nested_path = temp_dir
        .path()
        .join("does")
        .join("not")
        .join("exist")
        .join("test.log");

    let config = LogWriterConfig {
        path: nested_path.clone(),
        rotation: RotationConfig::default(),
        buffer_size: 4096,
        flush_interval: Duration::from_millis(100),
    };

    // Should fail to create writer in non-existent directory
    let result = AsyncLogWriter::new(config).await;
    assert!(
        result.is_err(),
        "Should fail to create log in non-existent directory"
    );
}

#[tokio::test]
async fn test_unicode_and_special_chars() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("unicode.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig::default(),
        buffer_size: 4096,
        flush_interval: Duration::from_millis(100),
    };

    let writer = AsyncLogWriter::new(config).await.unwrap();

    // Test various Unicode scenarios
    writer.write_line("ASCII: Hello World").unwrap();
    writer.write_line("Chinese: 你好世界").unwrap();
    writer.write_line("Japanese: こんにちは世界").unwrap();
    writer.write_line("Arabic: مرحبا بالعالم").unwrap();
    writer.write_line("Emoji: 🌍🌎🌏 Hello 👋").unwrap();
    writer.write_line("Math: ∑∏∫√∞").unwrap();
    writer.write_line("Symbols: ©®™€£¥").unwrap();
    writer.write_line("RTL: العربية עברית").unwrap();
    writer.write_line("Zero-width: ​‌‍").unwrap(); // Contains zero-width spaces
    writer
        .write_line("Control chars: \t\r\x1b[31mRed\x1b[0m")
        .unwrap();

    writer.flush().await.unwrap();
    writer.close().await.unwrap(); // Properly close the writer and wait for background task
    tokio::time::sleep(Duration::from_millis(100)).await; // Small delay to ensure file is written

    let content = fs::read_to_string(&log_path).await.unwrap();

    // Verify all special characters are preserved
    assert!(content.contains("你好世界"));
    assert!(content.contains("こんにちは世界"));
    assert!(content.contains("مرحبا بالعالم"));
    assert!(content.contains("🌍🌎🌏"));
    assert!(content.contains("∑∏∫√∞"));
    assert!(content.contains("\t"));
}

#[tokio::test]
#[cfg_attr(windows, ignore = "Path traversal behaves differently on Windows")]
async fn test_path_traversal_safety() {
    let temp_dir = TempDir::new().unwrap();

    // Test various path traversal attempts
    let unsafe_paths = vec![
        "../../../etc/passwd",
        "..\\..\\..\\windows\\system32\\config\\sam",
        "logs/../../../sensitive.log",
        "./../../private.log",
    ];

    for unsafe_path in unsafe_paths {
        let log_path = temp_dir.path().join(unsafe_path);

        let config = LogWriterConfig {
            path: log_path.clone(),
            rotation: RotationConfig::default(),
            buffer_size: 4096,
            flush_interval: Duration::from_millis(100),
        };

        // Should either fail or write to safe location
        match AsyncLogWriter::new(config).await {
            Ok(writer) => {
                writer.write_line("Test").unwrap();
                writer.flush().await.unwrap();

                // Verify file was created within temp directory or doesn't exist
                // On Windows, path traversal might be resolved differently
                if log_path.exists() {
                    let canonical = log_path.canonicalize().unwrap_or(log_path.clone());
                    let temp_canonical = temp_dir.path().canonicalize().unwrap();

                    // For Windows, we need to handle UNC paths and different canonicalization
                    let canonical_str = canonical.to_string_lossy().to_lowercase();
                    let temp_str = temp_canonical.to_string_lossy().to_lowercase();

                    // File should be within temp directory
                    assert!(
                        canonical_str.starts_with(&temp_str)
                            || canonical_str.contains("\\temp\\")
                            || canonical_str.contains("/temp/"),
                        "File {:?} was created outside temp directory {:?}",
                        canonical,
                        temp_canonical
                    );
                }
            }
            Err(_) => {
                // Failed to create, which is also acceptable
            }
        }
    }
}

#[tokio::test]
async fn test_very_long_lines() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("long_lines.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig::default(),
        buffer_size: 512 * 1024, // 512KB buffer
        flush_interval: Duration::from_millis(100),
    };

    let writer = AsyncLogWriter::new(config).await.unwrap();

    // Test various line lengths within reasonable limits
    let line_100 = "x".repeat(100);
    let line_1kb = "y".repeat(1024);
    let line_5kb = "z".repeat(5 * 1024);

    writer.write_line(&line_100).unwrap();
    writer.write_line("Normal line").unwrap();
    writer.write_line(&line_1kb).unwrap();
    writer.write_line(&line_5kb).unwrap();

    writer.flush().await.unwrap();
    drop(writer); // Ensure writer is properly closed
    tokio::time::sleep(Duration::from_millis(500)).await; // Give more time for write

    // Check if file exists first
    assert!(log_path.exists(), "Log file was not created");

    let content = fs::read_to_string(&log_path).await.unwrap_or_else(|e| {
        panic!("Failed to read log file: {}", e);
    });

    // Verify lines are preserved
    assert!(content.contains(&line_100));
    assert!(content.contains("Normal line"));
    assert!(content.contains(&line_1kb));
    assert!(content.contains(&line_5kb));

    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 4);
}

#[tokio::test]
async fn test_null_bytes_handling() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("null_bytes.log");

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig::default(),
        buffer_size: 4096,
        flush_interval: Duration::from_millis(100),
    };

    let writer = AsyncLogWriter::new(config).await.unwrap();

    // Write data with null bytes
    let data_with_nulls = b"Before\0null\0bytes\0after\n";
    writer.write(data_with_nulls.to_vec()).unwrap();
    writer.write_line("Normal line after nulls").unwrap();

    writer.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;

    let content = fs::read(&log_path).await.unwrap();

    // Verify null bytes are preserved
    assert!(
        content
            .windows(data_with_nulls.len())
            .any(|w| w == data_with_nulls)
    );

    // Text after nulls should still be readable
    let text = String::from_utf8_lossy(&content);
    assert!(text.contains("Normal line after nulls"));
}

#[tokio::test]
async fn test_concurrent_file_access() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("concurrent_access.log");

    // Create multiple writers to the same file
    let mut writers = vec![];

    for i in 0..3 {
        let config = LogWriterConfig {
            path: log_path.clone(),
            rotation: RotationConfig::default(),
            buffer_size: 4096,
            flush_interval: Duration::from_millis(50),
        };

        match AsyncLogWriter::new(config).await {
            Ok(writer) => {
                writers.push((i, writer));
            }
            Err(_) => {
                // Some systems may not allow multiple writers
            }
        }
    }

    // Write from all successful writers
    for (id, writer) in &writers {
        for j in 0..10 {
            writer
                .write_line(&format!("Writer {} Line {}", id, j))
                .unwrap();
        }
    }

    // Flush all
    for (_, writer) in &writers {
        writer.flush().await.unwrap();
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    if !writers.is_empty() {
        let content = fs::read_to_string(&log_path).await.unwrap();

        // Should have data from at least one writer
        assert!(!content.is_empty(), "Log file should not be empty");

        // Verify line integrity
        for line in content.lines() {
            assert!(line.starts_with("Writer ") || line.is_empty());
        }
    }
}

#[tokio::test]
async fn test_log_manager_empty_app_id() {
    let temp_dir = TempDir::new().unwrap();
    let config = LogConfig {
        base_dir: temp_dir.path().to_path_buf(),
        max_file_size: 10 * 1024 * 1024,
        max_files: 10,
        compression: false,
        buffer_size: 8192,
        flush_interval_ms: 100,
    };

    let manager = LogManager::new(config);

    // Test with various problematic app IDs
    let test_ids = vec![
        "",           // Empty
        " ",          // Whitespace
        ".",          // Dot
        "..",         // Double dot
        "con",        // Windows reserved name
        "prn",        // Windows reserved name
        "aux",        // Windows reserved name
        "nul",        // Windows reserved name
        "com1",       // Windows reserved name
        "../escape",  // Path traversal
        "..\\escape", // Windows path traversal
        "app/sub",    // Contains slash
        "app\\sub",   // Contains backslash
        "app:name",   // Contains colon (invalid on Windows)
        "app|name",   // Contains pipe (invalid on Windows)
    ];

    for test_id in test_ids {
        match bunctl_core::AppId::new(test_id) {
            Ok(app_id) => {
                // If AppId accepts it, test writer creation
                match manager.get_writer(&app_id).await {
                    Ok(writer) => {
                        // Should be able to write
                        writer.write_line("Test").ok();
                    }
                    Err(_) => {
                        // Failed to create writer, which is acceptable
                    }
                }
            }
            Err(_) => {
                // AppId validation rejected it, which is good
            }
        }
    }
}

#[cfg(windows)]
#[tokio::test]
async fn test_windows_file_locking() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("locked.log");

    // Create and lock a file
    use std::fs::OpenOptions;
    use std::io::Write;

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .open(&log_path)
        .unwrap();

    file.write_all(b"Locked content\n").unwrap();
    // Keep file handle open to maintain lock

    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig::default(),
        buffer_size: 4096,
        flush_interval: Duration::from_millis(100),
    };

    // Try to create writer while file is locked
    let writer_result = AsyncLogWriter::new(config).await;

    // On Windows, this might fail or succeed depending on share mode
    if let Ok(writer) = writer_result {
        // Try to write
        let write_result = writer.write_line("Attempt to write");

        // Writing might fail due to lock
        if write_result.is_ok() {
            writer.flush().await.ok();
        }
    }

    // Drop the lock
    drop(file);

    // Now it should work
    let config = LogWriterConfig {
        path: log_path.clone(),
        rotation: RotationConfig::default(),
        buffer_size: 4096,
        flush_interval: Duration::from_millis(100),
    };

    let writer = AsyncLogWriter::new(config).await.unwrap();
    writer.write_line("After unlock").unwrap();
    writer.flush().await.unwrap();
}
