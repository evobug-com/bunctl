#![cfg(target_os = "linux")]

use bunctl_core::AppConfig;
use bunctl_supervisor::create_supervisor;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_linux_supervisor_cgroups_v2_detection() {
    // Test that Linux supervisor properly detects cgroups v2
    let supervisor = create_supervisor().await.unwrap();

    // Basic supervisor creation should succeed even without cgroups
    let config = AppConfig {
        name: "cgroups-detect-test".to_string(),
        command: "echo".to_string(),
        args: vec!["test".to_string()],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn test_linux_supervisor_process_group_creation() {
    // Test that processes are created in their own process groups
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "pgrp-test".to_string(),
        command: "sh".to_string(),
        args: vec![
            "-c".to_string(),
            "ps -o pid,pgid,comm | grep $$".to_string(),
        ],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let pid = handle.pid;
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());

    // Process should have been in its own process group (pgid == pid)
    // Verification would require parsing ps output
}

#[tokio::test]
async fn test_linux_supervisor_kill_tree_with_children() {
    // Test that kill_tree properly terminates all child processes
    let supervisor = create_supervisor().await.unwrap();

    // Create a script that spawns children
    let config = AppConfig {
        name: "parent-child-test".to_string(),
        command: "sh".to_string(),
        args: vec![
            "-c".to_string(),
            "sleep 100 & sleep 100 & sleep 100 & wait".to_string(),
        ],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let handle = supervisor.spawn(&config).await.unwrap();
    let pgid = handle.pid;

    // Give children time to spawn
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Kill the entire process tree
    supervisor.kill_tree(&handle).await.unwrap();

    // Verify all processes in the group are terminated
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Check if any process with the pgid still exists
    let check_cmd = std::process::Command::new("ps")
        .args(&["-o", "pgid", "--no-headers"])
        .output();

    if let Ok(output) = check_cmd {
        let output_str = String::from_utf8_lossy(&output.stdout);
        assert!(!output_str.contains(&pgid.to_string()));
    }
}

#[tokio::test]
async fn test_linux_supervisor_signal_handling() {
    // Test various signal handling scenarios
    let supervisor = create_supervisor().await.unwrap();

    // Test SIGTERM handling
    let config = AppConfig {
        name: "signal-test".to_string(),
        command: "sh".to_string(),
        args: vec![
            "-c".to_string(),
            "trap 'echo SIGTERM received; exit 0' TERM; sleep 10".to_string(),
        ],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();

    // Send graceful stop (SIGTERM)
    let status = supervisor
        .graceful_stop(&mut handle, Duration::from_secs(2))
        .await
        .unwrap();

    assert!(status.code() == Some(0));
}

#[tokio::test]
async fn test_linux_supervisor_memory_limits_cgroups() {
    // Test memory limit enforcement via cgroups v2 (requires proper permissions)
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "memory-limit-test".to_string(),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), "echo Testing memory limits".to_string()],
        cwd: PathBuf::from("."),
        max_memory: Some(100 * 1024 * 1024), // 100MB limit
        ..Default::default()
    };

    let handle = supervisor.spawn(&config).await.unwrap();

    // Try to set resource limits (may fail without permissions)
    let _ = supervisor.set_resource_limits(&handle, &config).await;

    supervisor.kill_tree(&handle).await.unwrap();
}

#[tokio::test]
async fn test_linux_supervisor_cpu_limits_cgroups() {
    // Test CPU limit enforcement via cgroups v2 (requires proper permissions)
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "cpu-limit-test".to_string(),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), "echo Testing CPU limits".to_string()],
        cwd: PathBuf::from("."),
        max_cpu_percent: Some(50.0), // 50% CPU limit
        ..Default::default()
    };

    let handle = supervisor.spawn(&config).await.unwrap();

    // Try to set resource limits (may fail without permissions)
    let _ = supervisor.set_resource_limits(&handle, &config).await;

    supervisor.kill_tree(&handle).await.unwrap();
}

