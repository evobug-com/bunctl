use crate::{IpcMessage, IpcResponse};
use std::path::Path;

pub struct IpcServer {
    _pipe_name: String,
}

impl IpcServer {
    pub async fn bind(path: impl AsRef<Path>) -> bunctl_core::Result<Self> {
        let pipe_name = format!(
            r"\\.\pipe\bunctl_{}",
            path.as_ref().file_name().unwrap().to_string_lossy()
        );
        Ok(Self {
            _pipe_name: pipe_name,
        })
    }

    pub async fn accept(&self) -> bunctl_core::Result<()> {
        Ok(())
    }
}

pub struct IpcClient;

impl IpcClient {
    pub async fn connect(_path: impl AsRef<Path>) -> bunctl_core::Result<()> {
        Ok(())
    }

    pub async fn send(_msg: &IpcMessage) -> bunctl_core::Result<()> {
        Ok(())
    }

    pub async fn recv() -> bunctl_core::Result<IpcResponse> {
        Ok(IpcResponse::Success {
            message: "Not implemented on Windows".to_string(),
        })
    }
}
