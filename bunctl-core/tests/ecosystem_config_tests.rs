use bunctl_core::config::{EcosystemApp, EcosystemConfig, RestartPolicy};
use bunctl_core::config::ecosystem::BoolOrVec;
use std::collections::HashMap;

#[test]
fn test_ecosystem_app_basic() {
    let app = EcosystemApp {
        name: "test-app".to_string(),
        script: "server.ts".to_string(),
        cwd: Some("/app".to_string()),
        args: Some("--watch --port 3000".to_string()),
        interpreter: Some("bun".to_string()),
        interpreter_args: None,
        instances: Some(2),
        exec_mode: Some("cluster".to_string()),
        watch: Some(BoolOrVec::Bool(true)),
        ignore_watch: Some(vec!["node_modules".to_string(), "logs".to_string()]),
        max_memory_restart: Some("512M".to_string()),
        env: Some({
            let mut env = HashMap::new();
            env.insert("PORT".to_string(), "3000".to_string());
            env
        }),
        env_production: None,
        env_development: None,
        error_file: Some("logs/error.log".to_string()),
        out_file: Some("logs/out.log".to_string()),
        log_file: None,
        log_date_format: None,
        merge_logs: Some(true),
        autorestart: Some(true),
        restart_delay: Some(1000),
        min_uptime: None,
        max_restarts: Some(10),
        kill_timeout: Some(5000),
        wait_ready: Some(false),
        listen_timeout: None,
    };
    
    let config = app.to_app_config();
    assert_eq!(config.name, "test-app");
    assert!(config.command.contains("bun"));
    assert!(config.command.contains("server.ts"));
    assert_eq!(config.restart_policy, RestartPolicy::Always);
    assert_eq!(config.max_memory, Some(512 * 1024 * 1024));
    assert_eq!(config.backoff.base_delay_ms, 1000);
    assert_eq!(config.backoff.max_attempts, Some(10));
}

#[test]
fn test_ecosystem_app_minimal() {
    let app = EcosystemApp {
        name: "minimal".to_string(),
        script: "index.js".to_string(),
        cwd: None,
        args: None,
        interpreter: None,
        interpreter_args: None,
        instances: None,
        exec_mode: None,
        watch: None,
        ignore_watch: None,
        max_memory_restart: None,
        env: None,
        env_production: None,
        env_development: None,
        error_file: None,
        out_file: None,
        log_file: None,
        log_date_format: None,
        merge_logs: None,
        autorestart: None,
        restart_delay: None,
        min_uptime: None,
        max_restarts: None,
        kill_timeout: None,
        wait_ready: None,
        listen_timeout: None,
    };
    
    let config = app.to_app_config();
    assert_eq!(config.name, "minimal");
    // The command is built from interpreter + script
    let full_command = format!("{} {}", config.command, config.args.join(" "));
    assert!(full_command.contains("bun")); // Default interpreter
    assert!(full_command.contains("index.js"));
    assert_eq!(config.restart_policy, RestartPolicy::Always); // Default autorestart is true
}

#[test]
fn test_ecosystem_memory_parsing() {
    let test_cases = vec![
        ("100", Some(100)),
        ("100K", Some(100 * 1024)),
        ("100k", Some(100 * 1024)),
        ("100M", Some(100 * 1024 * 1024)),
        ("100m", Some(100 * 1024 * 1024)),
        ("2G", Some(2 * 1024 * 1024 * 1024)),
        ("2g", Some(2 * 1024 * 1024 * 1024)),
        ("", None),
        ("invalid", None),
    ];
    
    for (input, expected) in test_cases {
        let app = EcosystemApp {
            name: "test".to_string(),
            script: "test.js".to_string(),
            max_memory_restart: Some(input.to_string()),
            cwd: None,
            args: None,
            interpreter: None,
            interpreter_args: None,
            instances: None,
            exec_mode: None,
            watch: None,
            ignore_watch: None,
            env: None,
            env_production: None,
            env_development: None,
            error_file: None,
            out_file: None,
            log_file: None,
            log_date_format: None,
            merge_logs: None,
            autorestart: None,
            restart_delay: None,
            min_uptime: None,
            max_restarts: None,
            kill_timeout: None,
            wait_ready: None,
            listen_timeout: None,
        };
        
        let config = app.to_app_config();
        assert_eq!(config.max_memory, expected, "Failed for input: {}", input);
    }
}

