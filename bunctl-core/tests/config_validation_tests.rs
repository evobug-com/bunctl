use bunctl_core::config::{
    AppConfig, BackoffConfig, Config, ConfigWatcher, DaemonConfig, ExhaustedAction, HealthCheck,
    HealthCheckType, RestartPolicy,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

// ============================================================================
// Configuration Validation Tests
// ============================================================================

#[tokio::test]
async fn test_config_watcher_file_changes() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("bunctl.json");

    let initial_config = Config {
        apps: vec![AppConfig {
            name: "test-app".to_string(),
            command: "bun".to_string(),
            args: vec!["app.ts".to_string()],
            ..Default::default()
        }],
    };

    let json = serde_json::to_string_pretty(&initial_config).unwrap();
    std::fs::write(&config_path, json).unwrap();

    let watcher = ConfigWatcher::new(&config_path).await.unwrap();
    let config = watcher.get();
    assert_eq!(config.apps.len(), 1);
    assert_eq!(config.apps[0].name, "test-app");

    // Modify the config file
    let updated_config = Config {
        apps: vec![
            AppConfig {
                name: "test-app".to_string(),
                command: "node".to_string(),
                args: vec!["app.js".to_string()],
                ..Default::default()
            },
            AppConfig {
                name: "new-app".to_string(),
                command: "deno".to_string(),
                args: vec!["app.ts".to_string()],
                ..Default::default()
            },
        ],
    };

    let json = serde_json::to_string_pretty(&updated_config).unwrap();
    std::fs::write(&config_path, json).unwrap();

    // Check for reload
    tokio::time::sleep(Duration::from_millis(100)).await;
    let changed = watcher.check_reload().await.unwrap();
    assert!(changed, "Config should have been reloaded");

    let config = watcher.get();
    assert_eq!(config.apps.len(), 2);
    assert_eq!(config.apps[0].command, "node");
    assert_eq!(config.apps[1].name, "new-app");
}

#[tokio::test]
async fn test_config_watcher_no_change() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("bunctl.json");

    let config = Config {
        apps: vec![AppConfig {
            name: "test-app".to_string(),
            command: "bun".to_string(),
            ..Default::default()
        }],
    };

    let json = serde_json::to_string_pretty(&config).unwrap();
    std::fs::write(&config_path, json).unwrap();

    let watcher = ConfigWatcher::new(&config_path).await.unwrap();

    // Check reload without file change
    let changed = watcher.check_reload().await.unwrap();
    assert!(!changed, "Config should not have been reloaded");
}

#[test]
fn test_invalid_backoff_config_validation() {
    // Test invalid multiplier (< 1.0)
    let json = r#"{
        "base_delay_ms": 100,
        "max_delay_ms": 30000,
        "multiplier": 0.5,
        "jitter": 0.3,
        "max_attempts": 10,
        "exhausted_action": "stop"
    }"#;

    let config: BackoffConfig = serde_json::from_str(json).unwrap();
    // The multiplier will be stored as-is, but validation should happen at usage
    assert_eq!(config.multiplier, 0.5);

    // Test invalid jitter (> 1.0)
    let json = r#"{
        "base_delay_ms": 100,
        "max_delay_ms": 30000,
        "multiplier": 2.0,
        "jitter": 1.5,
        "max_attempts": 10,
        "exhausted_action": "stop"
    }"#;

    let config: BackoffConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.jitter, 1.5);

    // Test negative jitter
    let json = r#"{
        "base_delay_ms": 100,
        "max_delay_ms": 30000,
        "multiplier": 2.0,
        "jitter": -0.5,
        "max_attempts": 10,
        "exhausted_action": "stop"
    }"#;

    let config: BackoffConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.jitter, -0.5);
}

#[test]
fn test_app_config_path_handling() {
    // Test relative path conversion
    let json = r#"{
        "name": "test-app",
        "command": "bun",
        "cwd": "./app"
    }"#;

    let config: AppConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.cwd, PathBuf::from("./app"));

    // Test absolute path preservation
    let json = if cfg!(windows) {
        r#"{
            "name": "test-app",
            "command": "bun",
            "cwd": "C:\\Users\\test\\app"
        }"#
    } else {
        r#"{
            "name": "test-app",
            "command": "bun",
            "cwd": "/home/test/app"
        }"#
    };

    let config: AppConfig = serde_json::from_str(json).unwrap();
    if cfg!(windows) {
        assert_eq!(config.cwd, PathBuf::from("C:\\Users\\test\\app"));
    } else {
        assert_eq!(config.cwd, PathBuf::from("/home/test/app"));
    }
}

