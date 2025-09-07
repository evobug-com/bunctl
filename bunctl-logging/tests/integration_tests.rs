use bunctl_core::AppId;
use bunctl_logging::{LogConfig, LogManager};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::fs;
use tokio::task;

#[tokio::test]
async fn test_full_logging_lifecycle() {
    let temp_dir = TempDir::new().unwrap();
    let config = LogConfig {
        base_dir: temp_dir.path().to_path_buf(),
        max_file_size: 1024 * 1024,
        max_files: 5,
        compression: false,
        buffer_size: 8192,
        flush_interval_ms: 50,
    };

    let manager = LogManager::new(config);
    let app_id = AppId::new("lifecycle-test").unwrap();

    // Phase 1: Initial write
    let writer = manager.get_writer(&app_id).await.unwrap();
    writer.write_line("Application started").unwrap();
    writer.write_line("Initializing components").unwrap();
    writer.flush().await.unwrap();

    // Wait for flush to complete
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Phase 2: Normal operation
    for i in 0..100 {
        writer
            .write_line(&format!("Processing request {}", i))
            .unwrap();
    }
    writer.flush().await.unwrap();

    // Wait for flush to complete
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Phase 3: Read logs while still writing
    let logs = manager.read_logs(&app_id, 110).await.unwrap();
    assert!(
        logs.len() >= 10,
        "Should have at least 10 log lines, got {}",
        logs.len()
    );
    assert!(
        logs.iter().any(|l| l.contains("Application started")),
        "Logs: {:?}",
        logs
    );

    // Phase 4: Error logging
    for i in 0..10 {
        writer
            .write_line(&format!(
                "[lifecycle-test] [2024-01-01] [stderr] Error {}",
                i
            ))
            .unwrap();
    }
    writer.flush().await.unwrap();

    // Phase 5: Read structured logs
    let structured = manager.read_structured_logs(&app_id, 50).await.unwrap();
    assert!(!structured.errors.is_empty() || !structured.output.is_empty());

    // Phase 6: Rotation
    manager.rotate_all().await.unwrap();

    // Phase 7: Continue writing after rotation
    writer.write_line("After rotation").unwrap();
    writer.flush().await.unwrap();

    // Phase 8: Cleanup
    writer.flush().await.unwrap(); // Flush before removal
    let _ = manager.remove_writer(&app_id).await;

    // Wait a bit for file operations to complete
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify final state
    let final_logs = manager.read_logs(&app_id, 1000).await.unwrap();
    assert!(
        !final_logs.is_empty(),
        "Logs should persist after writer removal"
    );
}

