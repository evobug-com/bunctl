use bunctl_core::config::{ConfigLoader, EcosystemConfig};
use std::path::Path;
use tempfile::TempDir;
use tokio::fs;

#[tokio::test]
async fn test_js_config_execution_blocked() {
    // Create a malicious ecosystem.config.js that would execute code if allowed
    let temp_dir = TempDir::new().unwrap();
    let js_config_path = temp_dir.path().join("ecosystem.config.js");

    // This would be dangerous if executed
    let malicious_js = r#"
        const fs = require('fs');
        fs.writeFileSync('/tmp/pwned.txt', 'code executed!');
        module.exports = {
            apps: [{
                name: 'evil',
                script: 'app.js'
            }]
        };
    "#;

    fs::write(&js_config_path, malicious_js).await.unwrap();

    // Try to load the config
    let result = EcosystemConfig::load_from_js(&js_config_path).await;

    // Should fail with security error
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(err_msg.contains("security reasons"));
    assert!(err_msg.contains("JavaScript config files are not supported"));

    // Verify that the malicious code was NOT executed
    assert!(!Path::new("/tmp/pwned.txt").exists());
}

#[tokio::test]
async fn test_json_config_still_works() {
    // Create a valid JSON config
    let temp_dir = TempDir::new().unwrap();
    let json_config_path = temp_dir.path().join("ecosystem.config.json");

    let valid_json = r#"{
        "apps": [{
            "name": "myapp",
            "script": "./index.js",
            "interpreter": "bun"
        }]
    }"#;

    fs::write(&json_config_path, valid_json).await.unwrap();

    // Should load successfully
    let result = EcosystemConfig::load_from_json(&json_config_path).await;
    assert!(result.is_ok());

    let config = result.unwrap();
    assert_eq!(config.apps.len(), 1);
    assert_eq!(config.apps[0].name, "myapp");
}

#[tokio::test]
async fn test_config_loader_skips_js_files() {
    let temp_dir = TempDir::new().unwrap();

    // Create a JS config file
    let js_config = r#"
        module.exports = {
            apps: [{
                name: 'testapp',
                script: './test.js'
            }]
        };
    "#;
    fs::write(temp_dir.path().join("ecosystem.config.js"), js_config)
        .await
        .unwrap();

    // Create a JSON alternative
    let json_config = r#"{
        "apps": [{
            "name": "jsonapp",
            "script": "./app.js"
        }]
    }"#;
    fs::write(temp_dir.path().join("ecosystem.config.json"), json_config)
        .await
        .unwrap();

    // Load config from the directory
    let loader = ConfigLoader::new().with_search_path(temp_dir.path());
    let result = loader.load().await;

    // Should load the JSON config, not the JS one
    assert!(result.is_ok());
    let config = result.unwrap();
    assert_eq!(config.apps.len(), 1);
    assert_eq!(config.apps[0].name, "jsonapp");
}

#[tokio::test]
async fn test_direct_js_file_load_fails() {
    let temp_dir = TempDir::new().unwrap();
    let js_path = temp_dir.path().join("ecosystem.config.js");

    fs::write(&js_path, "module.exports = { apps: [] }")
        .await
        .unwrap();

    let loader = ConfigLoader::new();
    let result = loader.load_file(&js_path).await;

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("JavaScript config files are not supported"));
}

#[tokio::test]
async fn test_ecosystem_json_loading_works() {
    let temp_dir = TempDir::new().unwrap();
    let json_path = temp_dir.path().join("ecosystem.config.json");
    
    let json_config = r#"{
        "apps": [{
            "name": "test-app",
            "script": "./app.js",
            "interpreter": "bun",
            "max_restarts": 5,
            "env": {
                "NODE_ENV": "production",
                "PORT": "3000"
            }
        }]
    }"#;
    
    fs::write(&json_path, json_config).await.unwrap();
    
    // Test direct file loading
    let loader = ConfigLoader::new();
    let result = loader.load_file(&json_path).await;
    assert!(result.is_ok(), "Failed to load ecosystem.config.json");
    
    let config = result.unwrap();
    assert_eq!(config.apps.len(), 1);
    assert_eq!(config.apps[0].name, "test-app");
    assert_eq!(config.apps[0].command, "bun");
    assert_eq!(config.apps[0].args, vec!["./app.js"]);
    assert_eq!(config.apps[0].env.get("NODE_ENV"), Some(&"production".to_string()));
    assert_eq!(config.apps[0].env.get("PORT"), Some(&"3000".to_string()));
}

#[tokio::test]
async fn test_pm2_json_loading_works() {
    let temp_dir = TempDir::new().unwrap();
    let json_path = temp_dir.path().join("pm2.config.json");
    
    let json_config = r#"{
        "apps": [{
            "name": "pm2-app",
            "script": "server.ts",
            "interpreter": "none",
            "max_memory_restart": "512M"
        }]
    }"#;
    
    fs::write(&json_path, json_config).await.unwrap();
    
    let loader = ConfigLoader::new();
    let result = loader.load_file(&json_path).await;
    assert!(result.is_ok(), "Failed to load pm2.config.json");
    
    let config = result.unwrap();
    assert_eq!(config.apps.len(), 1);
    assert_eq!(config.apps[0].name, "pm2-app");
    assert_eq!(config.apps[0].command, "server.ts");
    assert_eq!(config.apps[0].args.len(), 0);
    assert_eq!(config.apps[0].max_memory, Some(512 * 1024 * 1024));
}
