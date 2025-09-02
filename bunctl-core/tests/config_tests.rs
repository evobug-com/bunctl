use bunctl_core::config::{AppConfig, Config, RestartPolicy, BackoffConfig};
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
        },
    };
    
    let config = Config {
        apps: vec![app],
        daemon: Default::default(),
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
        daemon: Default::default(),
    };
    
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: Config = serde_json::from_str(&json).unwrap();
    
    assert!(deserialized.apps.is_empty());
}