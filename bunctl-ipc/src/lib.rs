//! Inter-process communication library for bunctl
//!
//! This crate provides platform-specific IPC implementations for communication
//! between the bunctl CLI and daemon process. It uses Unix domain sockets on Linux
//! and named pipes on Windows.

#[cfg(target_os = "linux")]
mod unix;
#[cfg(target_os = "linux")]
pub use unix::{IpcClient, IpcConnection, IpcServer};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{IpcClient, IpcConnection, IpcServer};

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Maximum allowed message size (10MB)
pub const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024;

/// Default timeout for IPC operations
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Types of event subscriptions available
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SubscriptionType {
    StatusEvents { app_name: Option<String> },
    LogEvents { app_name: Option<String> },
    AllEvents { app_name: Option<String> },
}

/// Messages that can be sent from client to server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcMessage {
    Start { name: String, config: String },
    Stop { name: String },
    Restart { name: String },
    Status { name: Option<String> },
    List,
    Delete { name: String },
    Logs { name: Option<String>, lines: usize },
    Subscribe { subscription: SubscriptionType },
    Unsubscribe,
}

/// Responses sent from server to client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcResponse {
    Success {
        message: String,
    },
    Error {
        message: String,
    },
    Data {
        data: serde_json::Value,
    },
    Event {
        event_type: String,
        data: serde_json::Value,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_subscription_type_serialization() {
        // Test StatusEvents serialization
        let status_sub = SubscriptionType::StatusEvents {
            app_name: Some("test-app".to_string()),
        };
        let serialized = serde_json::to_string(&status_sub).unwrap();
        let deserialized: SubscriptionType = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            SubscriptionType::StatusEvents { app_name } => {
                assert_eq!(app_name, Some("test-app".to_string()));
            }
            _ => panic!("Wrong subscription type"),
        }

        // Test LogEvents serialization
        let log_sub = SubscriptionType::LogEvents { app_name: None };
        let serialized = serde_json::to_string(&log_sub).unwrap();
        let deserialized: SubscriptionType = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            SubscriptionType::LogEvents { app_name } => {
                assert_eq!(app_name, None);
            }
            _ => panic!("Wrong subscription type"),
        }

        // Test AllEvents serialization
        let all_sub = SubscriptionType::AllEvents {
            app_name: Some("another-app".to_string()),
        };
        let serialized = serde_json::to_string(&all_sub).unwrap();
        let deserialized: SubscriptionType = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            SubscriptionType::AllEvents { app_name } => {
                assert_eq!(app_name, Some("another-app".to_string()));
            }
            _ => panic!("Wrong subscription type"),
        }
    }

    #[test]
    fn test_ipc_message_serialization() {
        // Test Start message
        let start_msg = IpcMessage::Start {
            name: "app1".to_string(),
            config: "{\"script\": \"index.js\"}".to_string(),
        };
        let serialized = serde_json::to_string(&start_msg).unwrap();
        let deserialized: IpcMessage = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            IpcMessage::Start { name, config } => {
                assert_eq!(name, "app1");
                assert_eq!(config, "{\"script\": \"index.js\"}");
            }
            _ => panic!("Wrong message type"),
        }

        // Test Stop message
        let stop_msg = IpcMessage::Stop {
            name: "app1".to_string(),
        };
        let serialized = serde_json::to_string(&stop_msg).unwrap();
        let deserialized: IpcMessage = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            IpcMessage::Stop { name } => {
                assert_eq!(name, "app1");
            }
            _ => panic!("Wrong message type"),
        }

        // Test Restart message
        let restart_msg = IpcMessage::Restart {
            name: "app1".to_string(),
        };
        let serialized = serde_json::to_string(&restart_msg).unwrap();
        let deserialized: IpcMessage = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            IpcMessage::Restart { name } => {
                assert_eq!(name, "app1");
            }
            _ => panic!("Wrong message type"),
        }

        // Test Status message
        let status_msg = IpcMessage::Status {
            name: Some("app1".to_string()),
        };
        let serialized = serde_json::to_string(&status_msg).unwrap();
        let deserialized: IpcMessage = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            IpcMessage::Status { name } => {
                assert_eq!(name, Some("app1".to_string()));
            }
            _ => panic!("Wrong message type"),
        }

        // Test List message
        let list_msg = IpcMessage::List;
        let serialized = serde_json::to_string(&list_msg).unwrap();
        let deserialized: IpcMessage = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            IpcMessage::List => {}
            _ => panic!("Wrong message type"),
        }

        // Test Delete message
        let delete_msg = IpcMessage::Delete {
            name: "app1".to_string(),
        };
        let serialized = serde_json::to_string(&delete_msg).unwrap();
        let deserialized: IpcMessage = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            IpcMessage::Delete { name } => {
                assert_eq!(name, "app1");
            }
            _ => panic!("Wrong message type"),
        }

        // Test Logs message
        let logs_msg = IpcMessage::Logs {
            name: None,
            lines: 100,
        };
        let serialized = serde_json::to_string(&logs_msg).unwrap();
        let deserialized: IpcMessage = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            IpcMessage::Logs { name, lines } => {
                assert_eq!(name, None);
                assert_eq!(lines, 100);
            }
            _ => panic!("Wrong message type"),
        }

        // Test Subscribe message
        let subscribe_msg = IpcMessage::Subscribe {
            subscription: SubscriptionType::AllEvents { app_name: None },
        };
        let serialized = serde_json::to_string(&subscribe_msg).unwrap();
        let deserialized: IpcMessage = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            IpcMessage::Subscribe { subscription } => match subscription {
                SubscriptionType::AllEvents { app_name } => {
                    assert_eq!(app_name, None);
                }
                _ => panic!("Wrong subscription type"),
            },
            _ => panic!("Wrong message type"),
        }

        // Test Unsubscribe message
        let unsubscribe_msg = IpcMessage::Unsubscribe;
        let serialized = serde_json::to_string(&unsubscribe_msg).unwrap();
        let deserialized: IpcMessage = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            IpcMessage::Unsubscribe => {}
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_ipc_response_serialization() {
        // Test Success response
        let success_resp = IpcResponse::Success {
            message: "Operation completed".to_string(),
        };
        let serialized = serde_json::to_string(&success_resp).unwrap();
        let deserialized: IpcResponse = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            IpcResponse::Success { message } => {
                assert_eq!(message, "Operation completed");
            }
            _ => panic!("Wrong response type"),
        }

        // Test Error response
        let error_resp = IpcResponse::Error {
            message: "Operation failed".to_string(),
        };
        let serialized = serde_json::to_string(&error_resp).unwrap();
        let deserialized: IpcResponse = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            IpcResponse::Error { message } => {
                assert_eq!(message, "Operation failed");
            }
            _ => panic!("Wrong response type"),
        }

        // Test Data response
        let data_resp = IpcResponse::Data {
            data: json!({"status": "running", "pid": 1234}),
        };
        let serialized = serde_json::to_string(&data_resp).unwrap();
        let deserialized: IpcResponse = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            IpcResponse::Data { data } => {
                assert_eq!(data["status"], "running");
                assert_eq!(data["pid"], 1234);
            }
            _ => panic!("Wrong response type"),
        }

        // Test Event response
        let event_resp = IpcResponse::Event {
            event_type: "app-started".to_string(),
            data: json!({"name": "app1", "pid": 5678}),
        };
        let serialized = serde_json::to_string(&event_resp).unwrap();
        let deserialized: IpcResponse = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            IpcResponse::Event { event_type, data } => {
                assert_eq!(event_type, "app-started");
                assert_eq!(data["name"], "app1");
                assert_eq!(data["pid"], 5678);
            }
            _ => panic!("Wrong response type"),
        }
    }

    #[test]
    fn test_message_with_special_characters() {
        // Test with unicode and special characters
        let msg = IpcMessage::Start {
            name: "app-😀-test".to_string(),
            config:
                r#"{"script": "C:\\Path\\To\\Script.js", "env": {"KEY": "value with \"quotes\""}}"#
                    .to_string(),
        };
        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: IpcMessage = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            IpcMessage::Start { name, config } => {
                assert_eq!(name, "app-😀-test");
                assert!(config.contains("C:\\\\Path\\\\To\\\\Script.js"));
                assert!(config.contains("value with \\\"quotes\\\""));
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_large_payload_serialization() {
        // Test with large config string
        let large_config = "x".repeat(100_000);
        let msg = IpcMessage::Start {
            name: "large-app".to_string(),
            config: large_config.clone(),
        };
        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: IpcMessage = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            IpcMessage::Start { name, config } => {
                assert_eq!(name, "large-app");
                assert_eq!(config.len(), 100_000);
                assert_eq!(config, large_config);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_empty_strings_and_nulls() {
        // Test with empty strings
        let msg = IpcMessage::Start {
            name: "".to_string(),
            config: "".to_string(),
        };
        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: IpcMessage = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            IpcMessage::Start { name, config } => {
                assert_eq!(name, "");
                assert_eq!(config, "");
            }
            _ => panic!("Wrong message type"),
        }

        // Test with None values
        let status_msg = IpcMessage::Status { name: None };
        let serialized = serde_json::to_string(&status_msg).unwrap();
        let deserialized: IpcMessage = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            IpcMessage::Status { name } => {
                assert_eq!(name, None);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_complex_json_data() {
        // Test with nested JSON structures
        let complex_data = json!({
            "apps": [
                {"name": "app1", "status": "running", "cpu": 23.5},
                {"name": "app2", "status": "stopped", "cpu": 0.0}
            ],
            "metadata": {
                "version": "1.0.0",
                "timestamp": 1234567890,
                "nested": {
                    "deeply": {
                        "nested": {
                            "value": "test"
                        }
                    }
                }
            }
        });

        let resp = IpcResponse::Data {
            data: complex_data.clone(),
        };
        let serialized = serde_json::to_string(&resp).unwrap();
        let deserialized: IpcResponse = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            IpcResponse::Data { data } => {
                assert_eq!(data["apps"][0]["name"], "app1");
                assert_eq!(data["apps"][0]["cpu"], 23.5);
                assert_eq!(
                    data["metadata"]["nested"]["deeply"]["nested"]["value"],
                    "test"
                );
            }
            _ => panic!("Wrong response type"),
        }
    }
}
