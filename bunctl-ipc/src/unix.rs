use crate::{IpcMessage, IpcResponse};
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

pub struct IpcServer {
    listener: UnixListener,
}

impl IpcServer {
    pub async fn bind(path: impl AsRef<Path>) -> bunctl_core::Result<Self> {
        let path = path.as_ref();
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path).map_err(bunctl_core::Error::Io)?;
        Ok(Self { listener })
    }

    pub async fn accept(&mut self) -> bunctl_core::Result<IpcConnection> {
        let (stream, _addr) = self
            .listener
            .accept()
            .await
            .map_err(bunctl_core::Error::Io)?;
        Ok(IpcConnection { stream })
    }
}

pub struct IpcConnection {
    stream: UnixStream,
}

impl IpcConnection {
    pub async fn recv(&mut self) -> bunctl_core::Result<IpcMessage> {
        let mut len_bytes = [0u8; 4];
        self.stream
            .read_exact(&mut len_bytes)
            .await
            .map_err(bunctl_core::Error::Io)?;

        let len = u32::from_le_bytes(len_bytes) as usize;
        let mut buffer = vec![0u8; len];
        self.stream
            .read_exact(&mut buffer)
            .await
            .map_err(bunctl_core::Error::Io)?;

        serde_json::from_slice(&buffer).map_err(|e| bunctl_core::Error::Other(e.into()))
    }

    pub async fn send(&mut self, response: &IpcResponse) -> bunctl_core::Result<()> {
        let data = serde_json::to_vec(response).map_err(|e| bunctl_core::Error::Other(e.into()))?;

        let len = (data.len() as u32).to_le_bytes();
        self.stream
            .write_all(&len)
            .await
            .map_err(bunctl_core::Error::Io)?;
        self.stream
            .write_all(&data)
            .await
            .map_err(bunctl_core::Error::Io)?;
        self.stream.flush().await.map_err(bunctl_core::Error::Io)?;

        Ok(())
    }
}

pub struct IpcClient {
    stream: UnixStream,
}

impl IpcClient {
    pub async fn connect(path: impl AsRef<Path>) -> bunctl_core::Result<Self> {
        let stream = UnixStream::connect(path)
            .await
            .map_err(bunctl_core::Error::Io)?;
        Ok(Self { stream })
    }

    pub async fn send(&mut self, msg: &IpcMessage) -> bunctl_core::Result<()> {
        let data = serde_json::to_vec(msg).map_err(|e| bunctl_core::Error::Other(e.into()))?;

        let len = (data.len() as u32).to_le_bytes();
        self.stream
            .write_all(&len)
            .await
            .map_err(bunctl_core::Error::Io)?;
        self.stream
            .write_all(&data)
            .await
            .map_err(bunctl_core::Error::Io)?;
        self.stream.flush().await.map_err(bunctl_core::Error::Io)?;

        Ok(())
    }

    pub async fn recv(&mut self) -> bunctl_core::Result<IpcResponse> {
        let mut len_bytes = [0u8; 4];
        self.stream
            .read_exact(&mut len_bytes)
            .await
            .map_err(bunctl_core::Error::Io)?;

        let len = u32::from_le_bytes(len_bytes) as usize;
        let mut buffer = vec![0u8; len];
        self.stream
            .read_exact(&mut buffer)
            .await
            .map_err(bunctl_core::Error::Io)?;

        serde_json::from_slice(&buffer).map_err(|e| bunctl_core::Error::Other(e.into()))
    }
}
