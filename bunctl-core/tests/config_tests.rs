use bunctl_core::config::{AppConfig, BackoffConfig, Config, RestartPolicy};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

#[test]
fn test_app_config_default() {
    let config = AppConfig::default();
    assert_eq!(config.name, "");
    assert_eq!(config.command, "");
    assert!(config.args.is_empty());
    assert!(config.env.is_empty());
    assert!(!config.auto_start);
    assert_eq!(config.restart_policy, RestartPolicy::OnFailure);
    assert_eq!(config.stop_timeout, Duration::from_secs(10));
    assert_eq!(config.kill_timeout, Duration::from_secs(5));
}

#[test]
fn test_restart_policy_serialization() {
    let policies = vec![
        RestartPolicy::No,
        RestartPolicy::Always,
        RestartPolicy::OnFailure,
        RestartPolicy::UnlessStopped,
    ];

    for policy in policies {
        let json = serde_json::to_string(&policy).unwrap();
        let deserialized: RestartPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(policy, deserialized);
    }
}

#[test]
fn test_config_serialization() {
    let mut env = HashMap::new();
    env.insert("PORT".to_string(), "3000".to_string());
    env.insert("NODE_ENV".to_string(), "production".to_string());

    let app = AppConfig {
        name: "test-app".to_string(),
        command: "bun run server.ts".to_string(),
        args: vec!["--watch".to_string()],
        cwd: PathBuf::from("/app"),
        env,
        auto_start: true,
        restart_policy: RestartPolicy::Always,
        max_memory: Some(512 * 1024 * 1024),
        max_cpu_percent: Some(50.0),
        uid: Some(1000),
        gid: Some(1000),
        stdout_log: Some(PathBuf::from("logs/out.log")),
        stderr_log: Some(PathBuf::from("logs/err.log")),
        combined_log: None,
        log_max_size: Some(10 * 1024 * 1024),
        log_max_files: Some(5),
        health_check: None,
        stop_timeout: Duration::from_secs(30),
        kill_timeout: Duration::from_secs(10),
        backoff: BackoffConfig {
            base_delay_ms: 200,
            max_delay_ms: 60000,
            multiplier: 2.5,
            jitter: 0.2,
            max_attempts: Some(10),
            exhausted_action: bunctl_core::config::ExhaustedAction::Stop,
        },
    };

    let config = Config {
        apps: vec![app],
    };

    let json = serde_json::to_string_pretty(&config).unwrap();
    let deserialized: Config = serde_json::from_str(&json).unwrap();

    assert_eq!(config.apps.len(), deserialized.apps.len());
    assert_eq!(config.apps[0].name, deserialized.apps[0].name);
    assert_eq!(config.apps[0].command, deserialized.apps[0].command);
    assert_eq!(config.apps[0].max_memory, deserialized.apps[0].max_memory);
}

#[test]
fn test_backoff_config_default() {
    let backoff = BackoffConfig::default();
    assert_eq!(backoff.base_delay_ms, 100);
    assert_eq!(backoff.max_delay_ms, 30000);
    assert_eq!(backoff.multiplier, 2.0);
    assert_eq!(backoff.jitter, 0.3);
    assert_eq!(backoff.max_attempts, None);
}

#[cfg(unix)]
#[test]
fn test_unix_specific_config() {
    let app = AppConfig {
        name: "unix-app".to_string(),
        command: "node".to_string(),
        uid: Some(1000),
        gid: Some(1000),
        ..Default::default()
    };

    assert_eq!(app.uid, Some(1000));
    assert_eq!(app.gid, Some(1000));
}

#[test]
fn test_empty_config() {
    let config = Config {
        apps: vec![],
    };

    let json = serde_json::to_string(&config).unwrap();
    let deserialized: Config = serde_json::from_str(&json).unwrap();

    assert!(deserialized.apps.is_empty());
}