#[tokio::test]
async fn test_multiple_apps_isolation() {
    let temp_dir = TempDir::new().unwrap();
    let config = LogConfig {
        base_dir: temp_dir.path().to_path_buf(),
        max_file_size: 10 * 1024 * 1024,
        max_files: 10,
        compression: false,
        buffer_size: 8192,
        flush_interval_ms: 50,
    };

    let manager = Arc::new(LogManager::new(config));
    let app_ids: Vec<AppId> = (0..5)
        .map(|i| AppId::new(&format!("app-{}", i)).unwrap())
        .collect();

    // Write to each app concurrently
    let mut handles = vec![];

    for (idx, app_id) in app_ids.iter().enumerate() {
        let manager_clone = manager.clone();
        let app_id_clone = app_id.clone();

        let handle = task::spawn(async move {
            let writer = manager_clone.get_writer(&app_id_clone).await.unwrap();

            for i in 0..50 {
                writer
                    .write_line(&format!("App {} - Line {}", idx, i))
                    .unwrap();
            }

            writer.flush().await.unwrap();
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Verify isolation - each app should have only its own logs
    for (idx, app_id) in app_ids.iter().enumerate() {
        let logs = manager.read_logs(app_id, 100).await.unwrap();

        assert!(!logs.is_empty(), "App {} should have logs", idx);

        // All logs should be from this app only
        for log in &logs {
            if !log.contains("No log file") && !log.is_empty() {
                assert!(
                    log.contains(&format!("App {}", idx)),
                    "Log '{}' doesn't belong to App {}",
                    log,
                    idx
                );
            }
        }

        // Should not contain logs from other apps
        for other_idx in 0..5 {
            if other_idx != idx {
                for log in &logs {
                    assert!(
                        !log.contains(&format!("App {} ", other_idx)),
                        "App {} logs contain data from App {}",
                        idx,
                        other_idx
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn test_crash_recovery() {
    let temp_dir = TempDir::new().unwrap();
    let base_dir = temp_dir.path().to_path_buf();
    let app_id = AppId::new("crash-test").unwrap();

    // Simulate first run
    {
        let config = LogConfig {
            base_dir: base_dir.clone(),
            max_file_size: 10 * 1024 * 1024,
            max_files: 10,
            compression: false,
            buffer_size: 8192,
            flush_interval_ms: 50,
        };

        let manager = LogManager::new(config);
        let writer = manager.get_writer(&app_id).await.unwrap();

        writer.write_line("Before crash - line 1").unwrap();
        writer.write_line("Before crash - line 2").unwrap();
        writer.write_line("Before crash - line 3").unwrap();

        manager.flush_all().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Simulate crash - drop without proper cleanup
        drop(writer);
        drop(manager);
    }

    // Simulate recovery after crash
    {
        let config = LogConfig {
            base_dir: base_dir.clone(),
            max_file_size: 10 * 1024 * 1024,
            max_files: 10,
            compression: false,
            buffer_size: 8192,
            flush_interval_ms: 50,
        };

        let manager = LogManager::new(config);

        // Should be able to read old logs
        let old_logs = manager.read_logs(&app_id, 100).await.unwrap();
        assert!(old_logs.iter().any(|l| l.contains("Before crash")));

        // Should be able to continue writing
        let writer = manager.get_writer(&app_id).await.unwrap();
        writer.write_line("After recovery - line 1").unwrap();
        writer.write_line("After recovery - line 2").unwrap();
        writer.flush().await.unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify both old and new logs are present
        let all_logs = manager.read_logs(&app_id, 100).await.unwrap();
        assert!(all_logs.iter().any(|l| l.contains("Before crash")));
        assert!(all_logs.iter().any(|l| l.contains("After recovery")));
    }
}

#[tokio::test]
async fn test_read_all_apps() {
    let temp_dir = TempDir::new().unwrap();
    let config = LogConfig {
        base_dir: temp_dir.path().to_path_buf(),
        max_file_size: 10 * 1024 * 1024,
        max_files: 10,
        compression: false,
        buffer_size: 8192,
        flush_interval_ms: 50,
    };

    let manager = LogManager::new(config);

    // Create logs for multiple apps
    let app_names = vec!["web-server", "api-gateway", "database", "cache"];

    for app_name in &app_names {
        let app_id = AppId::new(*app_name).unwrap();
        let writer = manager.get_writer(&app_id).await.unwrap();

        // Write different types of logs
        writer
            .write_line(&format!(
                "[{}] [2024-01-01] [stdout] Starting {}",
                app_name, app_name
            ))
            .unwrap();
        writer
            .write_line(&format!(
                "[{}] [2024-01-01] [stdout] {} initialized",
                app_name, app_name
            ))
            .unwrap();
        writer
            .write_line(&format!(
                "[{}] [2024-01-01] [stderr] Warning: {} needs configuration",
                app_name, app_name
            ))
            .unwrap();

        writer.flush().await.unwrap();
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Read all apps' logs
    let all_logs = manager.read_all_apps_logs(10).await.unwrap();

    // Should have logs for all apps
    assert_eq!(all_logs.len(), app_names.len());

    // Verify apps are sorted alphabetically
    let returned_names: Vec<String> = all_logs.iter().map(|(name, _)| name.clone()).collect();
    let mut expected_names: Vec<String> = app_names.iter().map(|s| s.to_string()).collect();
    expected_names.sort();
    assert_eq!(returned_names, expected_names);

    // Verify structured logs for each app
    for (app_name, logs) in &all_logs {
        assert!(
            !logs.errors.is_empty() || !logs.output.is_empty(),
            "App {} should have some logs",
            app_name
        );
    }
}

#[tokio::test]
async fn test_log_file_permissions() {
    let temp_dir = TempDir::new().unwrap();
    let config = LogConfig {
        base_dir: temp_dir.path().to_path_buf(),
        max_file_size: 10 * 1024 * 1024,
        max_files: 10,
        compression: false,
        buffer_size: 8192,
        flush_interval_ms: 50,
    };

    let manager = LogManager::new(config);
    let app_id = AppId::new("permissions-test").unwrap();

    let writer = manager.get_writer(&app_id).await.unwrap();
    writer.write_line("Test line").unwrap();
    writer.flush().await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    let log_path = temp_dir.path().join("permissions-test.log");
    assert!(log_path.exists());

    // Check file metadata
    let metadata = fs::metadata(&log_path).await.unwrap();
    assert!(metadata.is_file());
    assert!(metadata.len() > 0);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = metadata.permissions();
        let mode = perms.mode();

        // File should be readable and writable by owner
        assert!(
            mode & 0o600 == 0o600,
            "File should be readable and writable by owner"
        );
    }
}

#[tokio::test]
async fn test_hot_reload_simulation() {
    let temp_dir = TempDir::new().unwrap();
    let app_id = AppId::new("hot-reload-test").unwrap();

    // Start with one configuration
    {
        let config = LogConfig {
            base_dir: temp_dir.path().to_path_buf(),
            max_file_size: 1024 * 1024,
            max_files: 3,
            compression: false,
            buffer_size: 4096,
            flush_interval_ms: 100,
        };

        let manager = LogManager::new(config);
        let writer = manager.get_writer(&app_id).await.unwrap();

        for i in 0..50 {
            writer
                .write_line(&format!("Config 1 - Line {}", i))
                .unwrap();
        }

        manager.flush_all().await.unwrap();
    }

    // Simulate configuration change
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Create new manager with different configuration
    {
        let config = LogConfig {
            base_dir: temp_dir.path().to_path_buf(),
            max_file_size: 2 * 1024 * 1024, // Different size
            max_files: 5,                   // Different max files
            compression: true,              // Enable compression
            buffer_size: 8192,              // Different buffer
            flush_interval_ms: 50,          // Different flush interval
        };

        let manager = LogManager::new(config);

        // Should be able to read old logs
        let old_logs = manager.read_logs(&app_id, 100).await.unwrap();
        assert!(old_logs.iter().any(|l| l.contains("Config 1")));

        // Continue writing with new config
        let writer = manager.get_writer(&app_id).await.unwrap();

        for i in 0..50 {
            writer
                .write_line(&format!("Config 2 - Line {}", i))
                .unwrap();
        }

        manager.flush_all().await.unwrap();

        // Verify both configurations' logs exist
        let all_logs = manager.read_logs(&app_id, 200).await.unwrap();
        assert!(all_logs.iter().any(|l| l.contains("Config 1")));
        assert!(all_logs.iter().any(|l| l.contains("Config 2")));
    }
}

#[tokio::test]
#[ignore = "Extreme concurrency test can timeout in CI environments"]
async fn test_extreme_concurrency() {
    let temp_dir = TempDir::new().unwrap();
    let config = LogConfig {
        base_dir: temp_dir.path().to_path_buf(),
        max_file_size: 10 * 1024 * 1024,
        max_files: 10,
        compression: false,
        buffer_size: 16384,
        flush_interval_ms: 10,
    };

    let manager = Arc::new(LogManager::new(config));

    // Create many concurrent operations
    let mut handles = vec![];

    // Multiple apps writing concurrently
    for app_idx in 0..10 {
        let manager_clone = manager.clone();
        let handle = task::spawn(async move {
            let app_id = AppId::new(&format!("concurrent-app-{}", app_idx)).unwrap();
            let writer = manager_clone.get_writer(&app_id).await.unwrap();

            for i in 0..100 {
                writer
                    .write_line(&format!("App {} Line {}", app_idx, i))
                    .unwrap();

                if i % 20 == 0 {
                    writer.flush().await.unwrap();
                }
            }
        });
        handles.push(handle);
    }

    // Concurrent readers
    for reader_idx in 0..5 {
        let manager_clone = manager.clone();
        let handle = task::spawn(async move {
            for _ in 0..20 {
                let app_id = AppId::new(&format!("concurrent-app-{}", reader_idx)).unwrap();
                let _ = manager_clone.read_logs(&app_id, 50).await;
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        handles.push(handle);
    }

    // Concurrent rotations
    let manager_clone = manager.clone();
    let rotation_handle = task::spawn(async move {
        for _ in 0..5 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = manager_clone.rotate_all().await;
        }
    });
    handles.push(rotation_handle);

    // Wait for all operations
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify system stability
    let all_logs = manager.read_all_apps_logs(10).await.unwrap();
    assert!(
        !all_logs.is_empty(),
        "Should have logs after extreme concurrency"
    );
}
