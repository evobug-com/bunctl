use bunctl_core::{AppConfig, ProcessSupervisor};
use bunctl_supervisor::create_supervisor;
use std::path::PathBuf;
use std::time::Duration;
use tokio;

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
        command: if cfg!(windows) { "cmd".to_string() } else { "echo".to_string() },
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
        command: if cfg!(windows) { "timeout".to_string() } else { "sleep".to_string() },
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
    
    // Use a simple command that exits quickly
    let config = AppConfig {
        name: "graceful-test".to_string(),
        command: if cfg!(windows) { "cmd".to_string() } else { "sh".to_string() },
        args: if cfg!(windows) {
            vec!["/C".to_string(), "echo".to_string(), "test".to_string()]
        } else {
            vec!["-c".to_string(), "echo test".to_string()]
        },
        cwd: PathBuf::from("."),
        ..Default::default()
    };
    
    let mut handle = supervisor.spawn(&config).await.unwrap();
    
    let status = supervisor.graceful_stop(&mut handle, Duration::from_secs(5)).await.unwrap();
    assert!(status.code().is_some());
}

#[tokio::test]
async fn test_supervisor_events_channel() {
    let supervisor = create_supervisor().await.unwrap();
    
    let mut events = supervisor.events();
    
    let config = AppConfig {
        name: "event-test".to_string(),
        command: if cfg!(windows) { "cmd".to_string() } else { "echo".to_string() },
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
                    assert!(true);
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
        command: if cfg!(windows) { "cmd".to_string() } else { "sh".to_string() },
        args: if cfg!(windows) {
            vec!["/C".to_string(), "echo".to_string(), "%TEST_VAR%".to_string()]
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
        command: if cfg!(windows) { "timeout".to_string() } else { "sleep".to_string() },
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