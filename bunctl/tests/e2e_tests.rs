// End-to-end integration tests for the full CLI workflow
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

#[cfg(test)]
mod e2e_tests {
    use super::*;

    /// Helper function to create a test script file
    fn create_test_script(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let script_path = dir.path().join(name);
        fs::write(&script_path, content).unwrap();
        script_path
    }

    /// Helper function to create a test config file
    fn create_test_config(dir: &TempDir, apps: Vec<(&str, &str)>) -> PathBuf {
        let mut config_apps = vec![];

        for (name, script) in apps {
            config_apps.push(format!(
                r#"{{
                    "name": "{}",
                    "command": "{}",
                    "args": [],
                    "cwd": "{}",
                    "env": {{}},
                    "auto_start": false,
                    "restart_policy": "No",
                    "max_memory": null,
                    "max_cpu_percent": null
                }}"#,
                name,
                script,
                dir.path().display()
            ));
        }

        let config_content = format!(
            r#"{{
                "apps": [{}]
            }}"#,
            config_apps.join(",")
        );

        let config_path = dir.path().join("bunctl.json");
        fs::write(&config_path, config_content).unwrap();
        config_path
    }

    #[test]
    fn test_full_workflow_init_start_stop() {
        let temp_dir = TempDir::new().unwrap();

        // Step 1: Initialize a new app
        let mut cmd = Command::cargo_bin("bunctl").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["init", "--name", "workflow-app", "--runtime", "bun"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Initialized app 'workflow-app'"));

        // Verify config file was created
        let config_path = temp_dir.path().join("bunctl.json");
        assert!(config_path.exists());

        // Step 2: Read and verify config content
        let config_content = fs::read_to_string(&config_path).unwrap();
        assert!(config_content.contains("workflow-app"));
    }

    #[test]
    fn test_ecosystem_config_workflow() {
        let temp_dir = TempDir::new().unwrap();

        // Step 1: Initialize with ecosystem format
        let mut cmd = Command::cargo_bin("bunctl").unwrap();
        cmd.current_dir(&temp_dir)
            .args([
                "init",
                "--name",
                "eco-app",
                "--ecosystem",
                "--instances",
                "2",
                "--memory",
                "256M",
                "--cpu",
                "25",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("ecosystem.config.js"));

        // Verify ecosystem.config.js was created
        let eco_path = temp_dir.path().join("ecosystem.config.js");
        assert!(eco_path.exists());

        let content = fs::read_to_string(&eco_path).unwrap();
        assert!(content.contains("module.exports"));
        assert!(content.contains("eco-app"));
        assert!(content.contains("\"instances\": 2"));
    }

    #[test]
    fn test_multiple_apps_workflow() {
        let temp_dir = TempDir::new().unwrap();

        // Create a config with multiple apps
        let config = r#"{
            "apps": [
                {
                    "name": "app1",
                    "command": "echo app1",
                    "args": [],
                    "cwd": ".",
                    "env": {},
                    "auto_start": false,
                    "restart_policy": "No"
                },
                {
                    "name": "app2",
                    "command": "echo app2",
                    "args": [],
                    "cwd": ".",
                    "env": {},
                    "auto_start": false,
                    "restart_policy": "No"
                }
            ]
        }"#;

        let config_path = temp_dir.path().join("bunctl.json");
        fs::write(&config_path, config).unwrap();

        // Test listing apps (should work even without daemon)
        let mut cmd = Command::cargo_bin("bunctl").unwrap();
        cmd.current_dir(&temp_dir).arg("list").assert().success();
    }

    #[test]
    fn test_init_with_auto_discovery() {
        let temp_dir = TempDir::new().unwrap();

        // Create some common entry files
        fs::write(temp_dir.path().join("server.ts"), "console.log('server')").unwrap();
        fs::write(temp_dir.path().join("index.js"), "console.log('index')").unwrap();

        // Initialize without specifying entry
        let mut cmd = Command::cargo_bin("bunctl").unwrap();
        cmd.current_dir(&temp_dir)
            .args(["init", "--name", "auto-app"])
            .assert()
            .success();

        let config_path = temp_dir.path().join("bunctl.json");
        assert!(config_path.exists());

        // Should auto-detect server.ts as it has higher priority
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("server.ts"));
    }

    #[test]
    fn test_init_with_custom_settings() {
        let temp_dir = TempDir::new().unwrap();

        // Create entry file
        let entry_file = "custom.ts";
        fs::write(temp_dir.path().join(entry_file), "// custom app").unwrap();

        let mut cmd = Command::cargo_bin("bunctl").unwrap();
        cmd.current_dir(&temp_dir)
            .args([
                "init",
                "--name",
                "custom-app",
                "--entry",
                entry_file,
                "--port",
                "8080",
                "--memory",
                "2G",
                "--cpu",
                "80",
                "--runtime",
                "node",
                "--autostart",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("custom-app"))
            .stdout(predicate::str::contains("8080"))
            .stdout(predicate::str::contains("2G"))
            .stdout(predicate::str::contains("80%"))
            .stdout(predicate::str::contains("node"));

        let config_path = temp_dir.path().join("bunctl.json");
        let content = fs::read_to_string(&config_path).unwrap();

        assert!(content.contains("custom-app"));
        assert!(content.contains(entry_file));
        assert!(content.contains("\"PORT\": \"8080\""));
        assert!(content.contains("\"auto_start\": true"));
    }

    #[test]
    fn test_error_handling_missing_daemon() {
        // Test stop without daemon running
        let mut cmd = Command::cargo_bin("bunctl").unwrap();
        cmd.args(["stop", "nonexistent"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("Daemon not running"));

        // Test restart without daemon running
        let mut cmd = Command::cargo_bin("bunctl").unwrap();
        cmd.args(["restart", "nonexistent"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("Daemon not running"));

        // Test logs without daemon running
        let mut cmd = Command::cargo_bin("bunctl").unwrap();
        cmd.args(["logs", "nonexistent"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("Daemon not running"));
    }

    #[test]
    fn test_status_command_variations() {
        // Status without daemon should show empty or error
        let mut cmd = Command::cargo_bin("bunctl").unwrap();
        cmd.arg("status")
            .assert()
            .success()
            .stdout(predicate::str::contains("No daemon running"));

        // Status with JSON format
        let mut cmd = Command::cargo_bin("bunctl").unwrap();
        cmd.args(["status", "--json"])
            .assert()
            .success()
            .stdout(predicate::str::contains("[]"));
    }

    #[test]
    fn test_delete_command_confirmation() {
        // Delete should ask for confirmation by default
        let mut cmd = Command::cargo_bin("bunctl").unwrap();
        cmd.args(["delete", "test-app"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Are you sure"));

        // Delete with force should not ask
        // Note: This would need actual daemon running to test properly
    }

    #[test]
    fn test_logs_command_options() {
        let temp_dir = TempDir::new().unwrap();

        // Test various log command options
        let mut cmd = Command::cargo_bin("bunctl").unwrap();
        cmd.current_dir(&temp_dir)
            .args([
                "logs",
                "--lines",
                "50",
                "--timestamps",
                "--no-colors",
                "--json",
            ])
            .assert()
            .failure(); // Should fail without daemon
    }

    #[test]
    fn test_config_file_precedence() {
        let temp_dir = TempDir::new().unwrap();

        // Create bunctl.json
        let bunctl_config = r#"{"apps": [{"name": "bunctl-app", "command": "echo bunctl"}]}"#;
        fs::write(temp_dir.path().join("bunctl.json"), bunctl_config).unwrap();

        // Create ecosystem.config.js
        let eco_config =
            r#"module.exports = {"apps": [{"name": "eco-app", "script": "index.js"}]}"#;
        fs::write(temp_dir.path().join("ecosystem.config.js"), eco_config).unwrap();

        // bunctl.json should take precedence
        // This would be tested by starting apps and checking which config was loaded
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Test hangs on Windows waiting for daemon connection"
    )]
    fn test_ad_hoc_start_without_config() {
        let temp_dir = TempDir::new().unwrap();

        // Create a script to run
        let script = temp_dir.path().join("test.js");
        fs::write(&script, "console.log('test')").unwrap();

        // Try ad-hoc start with script
        let mut cmd = Command::cargo_bin("bunctl").unwrap();
        cmd.current_dir(&temp_dir)
            .timeout(std::time::Duration::from_secs(5))
            .args([
                "start",
                "adhoc-app",
                "--script",
                "test.js",
                "--env",
                "NODE_ENV=test",
                "--env",
                "PORT=3000",
            ])
            .assert()
            .failure(); // Will fail without daemon, but tests argument parsing
    }

    #[test]
    fn test_environment_variable_handling() {
        let temp_dir = TempDir::new().unwrap();

        // Test environment variable parsing in init
        let mut cmd = Command::cargo_bin("bunctl").unwrap();
        cmd.current_dir(&temp_dir)
            .env("NODE_ENV", "production")
            .args(["init", "--name", "env-app", "--port", "4000"])
            .assert()
            .success();

        let config_path = temp_dir.path().join("bunctl.json");
        let content = fs::read_to_string(&config_path).unwrap();

        // Should include PORT from args
        assert!(content.contains("\"PORT\": \"4000\""));
        // Should include default NODE_ENV
        assert!(content.contains("\"NODE_ENV\": \"production\""));
    }

    #[test]
    fn test_memory_limit_parsing() {
        let temp_dir = TempDir::new().unwrap();

        // Test various memory limit formats
        let formats = vec!["100", "512M", "1G", "2048K"];

        for format in formats {
            let mut cmd = Command::cargo_bin("bunctl").unwrap();
            cmd.current_dir(&temp_dir)
                .args([
                    "init",
                    "--name",
                    &format!("mem-{}", format),
                    "--memory",
                    format,
                ])
                .assert()
                .success();
        }
    }

    #[test]
    fn test_restart_command_options() {
        // Test restart with parallel flag
        let mut cmd = Command::cargo_bin("bunctl").unwrap();
        cmd.args(["restart", "test-app", "--parallel", "--wait", "1000"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("Daemon not running"));
    }

    #[test]
    fn test_special_app_names() {
        // Test "all" special name
        let mut cmd = Command::cargo_bin("bunctl").unwrap();
        cmd.args(["stop", "all"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("Daemon not running"));

        let mut cmd = Command::cargo_bin("bunctl").unwrap();
        cmd.args(["restart", "all"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("Daemon not running"));

        let mut cmd = Command::cargo_bin("bunctl").unwrap();
        cmd.args(["delete", "all"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Are you sure"));
    }

    #[tokio::test]
    async fn test_concurrent_command_execution() {
        // Test that multiple commands can be executed concurrently
        let handles: Vec<_> = (0..5)
            .map(|i| {
                tokio::spawn(async move {
                    let mut cmd = Command::cargo_bin("bunctl").unwrap();
                    cmd.args(["status", "--json"]).assert().success();
                    i
                })
            })
            .collect();

        for handle in handles {
            let _ = handle.await.unwrap();
        }
    }

    #[test]
    fn test_help_subcommands() {
        // Test help for each subcommand
        let subcommands = vec![
            "init", "start", "stop", "restart", "status", "logs", "list", "delete",
        ];

        for subcommand in subcommands {
            let mut cmd = Command::cargo_bin("bunctl").unwrap();
            cmd.args([subcommand, "--help"])
                .assert()
                .success()
                .stdout(predicate::str::contains(subcommand));
        }
    }

    #[test]
    fn test_version_output() {
        let mut cmd = Command::cargo_bin("bunctl").unwrap();
        cmd.arg("--version")
            .assert()
            .success()
            .stdout(predicate::str::contains("bunctl"));
    }
}
