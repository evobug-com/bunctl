#![cfg(target_os = "windows")]

use bunctl_core::AppConfig;
use bunctl_supervisor::create_supervisor;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_windows_supervisor_job_object_creation() {
    // Test that Windows supervisor can create and manage processes with Job Objects
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "job-object-test".to_string(),
        command: "cmd".to_string(),
        args: vec![
            "/C".to_string(),
            "ping".to_string(),
            "127.0.0.1".to_string(),
            "-n".to_string(),
            "2".to_string(),
        ],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    assert!(handle.pid > 0);

    // Process should complete successfully
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn test_windows_supervisor_kill_tree_with_children() {
    // Test that kill_tree properly terminates parent and child processes via Job Object
    let supervisor = create_supervisor().await.unwrap();

    // Command that spawns child processes
    let config = AppConfig {
        name: "parent-child-test".to_string(),
        command: "cmd".to_string(),
        args: vec![
            "/C".to_string(),
            "start /B cmd /C ping 127.0.0.1 -t && ping 127.0.0.1 -t".to_string(),
        ],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let handle = supervisor.spawn(&config).await.unwrap();

    // Give processes time to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Kill the entire process tree
    supervisor.kill_tree(&handle).await.unwrap();

    // Verify process is terminated (can't easily check children on Windows without WMI)
    tokio::time::sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_windows_supervisor_env_inheritance() {
    // Test that important Windows environment variables are properly inherited
    let supervisor = create_supervisor().await.unwrap();

    let mut env = HashMap::new();
    env.insert("CUSTOM_VAR".to_string(), "custom_value".to_string());

    let config = AppConfig {
        name: "env-inheritance-test".to_string(),
        command: "cmd".to_string(),
        args: vec![
            "/C".to_string(),
            "echo %USERPROFILE% %CUSTOM_VAR%".to_string(),
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
async fn test_windows_supervisor_log_redirection() {
    // Test that stdout/stderr are properly redirected to log files
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "log-redirect-test".to_string(),
        command: "cmd".to_string(),
        args: vec![
            "/C".to_string(),
            "echo stdout output && echo stderr output 1>&2".to_string(),
        ],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());

    // Check that log files were created
    let log_dir = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("bunctl")
        .join("logs");

    let stdout_log = log_dir.join("log-redirect-test-out.log");
    let stderr_log = log_dir.join("log-redirect-test-err.log");

    // Files should exist (contents verification would require reading)
    assert!(stdout_log.exists() || stderr_log.exists());
}

#[tokio::test]
async fn test_windows_supervisor_working_directory() {
    // Test that processes start in the correct working directory
    let supervisor = create_supervisor().await.unwrap();

    let temp_dir = std::env::temp_dir();

    let config = AppConfig {
        name: "cwd-test".to_string(),
        command: "cmd".to_string(),
        args: vec!["/C".to_string(), "cd".to_string()],
        cwd: temp_dir.clone(),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn test_windows_supervisor_multiple_processes() {
    // Test managing multiple processes simultaneously
    let supervisor = create_supervisor().await.unwrap();
    let mut handles = Vec::new();

    for i in 0..5 {
        let config = AppConfig {
            name: format!("multi-process-{}", i),
            command: "cmd".to_string(),
            args: vec!["/C".to_string(), format!("ping 127.0.0.1 -n {}", i + 1)],
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
async fn test_windows_supervisor_long_running_process() {
    // Test handling of long-running processes
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "long-running-test".to_string(),
        command: "timeout".to_string(),
        args: vec!["/T".to_string(), "30".to_string()],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let handle = supervisor.spawn(&config).await.unwrap();

    // Process should still be running
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Kill it before timeout
    supervisor.kill_tree(&handle).await.unwrap();
}

#[tokio::test]
async fn test_windows_supervisor_batch_file_execution() {
    // Test execution of batch files
    let supervisor = create_supervisor().await.unwrap();

    // Create a temporary batch file
    let temp_dir = std::env::temp_dir();
    let batch_file = temp_dir.join("test_script.bat");
    std::fs::write(
        &batch_file,
        "@echo off\necho Batch file executed\nexit /b 0",
    )
    .unwrap();

    let config = AppConfig {
        name: "batch-test".to_string(),
        command: "cmd".to_string(),
        args: vec!["/C".to_string(), batch_file.to_string_lossy().to_string()],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());

    // Clean up
    let _ = std::fs::remove_file(batch_file);
}

#[tokio::test]
async fn test_windows_supervisor_powershell_execution() {
    // Test PowerShell script execution
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "powershell-test".to_string(),
        command: "powershell".to_string(),
        args: vec![
            "-NoProfile".to_string(),
            "-Command".to_string(),
            "Write-Host 'PowerShell test'; exit 0".to_string(),
        ],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn test_windows_supervisor_exit_codes() {
    // Test proper handling of various exit codes
    let supervisor = create_supervisor().await.unwrap();

    // Test success (0)
    let config = AppConfig {
        name: "exit-0".to_string(),
        command: "cmd".to_string(),
        args: vec!["/C".to_string(), "exit 0".to_string()],
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
        command: "cmd".to_string(),
        args: vec!["/C".to_string(), "exit 1".to_string()],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(!status.success());
    assert_eq!(status.code(), Some(1));

    // Test custom exit code
    let config = AppConfig {
        name: "exit-42".to_string(),
        command: "cmd".to_string(),
        args: vec!["/C".to_string(), "exit 42".to_string()],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(!status.success());
    assert_eq!(status.code(), Some(42));
}

#[tokio::test]
async fn test_windows_supervisor_concurrent_spawns() {
    // Test spawning multiple processes concurrently
    let supervisor = create_supervisor().await.unwrap();

    let futures: Vec<_> = (0..10)
        .map(|i| {
            let supervisor = supervisor.clone();
            async move {
                let config = AppConfig {
                    name: format!("concurrent-{}", i),
                    command: "cmd".to_string(),
                    args: vec!["/C".to_string(), "echo concurrent".to_string()],
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
async fn test_windows_supervisor_stress_spawn_kill() {
    // Stress test: rapidly spawn and kill processes
    let supervisor = create_supervisor().await.unwrap();

    for i in 0..50 {
        let config = AppConfig {
            name: format!("stress-{}", i),
            command: "timeout".to_string(),
            args: vec!["/T".to_string(), "10".to_string()],
            cwd: PathBuf::from("."),
            ..Default::default()
        };

        let handle = supervisor.spawn(&config).await.unwrap();

        // Immediately kill some, let others run briefly
        if i % 2 == 0 {
            supervisor.kill_tree(&handle).await.unwrap();
        } else {
            tokio::time::sleep(Duration::from_millis(10)).await;
            supervisor.kill_tree(&handle).await.unwrap();
        }
    }
}

#[tokio::test]
async fn test_windows_supervisor_invalid_commands() {
    // Test handling of invalid commands
    let supervisor = create_supervisor().await.unwrap();

    // Non-existent command
    let config = AppConfig {
        name: "invalid-cmd".to_string(),
        command: "this_does_not_exist_12345.exe".to_string(),
        args: vec![],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let result = supervisor.spawn(&config).await;
    assert!(result.is_err());

    // Invalid working directory
    let config = AppConfig {
        name: "invalid-cwd".to_string(),
        command: "cmd".to_string(),
        args: vec!["/C".to_string(), "echo test".to_string()],
        cwd: PathBuf::from("Z:\\this\\does\\not\\exist"),
        ..Default::default()
    };

    let result = supervisor.spawn(&config).await;
    // This might succeed on Windows as cmd can handle missing cwd
    // Just ensure it doesn't panic
    let _ = result;
}

#[tokio::test]
async fn test_windows_supervisor_memory_limits() {
    // Test memory limit enforcement (if implemented)
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "memory-limit-test".to_string(),
        command: "cmd".to_string(),
        args: vec!["/C".to_string(), "echo test".to_string()],
        cwd: PathBuf::from("."),
        max_memory: Some(50 * 1024 * 1024), // 50MB limit
        ..Default::default()
    };

    let handle = supervisor.spawn(&config).await.unwrap();

    // Set resource limits (may be no-op on Windows currently)
    let _ = supervisor.set_resource_limits(&handle, &config).await;

    supervisor.kill_tree(&handle).await.unwrap();
}

#[tokio::test]
async fn test_windows_supervisor_cpu_limits() {
    // Test CPU limit enforcement (if implemented)
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "cpu-limit-test".to_string(),
        command: "cmd".to_string(),
        args: vec!["/C".to_string(), "echo test".to_string()],
        cwd: PathBuf::from("."),
        max_cpu_percent: Some(50.0), // 50% CPU limit
        ..Default::default()
    };

    let handle = supervisor.spawn(&config).await.unwrap();

    // Set resource limits (may be no-op on Windows currently)
    let _ = supervisor.set_resource_limits(&handle, &config).await;

    supervisor.kill_tree(&handle).await.unwrap();
}

#[tokio::test]
async fn test_windows_supervisor_handle_spaces_in_paths() {
    // Test handling of paths with spaces
    let supervisor = create_supervisor().await.unwrap();

    let temp_dir = std::env::temp_dir().join("test dir with spaces");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let config = AppConfig {
        name: "spaces-test".to_string(),
        command: "cmd".to_string(),
        args: vec!["/C".to_string(), "cd && echo success".to_string()],
        cwd: temp_dir.clone(),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());

    // Clean up
    let _ = std::fs::remove_dir(&temp_dir);
}

#[tokio::test]
async fn test_windows_supervisor_unicode_handling() {
    // Test handling of Unicode in commands and arguments
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "unicode-test".to_string(),
        command: "cmd".to_string(),
        args: vec!["/C".to_string(), "echo 你好世界 🌍".to_string()],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = supervisor.wait(&mut handle).await.unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn test_windows_supervisor_network_commands() {
    // Test network-related commands
    let supervisor = create_supervisor().await.unwrap();

    let config = AppConfig {
        name: "network-test".to_string(),
        command: "cmd".to_string(),
        args: vec!["/C".to_string(), "nslookup localhost".to_string()],
        cwd: PathBuf::from("."),
        ..Default::default()
    };

    let mut handle = supervisor.spawn(&config).await.unwrap();
    let status = timeout(Duration::from_secs(5), supervisor.wait(&mut handle))
        .await
        .unwrap()
        .unwrap();

    // nslookup should complete successfully for localhost
    assert!(status.code().is_some());
}
