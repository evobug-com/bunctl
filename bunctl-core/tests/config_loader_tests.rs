use bunctl_core::config::ConfigLoader;
use std::path::PathBuf;
use tempfile::TempDir;

#[tokio::test]
async fn test_load_bunctl_json() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("bunctl.json");

    let config = r#"{
        "apps": [
            {
                "name": "test-app",
                "command": "bun run server.ts",
                "cwd": "/app",
                "env": {
                    "PORT": "3000"
                },
                "auto_start": true,
                "restart_policy": "always",
                "max_memory": 536870912,
                "max_cpu_percent": 50.0
            }
        ],
        "daemon": {
            "socket_path": "/tmp/bunctl.sock",
            "log_level": "info",
            "max_parallel_starts": 4
        }
    }"#;

    std::fs::write(&config_path, config).unwrap();

    let loader = ConfigLoader::new();
    let loaded = loader.load_file(&config_path).await.unwrap();

    assert_eq!(loaded.apps.len(), 1);
    assert_eq!(loaded.apps[0].name, "test-app");
    assert_eq!(loaded.apps[0].command, "bun");
    assert_eq!(loaded.apps[0].args, vec!["run", "server.ts"]);
    assert_eq!(loaded.apps[0].max_memory, Some(536870912));
    assert_eq!(loaded.apps[0].max_cpu_percent, Some(50.0));
}

#[tokio::test]
async fn test_load_ecosystem_json() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("ecosystem.config.json");

    let config = r#"{
        "apps": [
            {
                "name": "api",
                "script": "src/server.ts",
                "interpreter": "bun",
                "instances": 2,
                "exec_mode": "cluster",
                "max_memory_restart": "512M",
                "autorestart": true,
                "env": {
                    "NODE_ENV": "production",
                    "PORT": "3000"
                }
            }
        ]
    }"#;

    std::fs::write(&config_path, config).unwrap();

    let loader = ConfigLoader::new();
    let loaded = loader.load_file(&config_path).await.unwrap();

    assert_eq!(loaded.apps.len(), 1);
    assert_eq!(loaded.apps[0].name, "api");
    assert!(loaded.apps[0].command.contains("bun"));
    assert!(loaded.apps[0].command.contains("src/server.ts"));
    assert_eq!(loaded.apps[0].max_memory, Some(512 * 1024 * 1024));
}

#[tokio::test]
async fn test_auto_discovery_bunctl_json() {
    let temp_dir = TempDir::new().unwrap();

    let config = r#"{
        "apps": [
            {
                "name": "discovered",
                "command": "node app.js"
            }
        ]
    }"#;

    std::fs::write(temp_dir.path().join("bunctl.json"), config).unwrap();

    // Use with_search_path instead of changing current directory
    let loader = ConfigLoader::new().with_search_path(temp_dir.path());
    let loaded = loader.load().await.unwrap();

    assert_eq!(loaded.apps.len(), 1);
    assert_eq!(loaded.apps[0].name, "discovered");
}

#[tokio::test]
#[ignore = "Requires Bun to be installed"]
async fn test_auto_discovery_ecosystem_js() {
    let temp_dir = TempDir::new().unwrap();

    // Create a simple ecosystem.config.js that exports JSON
    let config = r#"module.exports = {
        apps: [{
            name: 'eco-app',
            script: 'server.js',
            interpreter: 'node'
        }]
    };"#;

    std::fs::write(temp_dir.path().join("ecosystem.config.js"), config).unwrap();

    // This test would need Bun installed to actually work
    // For now, we'll test the fallback behavior
    let loader = ConfigLoader::new().with_search_path(temp_dir.path());
    let loaded = loader.load().await;

    // Should either succeed if Bun is installed, or return default config
    assert!(loaded.is_ok());
}

#[tokio::test]
async fn test_load_package_json_with_bunctl_section() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("package.json");

    let config = r#"{
        "name": "my-app",
        "version": "1.0.0",
        "scripts": {
            "start": "bun run src/index.ts",
            "dev": "bun --watch src/index.ts"
        },
        "bunctl": {
            "apps": [
                {
                    "name": "my-app",
                    "command": "bun run start",
                    "auto_start": true
                }
            ]
        }
    }"#;

    std::fs::write(&config_path, config).unwrap();

    let loader = ConfigLoader::new();
    let loaded = loader.load_file(&config_path).await.unwrap();

    assert_eq!(loaded.apps.len(), 1);
    assert_eq!(loaded.apps[0].name, "my-app");
    assert_eq!(loaded.apps[0].command, "bun");
    assert_eq!(loaded.apps[0].args, vec!["run", "start"]);
    assert!(loaded.apps[0].auto_start);
}

#[tokio::test]
async fn test_load_package_json_with_pm2_section() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("package.json");

    let config = r#"{
        "name": "pm2-app",
        "version": "1.0.0",
        "pm2": {
            "apps": [
                {
                    "name": "pm2-app",
                    "script": "index.js",
                    "interpreter": "node"
                }
            ]
        }
    }"#;

    std::fs::write(&config_path, config).unwrap();

    let loader = ConfigLoader::new();
    let loaded = loader.load_file(&config_path).await.unwrap();

    assert_eq!(loaded.apps.len(), 1);
    assert_eq!(loaded.apps[0].name, "pm2-app");
}

#[tokio::test]
async fn test_load_package_json_fallback_to_start_script() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("package.json");

    let config = r#"{
        "name": "simple-app",
        "version": "1.0.0",
        "scripts": {
            "start": "node server.js"
        }
    }"#;

    std::fs::write(&config_path, config).unwrap();

    let loader = ConfigLoader::new();
    let loaded = loader.load_file(&config_path).await.unwrap();

    assert_eq!(loaded.apps.len(), 1);
    assert_eq!(loaded.apps[0].name, "simple-app");
    assert_eq!(loaded.apps[0].command, "bun");
    assert_eq!(loaded.apps[0].args, vec!["run", "start"]);
}

#[tokio::test]
async fn test_load_nonexistent_file() {
    let loader = ConfigLoader::new();
    let result = loader.load_file(&PathBuf::from("nonexistent.json")).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_load_invalid_json() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("invalid.json");

    std::fs::write(&config_path, "{ invalid json }").unwrap();

    let loader = ConfigLoader::new();
    let result = loader.load_file(&config_path).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_custom_search_paths() {
    let temp_dir = TempDir::new().unwrap();
    let config_dir = temp_dir.path().join("config");
    std::fs::create_dir(&config_dir).unwrap();

    let config = r#"{
        "apps": [
            {
                "name": "custom-path-app",
                "command": "bun run app.ts"
            }
        ]
    }"#;

    std::fs::write(config_dir.join("bunctl.json"), config).unwrap();

    let loader = ConfigLoader::new().with_search_path(&config_dir);

    let loaded = loader.load().await.unwrap();

    assert_eq!(loaded.apps.len(), 1);
    assert_eq!(loaded.apps[0].name, "custom-path-app");
}

#[tokio::test]
async fn test_empty_config() {
    let temp_dir = TempDir::new().unwrap();

    // No config files present
    let loader = ConfigLoader::new().with_search_path(temp_dir.path());
    let loaded = loader.load().await.unwrap();

    // Should return default empty config
    assert_eq!(loaded.apps.len(), 0);
}