#[test]
fn test_app_config_log_paths() {
    let json = r#"{
        "name": "test-app",
        "command": "bun",
        "stdout_log": "/var/log/app.out",
        "stderr_log": "/var/log/app.err",
        "combined_log": "/var/log/app.log",
        "log_max_size": 52428800,
        "log_max_files": 5
    }"#;

    let config: AppConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.stdout_log, Some(PathBuf::from("/var/log/app.out")));
    assert_eq!(config.stderr_log, Some(PathBuf::from("/var/log/app.err")));
    assert_eq!(config.combined_log, Some(PathBuf::from("/var/log/app.log")));
    assert_eq!(config.log_max_size, Some(52428800));
    assert_eq!(config.log_max_files, Some(5));
}

#[test]
fn test_app_config_environment_variables() {
    let mut env = HashMap::new();
    env.insert("NODE_ENV".to_string(), "production".to_string());
    env.insert("PORT".to_string(), "3000".to_string());
    env.insert(
        "DATABASE_URL".to_string(),
        "postgres://localhost/db".to_string(),
    );
    env.insert("API_KEY".to_string(), "secret-key-123".to_string());

    let config = AppConfig {
        name: "test-app".to_string(),
        command: "bun".to_string(),
        env: env.clone(),
        ..Default::default()
    };

    assert_eq!(config.env.len(), 4);
    assert_eq!(config.env.get("NODE_ENV"), Some(&"production".to_string()));
    assert_eq!(config.env.get("PORT"), Some(&"3000".to_string()));

    // Test serialization roundtrip
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: AppConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.env, env);
}

#[test]
fn test_restart_policy_edge_cases() {
    let policies = vec![
        ("\"no\"", RestartPolicy::No),
        ("\"always\"", RestartPolicy::Always),
        ("\"on-failure\"", RestartPolicy::OnFailure),
        ("\"unless-stopped\"", RestartPolicy::UnlessStopped),
    ];

    for (json, expected) in policies {
        let deserialized: RestartPolicy = serde_json::from_str(json).unwrap();
        assert_eq!(deserialized, expected);
    }

    // Test invalid policy
    let result: Result<RestartPolicy, _> = serde_json::from_str("\"invalid-policy\"");
    assert!(result.is_err());
}

#[test]
fn test_exhausted_action_edge_cases() {
    let actions = vec![
        ("\"stop\"", ExhaustedAction::Stop),
        ("\"remove\"", ExhaustedAction::Remove),
    ];

    for (json, expected) in actions {
        let deserialized: ExhaustedAction = serde_json::from_str(json).unwrap();
        assert_eq!(deserialized, expected);
    }

    // Test invalid action
    let result: Result<ExhaustedAction, _> = serde_json::from_str("\"invalid-action\"");
    assert!(result.is_err());
}

