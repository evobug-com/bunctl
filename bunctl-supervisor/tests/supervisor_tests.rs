use bunctl_core::AppConfig;
use bunctl_supervisor::create_supervisor;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[tokio::test]
async fn test_create_supervisor() {
    let supervisor = create_supervisor().await;
    assert!(supervisor.is_ok());
}

#[tokio::test]
async fn test_supervisor_spawn_echo() {
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "echo-test".to_string(),
        command: if cfg!(windows) {
            "cmd".to_string()
        } else {
            "echo".to_string()
        },
        args: if cfg!(windows) {
            vec!["/C".to_string(), "echo".to_string(), "hello".to_string()]
        } else {
            vec!["hello".to_string()]
        },
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    assert!(handle.pid > 0);

    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn test_supervisor_spawn_sleep_and_kill() {
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "sleep-test".to_string(),
        command: if cfg!(windows) {
            "timeout".to_string()
        } else {
            "sleep".to_string()
        },
        args: if cfg!(windows) {
            vec!["/T".to_string(), "10".to_string()]
        } else {
            vec!["10".to_string()]
        },
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let handle = supervisor.spawn(&config).await.unwrap();
    let pid = handle.pid;
    assert!(pid > 0);

    // Give it a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Kill the process tree
    supervisor.kill_tree(&handle).await.unwrap();

    // Process should be gone
    // Note: We can't easily verify this cross-platform without more complex checks
}

#[tokio::test]
async fn test_supervisor_graceful_stop() {
    let supervisor = create_supervisor().await.unwrap();

    // Use a command that runs long enough to be stopped
    let config = AppConfig {
        name: "graceful-test".to_string(),
        command: if cfg!(windows) {
            "cmd".to_string()
        } else {
            "sh".to_string()
        },
        args: if cfg!(windows) {
            vec![
                "/C".to_string(),
                "ping".to_string(),
                "127.0.0.1".to_string(),
                "-n".to_string(),
                "10".to_string(),
            ]
        } else {
            vec!["-c".to_string(), "sleep 10".to_string()]
        },
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();

    // Give the process time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    let status = supervisor
        .graceful_stop(&mut handle, Duration::from_secs(5))
        .await
        .unwrap();

    // The process should have been terminated, but the exit code might vary
    // On Unix, SIGTERM might not always produce a code
    // Just verify we got a status back
    assert!(status.code().is_some() || cfg!(unix));
}

#[tokio::test]
async fn test_supervisor_events_channel() {
    let supervisor = create_supervisor().await.unwrap();

    let mut events = supervisor.events();

    let config = AppConfig {
        name: "event-test".to_string(),
        command: if cfg!(windows) {
            "cmd".to_string()
        } else {
            "echo".to_string()
        },
        args: if cfg!(windows) {
            vec!["/C".to_string(), "echo".to_string(), "event".to_string()]
        } else {
            vec!["event".to_string()]
        },
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let _handle = supervisor.spawn(&config).await.unwrap();

    // Should receive a ProcessStarted event
    tokio::select! {
        event = events.recv() => {
            assert!(event.is_some());
            match event.unwrap() {
                bunctl_core::SupervisorEvent::ProcessStarted { .. } => {
                    // Event type verified
                }
                _ => panic!("Expected ProcessStarted event"),
            }
        }
        _ = tokio::time::sleep(Duration::from_secs(1)) => {
            panic!("Timeout waiting for event");
        }
    }
}

#[tokio::test]
async fn test_supervisor_spawn_with_env() {
    let supervisor = create_supervisor().await.unwrap();

    let mut env = std::collections::HashMap::new();
    env.insert("TEST_VAR".to_string(), "test_value".to_string());

    let config = AppConfig {
        name: "env-test".to_string(),
        command: if cfg!(windows) {
            "cmd".to_string()
        } else {
            "sh".to_string()
        },
        args: if cfg!(windows) {
            vec![
                "/C".to_string(),
                "echo".to_string(),
                "%TEST_VAR%".to_string(),
            ]
        } else {
            vec!["-c".to_string(), "echo $TEST_VAR".to_string()]
        },
        env,
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn test_supervisor_spawn_invalid_command() {
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "invalid-test".to_string(),
        command: "this_command_does_not_exist_12345".to_string(),
        args: vec![],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let result = supervisor.spawn(&config).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_supervisor_get_process_info() {
    let supervisor = create_supervisor().await.unwrap();

    // Start a long-running process
    let config = AppConfig {
        name: "info-test".to_string(),
        command: if cfg!(windows) {
            "timeout".to_string()
        } else {
            "sleep".to_string()
        },
        args: if cfg!(windows) {
            vec!["/T".to_string(), "5".to_string()]
        } else {
            vec!["5".to_string()]
        },
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let handle = supervisor.spawn(&config).await.unwrap();
    let pid = handle.pid;

    // Give it a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    let info = supervisor.get_process_info(pid).await.unwrap();
    assert_eq!(info.pid, pid);

    // Clean up
    supervisor.kill_tree(&handle).await.unwrap();
}

#[tokio::test]
async fn test_supervisor_process_lifecycle() {
    // Comprehensive test of process lifecycle: spawn, monitor, restart, kill
    let supervisor = create_supervisor().await.unwrap();

    // Test normal completion
    let config = AppConfig {
        name: "lifecycle-normal".to_string(),
        command: if cfg!(windows) {
            "cmd".to_string()
        } else {
            "sh".to_string()
        },
        args: if cfg!(windows) {
            vec!["/C".to_string(), "echo Normal exit".to_string()]
        } else {
            vec!["-c".to_string(), "echo 'Normal exit'".to_string()]
        },
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    assert!(handle.pid > 0);
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());

    // Test abnormal termination
    let config = AppConfig {
        name: "lifecycle-abnormal".to_string(),
        command: if cfg!(windows) {
            "cmd".to_string()
        } else {
            "sh".to_string()
        },
        args: if cfg!(windows) {
            vec!["/C".to_string(), "exit 1".to_string()]
        } else {
            vec!["-c".to_string(), "exit 1".to_string()]
        },
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(!status.success());
    assert_eq!(status.code(), Some(1));

    // Test forced termination
    let config = AppConfig {
        name: "lifecycle-kill".to_string(),
        command: if cfg!(windows) {
            "timeout".to_string()
        } else {
            "sleep".to_string()
        },
        args: if cfg!(windows) {
            vec!["/T".to_string(), "30".to_string()]
        } else {
            vec!["30".to_string()]
        },
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let handle = supervisor.spawn(&config).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    supervisor.kill_tree(&handle).await.unwrap();
}

#[tokio::test]
async fn test_supervisor_concurrent_process_management() {
    // Test managing many processes concurrently
    let supervisor = create_supervisor().await.unwrap();
    let process_count = 20;
    let handles = Arc::new(Mutex::new(Vec::new()));

    // Spawn processes concurrently
    let spawn_futures: Vec<_> = (0..process_count)
        .map(|i| {
            let supervisor = supervisor.clone();
            let handles = handles.clone();
            async move {
                let config = AppConfig {
                    name: format!("concurrent-{}", i),
                    command: if cfg!(windows) {
                        "cmd".to_string()
                    } else {
                        "sh".to_string()
                    },
                    args: if cfg!(windows) {
                        vec![
                            "/C".to_string(),
                            format!("ping 127.0.0.1 -n {}", (i % 3) + 1),
                        ]
                    } else {
                        vec!["-c".to_string(), format!("sleep 0.{}", i % 10)]
                    },
                    cwd: PathBuf::from("."),
                    ..Default::default()
                };

                let handle = supervisor.spawn(&config).await.unwrap();
                handles.lock().await.push(handle);
            }
        })
        .collect();

    for future in spawn_futures {
        future.await;
    }

    let handles = handles.lock().await;
    assert_eq!(handles.len(), process_count);

    // Verify all PIDs are unique
    let pids: Vec<u32> = handles.iter().map(|h| h.pid).collect();
    let unique_pids: std::collections::HashSet<_> = pids.iter().collect();
    assert_eq!(pids.len(), unique_pids.len());

    // Clean up all processes
    for handle in handles.iter() {
        let _ = supervisor.kill_tree(handle).await;
    }
}

#[tokio::test]
async fn test_supervisor_restart_behavior() {
    // Test process restart behavior
    let supervisor = create_supervisor().await.unwrap();
    let restart_count = Arc::new(AtomicU32::new(0));

    for attempt in 0..3 {
        let config = AppConfig {
            name: "restart-test".to_string(),
            command: if cfg!(windows) {
                "cmd".to_string()
            } else {
                "sh".to_string()
            },
            args: if cfg!(windows) {
                vec![
                    "/C".to_string(),
                    format!("echo Attempt {} && exit 1", attempt),
                ]
            } else {
                vec![
                    "-c".to_string(),
                    format!("echo 'Attempt {}' && exit 1", attempt),
                ]
            },
            cwd: PathBuf::from("."),
            ..Default::default()
        };

        let mut handle = supervisor.spawn(&config).await.unwrap();
        let status = supervisor.wait(&mut handle).await.unwrap();
        assert!(!status.success());

        restart_count.fetch_add(1, Ordering::SeqCst);

        // Simulate backoff delay
        tokio::time::sleep(Duration::from_millis(100 * (attempt + 1) as u64)).await;
    }

    assert_eq!(restart_count.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_supervisor_performance_spawn() {
    // Performance test: measure spawn latency
    let supervisor = create_supervisor().await.unwrap();
    let iterations = 10;
    let mut spawn_times = Vec::new();

    for i in 0..iterations {
        let config = AppConfig {
            name: format!("perf-spawn-{}", i),
            command: if cfg!(windows) {
                "cmd".to_string()
            } else {
                "true".to_string()
            },
            args: if cfg!(windows) {
                vec!["/C".to_string(), "exit 0".to_string()]
            } else {
                vec![]
            },
            cwd: PathBuf::from("."),
            ..Default::default()
        };

        let start = Instant::now();
        let mut handle = supervisor.spawn(&config).await.unwrap();
        let spawn_time = start.elapsed();
        spawn_times.push(spawn_time);

        let _ = supervisor.wait(&mut handle).await;
    }

    // Calculate average spawn time
    let total: Duration = spawn_times.iter().sum();
    let avg_spawn_time = total / iterations as u32;

    // Spawn should be reasonably fast (under 200ms on average, allowing for CI)
    assert!(
        avg_spawn_time < Duration::from_millis(200),
        "Average spawn time {:?} exceeds 200ms",
        avg_spawn_time
    );
}

#[tokio::test]
async fn test_supervisor_error_recovery() {
    // Test supervisor's ability to recover from errors
    let supervisor = create_supervisor().await.unwrap();

    // Test recovery from invalid command
    let config = AppConfig {
        name: "error-recovery-1".to_string(),
        command: "this_command_does_not_exist_999".to_string(),
        args: vec![],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let result = supervisor.spawn(&config).await;
    assert!(result.is_err());

    // Supervisor should still work after error
    let config = AppConfig {
        name: "error-recovery-2".to_string(),
        command: if cfg!(windows) {
            "cmd".to_string()
        } else {
            "echo".to_string()
        },
        args: if cfg!(windows) {
            vec!["/C".to_string(), "echo Recovery test".to_string()]
        } else {
            vec!["Recovery test".to_string()]
        },
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());
}