#[test]
fn test_daemon_config_rejects_unknown_fields() {
    use bunctl_core::config::DaemonConfig;

    let invalid_daemon_json = r#"{
        "socket_path": "/tmp/bunctl.sock",
        "log_level": "info",
        "metrics_port": 3001,
        "max_parallel_starts": 4,
        "unknown_field": "should_fail",
        "another_unknown": 123
    }"#;

    let result: Result<DaemonConfig, _> = serde_json::from_str(invalid_daemon_json);
    assert!(result.is_err(), "DaemonConfig should fail with unknown daemon fields");
    
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("unknown field"), "Error should mention unknown field");
}

#[test]
fn test_daemon_config_accepts_valid_fields() {
    use bunctl_core::config::DaemonConfig;

    let valid_daemon_json = r#"{
        "socket_path": "/tmp/custom.sock",
        "log_level": "debug",
        "metrics_port": 9090,
        "max_parallel_starts": 8
    }"#;

    let result: Result<DaemonConfig, _> = serde_json::from_str(valid_daemon_json);
    assert!(result.is_ok(), "DaemonConfig should accept valid daemon fields");
    
    let config = result.unwrap();
    assert_eq!(config.socket_path.to_string_lossy(), "/tmp/custom.sock");
    assert_eq!(config.log_level, "debug");
    assert_eq!(config.metrics_port, Some(9090));
    assert_eq!(config.max_parallel_starts, 8);
}

#[tokio::test]
async fn test_daemon_config_validation() {
    use bunctl_core::config::DaemonConfig;
    use std::path::PathBuf;

    // Test invalid max_parallel_starts (0)
    let daemon_zero_parallel = DaemonConfig {
        socket_path: PathBuf::from("/tmp/test.sock"),
        log_level: "info".to_string(),
        metrics_port: None,
        max_parallel_starts: 0,
    };
    let json = serde_json::to_string(&daemon_zero_parallel).unwrap();
    let path = std::env::temp_dir().join("test_daemon_validation.json");
    std::fs::write(&path, &json).unwrap();
    let daemon_result = DaemonConfig::load_from_file(&path).await;
    assert!(daemon_result.is_err(), "Should reject max_parallel_starts = 0");
    std::fs::remove_file(&path).ok();

    // Test invalid max_parallel_starts (too high)
    let daemon_high_parallel = DaemonConfig {
        socket_path: PathBuf::from("/tmp/test.sock"),
        log_level: "info".to_string(),
        metrics_port: None,
        max_parallel_starts: 101,
    };
    let json = serde_json::to_string(&daemon_high_parallel).unwrap();
    let path = std::env::temp_dir().join("test_daemon_validation_high.json");
    std::fs::write(&path, &json).unwrap();
    let daemon_result = DaemonConfig::load_from_file(&path).await;
    assert!(daemon_result.is_err(), "Should reject max_parallel_starts > 100");
    std::fs::remove_file(&path).ok();

    // Test invalid metrics port (privileged)
    let daemon_priv_port = DaemonConfig {
        socket_path: PathBuf::from("/tmp/test.sock"),
        log_level: "info".to_string(),
        metrics_port: Some(80),
        max_parallel_starts: 4,
    };
    let json = serde_json::to_string(&daemon_priv_port).unwrap();
    let path = std::env::temp_dir().join("test_daemon_validation_port.json");
    std::fs::write(&path, &json).unwrap();
    let daemon_result = DaemonConfig::load_from_file(&path).await;
    assert!(daemon_result.is_err(), "Should reject privileged port");
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_app_config_no_daemon_section() {
    let config = Config {
        apps: vec![],
    };

    let json = serde_json::to_string(&config).unwrap();
    
    // App configs should never contain daemon section
    assert!(!json.contains("daemon"), "App config should not contain daemon section");
    assert_eq!(json, r#"{"apps":[]}"#);
    
    // Should still deserialize properly
    let deserialized: Config = serde_json::from_str(&json).unwrap();
    assert!(deserialized.apps.is_empty());
}