#[test]
fn test_health_check_serialization() {
    // HTTP health check with all fields
    let http_check = HealthCheck {
        check_type: HealthCheckType::Http {
            url: "https://api.example.com/health".to_string(),
            expected_status: 204,
        },
        interval: Duration::from_secs(45),
        timeout: Duration::from_secs(10),
        retries: 5,
        start_period: Duration::from_secs(120),
    };

    let json = serde_json::to_string(&http_check).unwrap();
    assert!(json.contains("\"type\":\"http\""));
    assert!(json.contains("\"url\":\"https://api.example.com/health\""));
    assert!(json.contains("\"expected_status\":204"));

    let deserialized: HealthCheck = serde_json::from_str(&json).unwrap();
    match deserialized.check_type {
        HealthCheckType::Http {
            url,
            expected_status,
        } => {
            assert_eq!(url, "https://api.example.com/health");
            assert_eq!(expected_status, 204);
        }
        _ => panic!("Wrong health check type"),
    }

    // TCP health check
    let tcp_check = HealthCheck {
        check_type: HealthCheckType::Tcp {
            host: "192.168.1.100".to_string(),
            port: 5432,
        },
        interval: Duration::from_secs(10),
        timeout: Duration::from_secs(2),
        retries: 3,
        start_period: Duration::from_secs(30),
    };

    let json = serde_json::to_string(&tcp_check).unwrap();
    let deserialized: HealthCheck = serde_json::from_str(&json).unwrap();
    match deserialized.check_type {
        HealthCheckType::Tcp { host, port } => {
            assert_eq!(host, "192.168.1.100");
            assert_eq!(port, 5432);
        }
        _ => panic!("Wrong health check type"),
    }

    // Exec health check
    let exec_check = HealthCheck {
        check_type: HealthCheckType::Exec {
            command: "/usr/bin/healthcheck".to_string(),
            args: vec!["--verbose".to_string(), "--timeout=5".to_string()],
        },
        interval: Duration::from_secs(60),
        timeout: Duration::from_secs(15),
        retries: 2,
        start_period: Duration::from_secs(180),
    };

    let json = serde_json::to_string(&exec_check).unwrap();
    let deserialized: HealthCheck = serde_json::from_str(&json).unwrap();
    match deserialized.check_type {
        HealthCheckType::Exec { command, args } => {
            assert_eq!(command, "/usr/bin/healthcheck");
            assert_eq!(args, vec!["--verbose", "--timeout=5"]);
        }
        _ => panic!("Wrong health check type"),
    }
}

#[test]
fn test_daemon_config_log_levels() {
    let valid_levels = vec!["error", "warn", "info", "debug", "trace"];

    for level in valid_levels {
        let config = DaemonConfig {
            log_level: level.to_string(),
            ..Default::default()
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: DaemonConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.log_level, level);
    }
}

#[test]
fn test_config_memory_limits() {
    let json = r#"{
        "name": "memory-limited-app",
        "command": "bun",
        "max_memory": 1073741824,
        "max_cpu_percent": 75.5
    }"#;

    let config: AppConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.max_memory, Some(1073741824)); // 1GB
    assert_eq!(config.max_cpu_percent, Some(75.5));

    // Test edge cases
    let json = r#"{
        "name": "unlimited-app",
        "command": "bun",
        "max_memory": null,
        "max_cpu_percent": null
    }"#;

    let config: AppConfig = serde_json::from_str(json).unwrap();
    assert!(config.max_memory.is_none());
    assert!(config.max_cpu_percent.is_none());
}

#[test]
fn test_config_timeouts() {
    let json = r#"{
        "name": "timeout-app",
        "command": "bun",
        "stop_timeout": {"secs": 30, "nanos": 0},
        "kill_timeout": {"secs": 15, "nanos": 500000000}
    }"#;

    let config: AppConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.stop_timeout, Duration::from_secs(30));
    assert_eq!(config.kill_timeout, Duration::from_millis(15500));
}

#[test]
fn test_empty_string_fields() {
    let json = r#"{
        "name": "",
        "command": ""
    }"#;

    let config: AppConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.name, "");
    assert_eq!(config.command, "");

    // Empty strings should be preserved, not converted to defaults
    assert!(config.name.is_empty());
    assert!(config.command.is_empty());
}

#[test]
fn test_config_with_special_characters() {
    let json = r#"{
        "name": "app-with-special-chars",
        "command": "bun",
        "args": ["--config=\"path with spaces/config.json\"", "--name='quoted name'"],
        "cwd": "/path/with spaces/and-special@chars"
    }"#;

    let config: AppConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.name, "app-with-special-chars");
    assert_eq!(config.args[0], "--config=\"path with spaces/config.json\"");
    assert_eq!(config.args[1], "--name='quoted name'");
    assert_eq!(
        config.cwd,
        PathBuf::from("/path/with spaces/and-special@chars")
    );
}

#[test]
fn test_config_unknown_fields_rejection() {
    // Test that unknown fields are rejected due to deny_unknown_fields
    let json = r#"{
        "name": "test-app",
        "command": "bun",
        "unknown_field": "should fail",
        "another_unknown": 123
    }"#;

    let result: Result<AppConfig, _> = serde_json::from_str(json);
    assert!(result.is_err(), "Should reject unknown fields");

    // Test BackoffConfig with unknown fields
    let json = r#"{
        "base_delay_ms": 100,
        "max_delay_ms": 30000,
        "multiplier": 2.0,
        "jitter": 0.3,
        "max_attempts": 10,
        "exhausted_action": "stop",
        "unknown_backoff_field": true
    }"#;

    let result: Result<BackoffConfig, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "BackoffConfig should reject unknown fields"
    );

    // Test HealthCheck with unknown fields
    let json = r#"{
        "check_type": {"type": "tcp", "host": "localhost", "port": 8080},
        "interval": {"secs": 30, "nanos": 0},
        "timeout": {"secs": 5, "nanos": 0},
        "retries": 3,
        "start_period": {"secs": 60, "nanos": 0},
        "unknown_health_field": "invalid"
    }"#;

    let result: Result<HealthCheck, _> = serde_json::from_str(json);
    assert!(result.is_err(), "HealthCheck should reject unknown fields");
}

