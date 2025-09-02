#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{IpcClient, IpcServer};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{IpcClient, IpcServer};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcMessage {
    Start { name: String, config: String },
    Stop { name: String },
    Restart { name: String },
    Status { name: Option<String> },
    List,
    Delete { name: String },
    Logs { name: String, lines: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcResponse {
    Success { message: String },
    Error { message: String },
    Data { data: serde_json::Value },
}