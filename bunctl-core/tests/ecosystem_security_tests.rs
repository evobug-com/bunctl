use bunctl_core::config::ecosystem::EcosystemApp;

#[cfg(test)]
mod ecosystem_security_tests {
    use super::*;

    #[test]
    fn test_ecosystem_no_command_injection_in_script() {
        // Test that script paths with shell metacharacters are treated literally
        let app = EcosystemApp {
            name: "test-app".to_string(),
            script: "script.js ; echo INJECTED".to_string(),
            interpreter: Some("node".to_string()),
            args: None,
            cwd: None,
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
            merge_logs: None,
            log_date_format: None,
            max_restarts: None,
            autorestart: None,
            restart_delay: None,
            min_uptime: None,
            kill_timeout: None,
            wait_ready: None,
            listen_timeout: None,
        };

        let config = app.to_app_config();

        // Command should be just the interpreter
        assert_eq!(config.command, "node");
        // Script with injection attempt should be first argument
        assert_eq!(config.args[0], "script.js ; echo INJECTED");
    }

    #[test]
    fn test_ecosystem_args_no_shell_parsing() {
        // Test that args are split by whitespace only, no shell parsing
        let app = EcosystemApp {
            name: "test-app".to_string(),
            script: "script.js".to_string(),
            interpreter: Some("node".to_string()),
            args: Some("--max-old-space-size=4096 ; echo INJECTED".to_string()),
            cwd: None,
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
            merge_logs: None,
            log_date_format: None,
            max_restarts: None,
            autorestart: None,
            restart_delay: None,
            min_uptime: None,
            kill_timeout: None,
            wait_ready: None,
            listen_timeout: None,
        };

        let config = app.to_app_config();

        assert_eq!(config.command, "node");
        assert_eq!(config.args[0], "script.js");
        // Args should be split by whitespace, semicolon treated literally
        assert!(
            config
                .args
                .contains(&"--max-old-space-size=4096".to_string())
        );
        assert!(config.args.contains(&";".to_string()));
        assert!(config.args.contains(&"echo".to_string()));
    }

    #[test]
    fn test_ecosystem_no_command_substitution() {
        // Test that command substitution syntax is treated literally
        let app = EcosystemApp {
            name: "test-app".to_string(),
            script: "$(whoami).js".to_string(),
            interpreter: Some("node".to_string()),
            args: Some("`id` ${USER}".to_string()),
            cwd: None,
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
            merge_logs: None,
            log_date_format: None,
            max_restarts: None,
            autorestart: None,
            restart_delay: None,
            min_uptime: None,
            kill_timeout: None,
            wait_ready: None,
            listen_timeout: None,
        };

        let config = app.to_app_config();

        assert_eq!(config.command, "node");
        assert_eq!(config.args[0], "$(whoami).js");
        assert!(config.args.contains(&"`id`".to_string()));
        assert!(config.args.contains(&"${USER}".to_string()));
    }

    #[test]
    fn test_ecosystem_pipe_treated_literally() {
        // Test that pipe characters are treated as literal strings
        let app = EcosystemApp {
            name: "test-app".to_string(),
            script: "script.js".to_string(),
            interpreter: Some("node".to_string()),
            args: Some("arg1 | tee /tmp/output".to_string()),
            cwd: None,
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
            merge_logs: None,
            log_date_format: None,
            max_restarts: None,
            autorestart: None,
            restart_delay: None,
            min_uptime: None,
            kill_timeout: None,
            wait_ready: None,
            listen_timeout: None,
        };

        let config = app.to_app_config();

        assert_eq!(config.command, "node");
        assert!(config.args.contains(&"|".to_string()));
        assert!(config.args.contains(&"tee".to_string()));
    }

    #[test]
    fn test_ecosystem_no_interpreter_injection() {
        // Test that interpreter field doesn't allow injection
        let app = EcosystemApp {
            name: "test-app".to_string(),
            script: "script.js".to_string(),
            interpreter: Some("node ; malicious_command".to_string()),
            args: None,
            cwd: None,
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
            merge_logs: None,
            log_date_format: None,
            max_restarts: None,
            autorestart: None,
            restart_delay: None,
            min_uptime: None,
            kill_timeout: None,
            wait_ready: None,
            listen_timeout: None,
        };

        let config = app.to_app_config();

        // The entire interpreter string should be the command
        assert_eq!(config.command, "node ; malicious_command");
        assert_eq!(config.args[0], "script.js");
    }

    #[test]
    fn test_ecosystem_none_interpreter() {
        // Test that "none" interpreter works correctly
        let app = EcosystemApp {
            name: "test-app".to_string(),
            script: "./binary".to_string(),
            interpreter: Some("none".to_string()),
            args: Some("--flag value".to_string()),
            cwd: None,
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
            merge_logs: None,
            log_date_format: None,
            max_restarts: None,
            autorestart: None,
            restart_delay: None,
            min_uptime: None,
            kill_timeout: None,
            wait_ready: None,
            listen_timeout: None,
        };

        let config = app.to_app_config();

        // With "none", script becomes the command
        assert_eq!(config.command, "./binary");
        assert_eq!(config.args[0], "--flag");
        assert_eq!(config.args[1], "value");
    }

    #[test]
    fn test_ecosystem_whitespace_splitting() {
        // Test that args are split by whitespace (newlines become separate args)
        let app = EcosystemApp {
            name: "test-app".to_string(),
            script: "script.js".to_string(),
            interpreter: Some("node".to_string()),
            args: Some("arg1 arg2 arg3".to_string()),
            cwd: None,
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
            merge_logs: None,
            log_date_format: None,
            max_restarts: None,
            autorestart: None,
            restart_delay: None,
            min_uptime: None,
            kill_timeout: None,
            wait_ready: None,
            listen_timeout: None,
        };

        let config = app.to_app_config();

        // Args should be split into separate elements
        assert_eq!(config.args[0], "script.js");
        assert_eq!(config.args[1], "arg1");
        assert_eq!(config.args[2], "arg2");
        assert_eq!(config.args[3], "arg3");
    }

    #[test]
    fn test_ecosystem_quotes_not_parsed() {
        // Test that quotes don't affect parsing
        let app = EcosystemApp {
            name: "test-app".to_string(),
            script: "script.js".to_string(),
            interpreter: Some("node".to_string()),
            args: Some(r#""arg with spaces" 'another arg' `backticks`"#.to_string()),
            cwd: None,
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
            merge_logs: None,
            log_date_format: None,
            max_restarts: None,
            autorestart: None,
            restart_delay: None,
            min_uptime: None,
            kill_timeout: None,
            wait_ready: None,
            listen_timeout: None,
        };

        let config = app.to_app_config();

        // Quotes should be preserved as literals
        assert!(config.args.iter().any(|arg| arg.contains('"')));
        assert!(config.args.iter().any(|arg| arg.contains('\'')));
        assert!(config.args.iter().any(|arg| arg.contains('`')));
    }
}
