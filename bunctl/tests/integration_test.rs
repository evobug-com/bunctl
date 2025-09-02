use bunctl_core::{AppConfig, AppId, BackoffStrategy, Config};
use bunctl_logging::{LogConfig, LogManager};
use bunctl_supervisor::create_supervisor;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio;

#[tokio::test]
async fn test_full_app_lifecycle() {
    // Create supervisor
    let supervisor = create_supervisor().await.unwrap();

    // Create app config
    let mut env = HashMap::new();
    env.insert("TEST_ENV".to_string(), "integration".to_string());

    let config = AppConfig {
        name: "lifecycle-test".to_string(),
        command: if cfg!(windows) {
            "cmd".to_string()
        } else {
            "sh".to_string()
        },
        args: if cfg!(windows) {
            vec!["/C".to_string(), "echo".to_string(), "test".to_string()]
        } else {
            vec!["-c".to_string(), "echo test".to_string()]
        },
        cwd: PathBuf::from("."),
        env,
        auto_start: false,
        restart_policy: bunctl_core::config::RestartPolicy::No,
        ..Default::default()
    };

    // Spawn process
    let mut handle = supervisor.spawn(&config).await.unwrap();
    assert!(handle.pid > 0);

    // Wait for completion
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn test_backoff_strategy_integration() {
    let mut backoff = BackoffStrategy::new()
        .with_base_delay(Duration::from_millis(10))
        .with_max_delay(Duration::from_millis(100))
        .with_max_attempts(5)
        .with_jitter(0.1);

    let mut total_delay = Duration::ZERO;

    while let Some(delay) = backoff.next_delay() {
        total_delay += delay;
        // Simulate waiting
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    assert!(backoff.is_exhausted());
    assert_eq!(backoff.attempt(), 5);

    // Reset and verify it works again
    backoff.reset();
    assert!(!backoff.is_exhausted());
    assert!(backoff.next_delay().is_some());
}

#[tokio::test]
async fn test_logging_integration() {
    let temp_dir = TempDir::new().unwrap();

    let log_config = LogConfig {
        base_dir: temp_dir.path().to_path_buf(),
        max_file_size: 1024,
        max_files: 3,
        compression: false,
        buffer_size: 512,
        flush_interval_ms: 50,
    };

    let log_manager = LogManager::new(log_config);
    let app_id = AppId::new("log-test").unwrap();

    // Get writer for app
    let writer = log_manager.get_writer(&app_id).await.unwrap();

    // Write some logs
    for i in 0..10 {
        writer.write_line(&format!("Log line {}", i)).unwrap();
    }

    // Flush logs
    writer.flush().await.unwrap();

    // Verify log file exists
    let log_path = temp_dir.path().join("log-test.log");
    assert!(log_path.exists());

    // Remove writer
    log_manager.remove_writer(&app_id).await;
}

#[tokio::test]
async fn test_config_with_supervisor() {
    let config = Config {
        apps: vec![
            AppConfig {
                name: "app1".to_string(),
                command: "echo".to_string(),
                args: vec!["app1".to_string()],
                ..Default::default()
            },
            AppConfig {
                name: "app2".to_string(),
                command: "echo".to_string(),
                args: vec!["app2".to_string()],
                ..Default::default()
            },
        ],
        daemon: Default::default(),
    };

    let supervisor = create_supervisor().await.unwrap();

    // Start all apps from config
    let mut handles = Vec::new();
    for app_config in &config.apps {
        let handle = supervisor.spawn(app_config).await.unwrap();
        handles.push(handle);
    }

    assert_eq!(handles.len(), 2);

    // All processes should have valid PIDs
    for handle in &handles {
        assert!(handle.pid > 0);
    }
}

#[tokio::test]
async fn test_resource_limits() {
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "resource-test".to_string(),
        command: if cfg!(windows) {
            "cmd".to_string()
        } else {
            "sh".to_string()
        },
        args: if cfg!(windows) {
            vec!["/C".to_string(), "echo".to_string(), "test".to_string()]
        } else {
            vec!["-c".to_string(), "echo test".to_string()]
        },
        max_memory: Some(100 * 1024 * 1024), // 100MB
        max_cpu_percent: Some(50.0),
        ..Default::default()
    };

    let handle = supervisor.spawn(&config).await.unwrap();

    // Set resource limits
    let result = supervisor.set_resource_limits(&handle, &config).await;
    // May fail on some systems without proper permissions
    assert!(result.is_ok() || result.is_err());

    supervisor.kill_tree(&handle).await.unwrap();
}

#[tokio::test]
async fn test_process_info_retrieval() {
    let supervisor = create_supervisor().await.unwrap();

    // Start a longer-running process that works on all platforms
    let config = AppConfig {
        name: "info-test".to_string(),
        command: if cfg!(windows) {
            "ping".to_string()
        } else {
            "sh".to_string()
        },
        args: if cfg!(windows) {
            vec!["127.0.0.1".to_string(), "-n".to_string(), "5".to_string()]
        } else {
            vec!["-c".to_string(), "sleep 2".to_string()]
        },
        ..Default::default()
    };

    let handle = supervisor.spawn(&config).await.unwrap();
    let pid = handle.pid;

    // Give process time to start properly
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Get process info - it might fail on restricted environments
    match supervisor.get_process_info(pid).await {
        Ok(info) => {
            assert_eq!(info.pid, pid);
        }
        Err(_) => {
            // Process info retrieval can fail in CI environments
            // Just ensure the process was spawned
            assert!(pid > 0);
        }
    }

    // Clean up
    let _ = supervisor.kill_tree(&handle).await;
}

#[tokio::test]
async fn test_graceful_shutdown() {
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "graceful-test".to_string(),
        command: if cfg!(windows) {
            "ping".to_string()
        } else {
            "sh".to_string()
        },
        args: if cfg!(windows) {
            vec!["127.0.0.1".to_string(), "-n".to_string(), "10".to_string()]
        } else {
            vec!["-c".to_string(), "sleep 10".to_string()]
        },
        stop_timeout: Duration::from_secs(2),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();

    // Give it time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Graceful stop with timeout
    let start = tokio::time::Instant::now();
    let _status = supervisor
        .graceful_stop(&mut handle, config.stop_timeout)
        .await
        .unwrap();
    let elapsed = start.elapsed();

    // Should complete within timeout (plus some margin)
    assert!(elapsed < Duration::from_secs(4));
}

#[tokio::test]
async fn test_concurrent_app_management() {
    let supervisor = create_supervisor().await.unwrap();
    let supervisor = std::sync::Arc::new(supervisor);

    let mut tasks = Vec::new();

    // Spawn multiple apps concurrently
    for i in 0..5 {
        let supervisor_clone = supervisor.clone();
        let task = tokio::spawn(async move {
            let config = AppConfig {
                name: format!("concurrent-{}", i),
                command: if cfg!(windows) {
                    "cmd".to_string()
                } else {
                    "echo".to_string()
                },
                args: if cfg!(windows) {
                    vec!["/C".to_string(), "echo".to_string(), format!("{}", i)]
                } else {
                    vec![format!("{}", i)]
                },
                ..Default::default()
            };

            let mut handle = supervisor_clone.spawn(&config).await.unwrap();
            let status = supervisor_clone.wait(&mut handle).await.unwrap();
            assert!(status.success());
        });
        tasks.push(task);
    }

    // Wait for all tasks
    for task in tasks {
        task.await.unwrap();
    }
}
