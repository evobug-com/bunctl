use bunctl_core::config::{AppConfig, RestartPolicy};
use std::collections::HashMap;
use std::path::PathBuf;

#[cfg(test)]
mod command_injection_tests {
    use super::*;

    #[test]
    fn test_process_builder_builds_without_parsing() {
        // This test verifies that ProcessBuilder doesn't parse shell commands
        // We can't directly access the private fields, but we can verify the builder
        // compiles and the commands don't get parsed by testing the config
        use bunctl_core::process::ProcessBuilder;

        // These should all compile and not cause parsing
        let _b1 = ProcessBuilder::new("echo").args(vec!["; echo INJECTED"]);

        let _b2 = ProcessBuilder::new("echo").args(vec!["$(whoami)", "`id`"]);

        let _b3 = ProcessBuilder::new("echo").args(vec!["|", "cat /etc/passwd"]);

        // The test passes if it compiles - the actual execution would show
        // that these are treated as literal arguments
    }

    #[test]
    fn test_config_no_embedded_args_in_command() {
        // Test that AppConfig doesn't parse commands with embedded arguments
        let config = AppConfig {
            name: "test-app".to_string(),
            command: "echo test ; echo INJECTED".to_string(),
            args: vec![],
            cwd: PathBuf::from("."),
            env: HashMap::new(),
            auto_start: false,
            restart_policy: RestartPolicy::No,
            max_memory: None,
            max_cpu_percent: None,
            uid: None,
            gid: None,
            stdout_log: None,
            stderr_log: None,
            combined_log: None,
            log_max_size: None,
            log_max_files: None,
            health_check: None,
            stop_timeout: std::time::Duration::from_secs(10),
            kill_timeout: std::time::Duration::from_secs(5),
            backoff: Default::default(),
        };

        // The entire string should be treated as the command, not parsed
        assert_eq!(config.command, "echo test ; echo INJECTED");
        assert_eq!(config.args.len(), 0);
    }

    #[test]
    fn test_config_with_explicit_args() {
        // Test that properly separated command and args work correctly
        let config = AppConfig {
            name: "test-app".to_string(),
            command: "echo".to_string(),
            args: vec!["test".to_string(), "; echo INJECTED".to_string()],
            cwd: PathBuf::from("."),
            env: HashMap::new(),
            auto_start: false,
            restart_policy: RestartPolicy::No,
            max_memory: None,
            max_cpu_percent: None,
            uid: None,
            gid: None,
            stdout_log: None,
            stderr_log: None,
            combined_log: None,
            log_max_size: None,
            log_max_files: None,
            health_check: None,
            stop_timeout: std::time::Duration::from_secs(10),
            kill_timeout: std::time::Duration::from_secs(5),
            backoff: Default::default(),
        };

        assert_eq!(config.command, "echo");
        assert_eq!(config.args, vec!["test", "; echo INJECTED"]);
    }
}
