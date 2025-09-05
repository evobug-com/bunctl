#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{IpcClient, IpcConnection, IpcServer};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{IpcClient, IpcConnection, IpcServer};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SubscriptionType {
    StatusEvents { app_name: Option<String> },
    LogEvents { app_name: Option<String> },
    AllEvents { app_name: Option<String> },
}

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