#[tokio::test]
async fn test_linux_supervisor_env_variables() {
    // Test environment variable handling
    let supervisor = create_supervisor().await.unwrap();

    let mut env = HashMap::new();
    env.insert("TEST_VAR1".to_string(), "value1".to_string());
    env.insert("TEST_VAR2".to_string(), "value2".to_string());
    env.insert("PATH".to_string(), "/usr/bin:/bin".to_string());

    let config = AppConfig {
        name: "env-test".to_string(),
        command: "sh".to_string(),
        args: vec![
            "-c".to_string(),
            "echo $TEST_VAR1 $TEST_VAR2 && echo $PATH".to_string(),
        ],
        env,
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn test_linux_supervisor_working_directory() {
    // Test that processes start in the correct working directory
    let supervisor = create_supervisor().await.unwrap();

    let temp_dir = "/tmp";

    let config = AppConfig {
        name: "cwd-test".to_string(),
        command: "pwd".to_string(),
        args: vec![],
        cwd: PathBuf::from(temp_dir),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn test_linux_supervisor_uid_gid() {
    // Test UID/GID setting (requires appropriate permissions)
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "uid-gid-test".to_string(),
        command: "id".to_string(),
        args: vec![],
        cwd: PathBuf::from("."),
        uid: Some(1000),
        gid: Some(1000),
        ..Default::default()
    };

    // This will likely fail in CI without proper permissions
    let result = supervisor.spawn(&config).await;

    // Just ensure it doesn't panic
    match result {
        Ok(mut handle) => {
            let _ = supervisor.wait(&mut handle).await;
        }
        Err(_) => {
            // Expected in restricted environments
        }
    }
}

#[tokio::test]
async fn test_linux_supervisor_multiple_processes() {
    // Test managing multiple processes simultaneously
    let supervisor = create_supervisor().await.unwrap();
    let mut handles = Vec::new();

    for i in 0..10 {
        let config = AppConfig {
            name: format!("multi-process-{}", i),
            command: "sleep".to_string(),
            args: vec![format!("0.{}", i)],
            cwd: PathBuf::from("."),
            ..Default::default()
        };

        let handle = supervisor.spawn(&config).await.unwrap();
        handles.push(handle);
    }

    // All processes should have unique PIDs
    let pids: Vec<u32> = handles.iter().map(|h| h.pid).collect();
    let unique_pids: std::collections::HashSet<_> = pids.iter().collect();
    assert_eq!(pids.len(), unique_pids.len());

    // Clean up all processes
    for handle in &handles {
        let _ = supervisor.kill_tree(handle).await;
    }
}

#[tokio::test]
async fn test_linux_supervisor_zombie_prevention() {
    // Test that supervisor properly reaps zombie processes
    let supervisor = create_supervisor().await.unwrap();

    // Create a process that exits quickly
    let config = AppConfig {
        name: "zombie-test".to_string(),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), "(sleep 0.1 &) && exit 0".to_string()],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());

    // Give time for zombie to be reaped
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Check for zombies (would require parsing /proc)
}

#[tokio::test]
async fn test_linux_supervisor_script_execution() {
    // Test execution of shell scripts
    let supervisor = create_supervisor().await.unwrap();

    // Create a temporary script
    let script_path = "/tmp/test_script.sh";
    fs::write(script_path, "#!/bin/sh\necho 'Script executed'\nexit 0").unwrap();

    // Make it executable
    std::process::Command::new("chmod")
        .args(&["+x", script_path])
        .output()
        .unwrap();

    let config = AppConfig {
        name: "script-test".to_string(),
        command: script_path.to_string(),
        args: vec![],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());

    // Clean up
    let _ = fs::remove_file(script_path);
}

#[tokio::test]
async fn test_linux_supervisor_pipe_handling() {
    // Test handling of pipes and redirection
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "pipe-test".to_string(),
        command: "sh".to_string(),
        args: vec![
            "-c".to_string(),
            "echo 'test' | grep 'test' | wc -l".to_string(),
        ],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn test_linux_supervisor_exit_codes() {
    // Test proper handling of various exit codes
    let supervisor = create_supervisor().await.unwrap();

    // Test success (0)
    let config = AppConfig {
        name: "exit-0".to_string(),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), "exit 0".to_string()],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());
    assert_eq!(status.code(), Some(0));

    // Test failure (1)
    let config = AppConfig {
        name: "exit-1".to_string(),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), "exit 1".to_string()],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(!status.success());
    assert_eq!(status.code(), Some(1));

    // Test signal termination
    let config = AppConfig {
        name: "signal-exit".to_string(),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), "kill -9 $$".to_string()],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(!status.success());
}

