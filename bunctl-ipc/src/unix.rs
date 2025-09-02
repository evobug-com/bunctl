use crate::{IpcMessage, IpcResponse};
use std::path::Path;
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct IpcServer {
    listener: UnixListener,
}

impl IpcServer {
    pub async fn bind(path: impl AsRef<Path>) -> bunctl_core::Result<Self> {
        let path = path.as_ref();
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path)?;
        Ok(Self { listener })
    }
    
    pub async fn accept(&self) -> bunctl_core::Result<(UnixStream, std::net::SocketAddr)> {
        let (stream, addr) = self.listener.accept().await?;
        Ok((stream, addr.into()))
    }
}

pub struct IpcClient;

impl IpcClient {
    pub async fn connect(path: impl AsRef<Path>) -> bunctl_core::Result<UnixStream> {
        Ok(UnixStream::connect(path).await?)
    }
    
    pub async fn send(stream: &mut UnixStream, msg: &IpcMessage) -> bunctl_core::Result<()> {
        let data = serde_json::to_vec(msg)?;
        stream.write_u32(data.len() as u32).await?;
        stream.write_all(&data).await?;
        stream.flush().await?;
        Ok(())
    }
    
    pub async fn recv(stream: &mut UnixStream) -> bunctl_core::Result<IpcResponse> {
        let len = stream.read_u32().await?;
        let mut buf = vec![0u8; len as usize];
        stream.read_exact(&mut buf).await?;
        Ok(serde_json::from_slice(&buf)?)
    }
}