#[tokio::test]
async fn test_daemon_config_empty_socket_path() {
    let daemon = DaemonConfig {
        socket_path: PathBuf::from(""),
        log_level: "info".to_string(),
        metrics_port: None,
        max_parallel_starts: 4,
    };

    let json = serde_json::to_string(&daemon).unwrap();
    let path = std::env::temp_dir().join("test_empty_socket.json");
    std::fs::write(&path, &json).unwrap();

    let result = DaemonConfig::load_from_file(&path).await;
    assert!(result.is_err(), "Should reject empty socket path");

    std::fs::remove_file(&path).ok();
}

#[tokio::test]
async fn test_daemon_config_invalid_log_level() {
    let daemon = DaemonConfig {
        socket_path: PathBuf::from("/tmp/test.sock"),
        log_level: "invalid-level".to_string(),
        metrics_port: None,
        max_parallel_starts: 4,
    };

    let json = serde_json::to_string(&daemon).unwrap();
    let path = std::env::temp_dir().join("test_invalid_log.json");
    std::fs::write(&path, &json).unwrap();

    let result = DaemonConfig::load_from_file(&path).await;
    assert!(result.is_err(), "Should reject invalid log level");

    std::fs::remove_file(&path).ok();
}

#[test]
fn test_large_config_file() {
    // Test with many apps
    let mut apps = Vec::new();
    for i in 0..100 {
        apps.push(AppConfig {
            name: format!("app-{}", i),
            command: "bun".to_string(),
            args: vec![format!("app{}.ts", i)],
            env: {
                let mut env = HashMap::new();
                env.insert("APP_ID".to_string(), i.to_string());
                env.insert("PORT".to_string(), (3000 + i).to_string());
                env
            },
            auto_start: i % 2 == 0,
            restart_policy: if i % 3 == 0 {
                RestartPolicy::Always
            } else if i % 3 == 1 {
                RestartPolicy::OnFailure
            } else {
                RestartPolicy::No
            },
            ..Default::default()
        });
    }

    let config = Config { apps };

    // Test serialization/deserialization performance
    let json = serde_json::to_string(&config).unwrap();
    assert!(
        json.len() > 1000,
        "Large config should produce substantial JSON"
    );

    let deserialized: Config = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.apps.len(), 100);

    // Verify some random apps
    assert_eq!(deserialized.apps[0].name, "app-0");
    assert_eq!(deserialized.apps[50].name, "app-50");
    assert_eq!(deserialized.apps[99].name, "app-99");
    assert!(deserialized.apps[0].auto_start);
    assert!(!deserialized.apps[1].auto_start);
}

#[test]
fn test_config_defaults_consistency() {
    let default1 = AppConfig::default();
    let default2 = AppConfig::default();

    // Defaults should be consistent
    assert_eq!(default1.stop_timeout, default2.stop_timeout);
    assert_eq!(default1.kill_timeout, default2.kill_timeout);
    assert_eq!(default1.log_max_size, default2.log_max_size);
    assert_eq!(default1.log_max_files, default2.log_max_files);
    assert_eq!(default1.restart_policy, default2.restart_policy);

    // Test BackoffConfig defaults
    let backoff1 = BackoffConfig::default();
    let backoff2 = BackoffConfig::default();

    assert_eq!(backoff1.base_delay_ms, backoff2.base_delay_ms);
    assert_eq!(backoff1.max_delay_ms, backoff2.max_delay_ms);
    assert_eq!(backoff1.multiplier, backoff2.multiplier);
    assert_eq!(backoff1.jitter, backoff2.jitter);
    assert_eq!(backoff1.exhausted_action, backoff2.exhausted_action);
}