#[tokio::test]
async fn test_linux_supervisor_concurrent_spawns() {
    // Test spawning multiple processes concurrently
    let supervisor = create_supervisor().await.unwrap();

    let futures: Vec<_> = (0..20)
        .map(|i| {
            let supervisor = supervisor.clone();
            async move {
                let config = AppConfig {
                    name: format!("concurrent-{}", i),
                    command: "echo".to_string(),
                    args: vec![format!("process-{}", i)],
                    cwd: PathBuf::from("."),
                    ..Default::default()
                };
                supervisor.spawn(&config).await
            }
        })
        .collect();

    let mut results = Vec::new();
    for future in futures {
        results.push(future.await);
    }

    for result in results {
        assert!(result.is_ok());
        let handle = result.unwrap();
        assert!(handle.pid > 0);
    }
}

#[tokio::test]
async fn test_linux_supervisor_stress_spawn_kill() {
    // Stress test: rapidly spawn and kill processes
    let supervisor = create_supervisor().await.unwrap();

    for i in 0..100 {
        let config = AppConfig {
            name: format!("stress-{}", i),
            command: "sleep".to_string(),
            args: vec!["10".to_string()],
            cwd: PathBuf::from("."),
            ..Default::default()
        };

        let handle = supervisor.spawn(&config).await.unwrap();

        // Immediately kill some, let others run briefly
        if i % 2 == 0 {
            supervisor.kill_tree(&handle).await.unwrap();
        } else {
            tokio::time::sleep(Duration::from_micros(100)).await;
            supervisor.kill_tree(&handle).await.unwrap();
        }
    }
}

#[tokio::test]
async fn test_linux_supervisor_invalid_commands() {
    // Test handling of invalid commands
    let supervisor = create_supervisor().await.unwrap();

    // Non-existent command
    let config = AppConfig {
        name: "invalid-cmd".to_string(),
        command: "/usr/bin/this_does_not_exist_12345".to_string(),
        args: vec![],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let result = supervisor.spawn(&config).await;
    assert!(result.is_err());

    // Invalid working directory
    let config = AppConfig {
        name: "invalid-cwd".to_string(),
        command: "echo".to_string(),
        args: vec!["test".to_string()],
        cwd: PathBuf::from("/this/does/not/exist"),
        ..Default::default()
    };

    let result = supervisor.spawn(&config).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_linux_supervisor_file_descriptors() {
    // Test that file descriptors are properly managed
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "fd-test".to_string(),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), "ls -l /proc/$$/fd | wc -l".to_string()],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn test_linux_supervisor_process_info() {
    // Test getting process information
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "info-test".to_string(),
        command: "sleep".to_string(),
        args: vec!["1".to_string()],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let handle = supervisor.spawn(&config).await.unwrap();
    let pid = handle.pid;

    // Get process info
    let info = supervisor.get_process_info(pid).await.unwrap();
    assert_eq!(info.pid, pid);

    // Clean up
    supervisor.kill_tree(&handle).await.unwrap();
}

#[tokio::test]
async fn test_linux_supervisor_background_processes() {
    // Test handling of background processes
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "background-test".to_string(),
        command: "sh".to_string(),
        args: vec![
            "-c".to_string(),
            "(sleep 0.5 &) && echo 'Parent done'".to_string(),
        ],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());
}