#[test]
fn test_ecosystem_environment_override() {
    let mut base_env = HashMap::new();
    base_env.insert("PORT".to_string(), "3000".to_string());
    base_env.insert("DEBUG".to_string(), "false".to_string());
    
    let mut prod_env = HashMap::new();
    prod_env.insert("DEBUG".to_string(), "false".to_string());
    prod_env.insert("LOG_LEVEL".to_string(), "error".to_string());
    
    let mut dev_env = HashMap::new();
    dev_env.insert("DEBUG".to_string(), "true".to_string());
    dev_env.insert("LOG_LEVEL".to_string(), "debug".to_string());
    
    let app = EcosystemApp {
        name: "env-test".to_string(),
        script: "server.js".to_string(),
        env: Some(base_env),
        env_production: Some(prod_env),
        env_development: Some(dev_env),
        cwd: None,
        args: None,
        interpreter: None,
        interpreter_args: None,
        instances: None,
        exec_mode: None,
        watch: None,
        ignore_watch: None,
        max_memory_restart: None,
        error_file: None,
        out_file: None,
        log_file: None,
        log_date_format: None,
        merge_logs: None,
        autorestart: None,
        restart_delay: None,
        min_uptime: None,
        max_restarts: None,
        kill_timeout: None,
        wait_ready: None,
        listen_timeout: None,
    };
    
    // Default should use production
    unsafe { std::env::remove_var("NODE_ENV"); }
    let config = app.to_app_config();
    assert_eq!(config.env.get("PORT"), Some(&"3000".to_string()));
    assert_eq!(config.env.get("DEBUG"), Some(&"false".to_string()));
    assert_eq!(config.env.get("LOG_LEVEL"), Some(&"error".to_string()));
}

#[test]
fn test_bool_or_vec_serialization() {
    let bool_variant = BoolOrVec::Bool(true);
    let vec_variant = BoolOrVec::Vec(vec!["*.js".to_string(), "*.ts".to_string()]);
    
    let bool_json = serde_json::to_string(&bool_variant).unwrap();
    assert_eq!(bool_json, "true");
    
    let vec_json = serde_json::to_string(&vec_variant).unwrap();
    assert_eq!(vec_json, r#"["*.js","*.ts"]"#);
    
    let bool_deserialized: BoolOrVec = serde_json::from_str(&bool_json).unwrap();
    let vec_deserialized: BoolOrVec = serde_json::from_str(&vec_json).unwrap();
    
    match bool_deserialized {
        BoolOrVec::Bool(b) => assert!(b),
        _ => panic!("Expected Bool variant"),
    }
    
    match vec_deserialized {
        BoolOrVec::Vec(v) => assert_eq!(v.len(), 2),
        _ => panic!("Expected Vec variant"),
    }
}

#[test]
fn test_ecosystem_config_multiple_apps() {
    let apps = vec![
        EcosystemApp {
            name: "app1".to_string(),
            script: "app1.js".to_string(),
            cwd: None,
            args: None,
            interpreter: None,
            interpreter_args: None,
            instances: None,
            exec_mode: None,
            watch: None,
            ignore_watch: None,
            max_memory_restart: None,
            env: None,
            env_production: None,
            env_development: None,
            error_file: None,
            out_file: None,
            log_file: None,
            log_date_format: None,
            merge_logs: None,
            autorestart: None,
            restart_delay: None,
            min_uptime: None,
            max_restarts: None,
            kill_timeout: None,
            wait_ready: None,
            listen_timeout: None,
        },
        EcosystemApp {
            name: "app2".to_string(),
            script: "app2.js".to_string(),
            cwd: None,
            args: None,
            interpreter: None,
            interpreter_args: None,
            instances: None,
            exec_mode: None,
            watch: None,
            ignore_watch: None,
            max_memory_restart: None,
            env: None,
            env_production: None,
            env_development: None,
            error_file: None,
            out_file: None,
            log_file: None,
            log_date_format: None,
            merge_logs: None,
            autorestart: None,
            restart_delay: None,
            min_uptime: None,
            max_restarts: None,
            kill_timeout: None,
            wait_ready: None,
            listen_timeout: None,
        },
    ];
    
    let ecosystem = EcosystemConfig { apps };
    
    let json = serde_json::to_string(&ecosystem).unwrap();
    let deserialized: EcosystemConfig = serde_json::from_str(&json).unwrap();
    
    assert_eq!(deserialized.apps.len(), 2);
    assert_eq!(deserialized.apps[0].name, "app1");
    assert_eq!(deserialized.apps[1].name, "app2");
}