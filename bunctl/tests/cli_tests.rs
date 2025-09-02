use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;
use std::fs;

#[test]
fn test_cli_version() {
    let mut cmd = Command::cargo_bin("bunctl").unwrap();
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("bunctl"));
}

#[test]
fn test_cli_help() {
    let mut cmd = Command::cargo_bin("bunctl").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Production-grade process manager"));
}

#[test]
fn test_init_command_help() {
    let mut cmd = Command::cargo_bin("bunctl").unwrap();
    cmd.args(&["init", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialize a new application"));
}

#[test]
fn test_init_basic() {
    let temp_dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("bunctl").unwrap();
    
    cmd.current_dir(&temp_dir)
        .args(&["init", "--name", "test-app"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized app 'test-app'"));
    
    // Check that bunctl.json was created
    assert!(temp_dir.path().join("bunctl.json").exists());
    
    let content = fs::read_to_string(temp_dir.path().join("bunctl.json")).unwrap();
    assert!(content.contains("test-app"));
}

#[test]
fn test_init_ecosystem_format() {
    let temp_dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("bunctl").unwrap();
    
    cmd.current_dir(&temp_dir)
        .args(&["init", "--name", "eco-app", "--ecosystem"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ecosystem.config.js"));
    
    // Check that ecosystem.config.js was created
    assert!(temp_dir.path().join("ecosystem.config.js").exists());
    
    let content = fs::read_to_string(temp_dir.path().join("ecosystem.config.js")).unwrap();
    assert!(content.contains("eco-app"));
    assert!(content.contains("module.exports"));
}

#[test]
fn test_init_with_custom_settings() {
    let temp_dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("bunctl").unwrap();
    
    // Create a dummy entry file
    fs::write(temp_dir.path().join("server.ts"), "console.log('test')").unwrap();
    
    cmd.current_dir(&temp_dir)
        .args(&[
            "init",
            "--name", "custom-app",
            "--entry", "server.ts",
            "--port", "3000",
            "--memory", "1G",
            "--cpu", "75",
            "--runtime", "bun",
            "--autostart"
        ])
        .assert()
        .success();
    
    let config_path = temp_dir.path().join("bunctl.json");
    assert!(config_path.exists());
    
    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("custom-app"));
    assert!(content.contains("server.ts"));
    assert!(content.contains("3000"));
}

#[test]
fn test_start_without_config() {
    let mut cmd = Command::cargo_bin("bunctl").unwrap();
    
    cmd.args(&["start", "nonexistent"])
        .assert()
        .failure();
}

#[test]
fn test_list_command() {
    let mut cmd = Command::cargo_bin("bunctl").unwrap();
    
    cmd.arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("No apps"));
}

#[test]
fn test_status_command() {
    let mut cmd = Command::cargo_bin("bunctl").unwrap();
    
    cmd.arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("No apps"));
}

#[test]
fn test_status_json() {
    let mut cmd = Command::cargo_bin("bunctl").unwrap();
    
    cmd.args(&["status", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("{}"));
}

#[test]
fn test_invalid_command() {
    let mut cmd = Command::cargo_bin("bunctl").unwrap();
    
    cmd.arg("invalid-command")
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn test_delete_nonexistent() {
    let mut cmd = Command::cargo_bin("bunctl").unwrap();
    
    cmd.args(&["delete", "nonexistent"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Deleted app nonexistent"));
}

#[test]
fn test_logs_command() {
    let mut cmd = Command::cargo_bin("bunctl").unwrap();
    
    cmd.args(&["logs", "test-app"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Showing logs for test-app"));
}

#[test]
fn test_restart_command() {
    let mut cmd = Command::cargo_bin("bunctl").unwrap();
    
    cmd.args(&["restart", "test-app"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Restarted app test-app"));
}

#[test]
fn test_stop_command() {
    let mut cmd = Command::cargo_bin("bunctl").unwrap();
    
    cmd.args(&["stop", "test-app"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Stopped app test-app"));
}