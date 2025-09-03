use crate::{IpcMessage, IpcResponse};
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions};
use tracing::{debug, error, trace};

pub struct IpcServer {
    pipe_name: String,
    server: Option<NamedPipeServer>,
}

impl IpcServer {
    pub async fn bind(path: impl AsRef<Path>) -> bunctl_core::Result<Self> {
        let pipe_name = format!(
            r"\\.\pipe\bunctl_{}",
            path.as_ref()
                .file_name()
                .unwrap_or(std::ffi::OsStr::new("default"))
                .to_string_lossy()
        );
        
        debug!("Creating IPC server on named pipe: {}", pipe_name);
        
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(&pipe_name)
            .map_err(|e| {
                error!("Failed to create named pipe {}: {}", pipe_name, e);
                bunctl_core::Error::Io(e)
            })?;
            
        debug!("IPC server created successfully on: {}", pipe_name);
        Ok(Self {
            pipe_name,
            server: Some(server),
        })
    }

    pub async fn accept(&mut self) -> bunctl_core::Result<IpcConnection> {
        if let Some(server) = self.server.take() {
            debug!("IPC server waiting for client connection on: {}", self.pipe_name);
            server.connect().await.map_err(|e| {
                error!("Failed to accept connection on {}: {}", self.pipe_name, e);
                bunctl_core::Error::Io(e)
            })?;
            debug!("IPC client connected on: {}", self.pipe_name);
            
            // Create next server instance for next connection
            debug!("Creating next server instance for: {}", self.pipe_name);
            let next_server = ServerOptions::new()
                .create(&self.pipe_name)
                .map_err(|e| {
                    error!("Failed to create next server instance for {}: {}", self.pipe_name, e);
                    bunctl_core::Error::Io(e)
                })?;
                
            let connection = IpcConnection::Server(server);
            self.server = Some(next_server);
            debug!("Next server instance ready for: {}", self.pipe_name);
            
            Ok(connection)
        } else {
            error!("IPC server not initialized");
            Err(bunctl_core::Error::Other(anyhow::anyhow!("Server not initialized")))
        }
    }
}

pub enum IpcConnection {
    Server(NamedPipeServer),
    Client(NamedPipeClient),
}

impl IpcConnection {
    pub async fn recv(&mut self) -> bunctl_core::Result<IpcMessage> {
        trace!("IPC connection waiting to receive message");
        let mut len_bytes = [0u8; 4];
        match self {
            IpcConnection::Server(pipe) => pipe.read_exact(&mut len_bytes).await,
            IpcConnection::Client(pipe) => pipe.read_exact(&mut len_bytes).await,
        }.map_err(|e| {
            error!("Failed to read message length from IPC: {}", e);
            bunctl_core::Error::Io(e)
        })?;
        
        let len = u32::from_le_bytes(len_bytes) as usize;
        trace!("IPC message length: {} bytes", len);
        let mut buffer = vec![0u8; len];
        match self {
            IpcConnection::Server(pipe) => pipe.read_exact(&mut buffer).await,
            IpcConnection::Client(pipe) => pipe.read_exact(&mut buffer).await,
        }.map_err(|e| {
            error!("Failed to read message body from IPC: {}", e);
            bunctl_core::Error::Io(e)
        })?;
        
        let message = serde_json::from_slice(&buffer)
            .map_err(|e| {
                error!("Failed to deserialize IPC message: {}", e);
                bunctl_core::Error::Other(e.into())
            })?;
        
        debug!("IPC received message: {:?}", message);
        Ok(message)
    }
    
    pub async fn send(&mut self, response: &IpcResponse) -> bunctl_core::Result<()> {
        debug!("IPC sending response: {:?}", response);
        let data = serde_json::to_vec(response)
            .map_err(|e| {
                error!("Failed to serialize IPC response: {}", e);
                bunctl_core::Error::Other(e.into())
            })?;
        
        trace!("IPC response serialized to {} bytes", data.len());
        let len = (data.len() as u32).to_le_bytes();
        match self {
            IpcConnection::Server(pipe) => {
                pipe.write_all(&len).await
                    .map_err(|e| {
                        error!("Failed to write response length to IPC: {}", e);
                        bunctl_core::Error::Io(e)
                    })?;
                pipe.write_all(&data).await
                    .map_err(|e| {
                        error!("Failed to write response data to IPC: {}", e);
                        bunctl_core::Error::Io(e)
                    })?;
                pipe.flush().await
                    .map_err(|e| {
                        error!("Failed to flush IPC response: {}", e);
                        bunctl_core::Error::Io(e)
                    })?;
            }
            IpcConnection::Client(pipe) => {
                pipe.write_all(&len).await
                    .map_err(|e| {
                        error!("Failed to write response length to IPC: {}", e);
                        bunctl_core::Error::Io(e)
                    })?;
                pipe.write_all(&data).await
                    .map_err(|e| {
                        error!("Failed to write response data to IPC: {}", e);
                        bunctl_core::Error::Io(e)
                    })?;
                pipe.flush().await
                    .map_err(|e| {
                        error!("Failed to flush IPC response: {}", e);
                        bunctl_core::Error::Io(e)
                    })?;
            }
        }
        
        trace!("IPC response sent successfully");
        Ok(())
    }
}

pub struct IpcClient {
    pipe: NamedPipeClient,
}

impl IpcClient {
    pub async fn connect(path: impl AsRef<Path>) -> bunctl_core::Result<Self> {
        let pipe_name = format!(
            r"\\.\pipe\bunctl_{}",
            path.as_ref()
                .file_name()
                .unwrap_or(std::ffi::OsStr::new("default"))
                .to_string_lossy()
        );
        
        let pipe = ClientOptions::new()
            .open(&pipe_name)
            .map_err(bunctl_core::Error::Io)?;
            
        Ok(Self { pipe })
    }

    pub async fn send(&mut self, msg: &IpcMessage) -> bunctl_core::Result<()> {
        let data = serde_json::to_vec(msg)
            .map_err(|e| bunctl_core::Error::Other(e.into()))?;
        
        let len = (data.len() as u32).to_le_bytes();
        self.pipe.write_all(&len).await
            .map_err(bunctl_core::Error::Io)?;
        self.pipe.write_all(&data).await
            .map_err(bunctl_core::Error::Io)?;
        self.pipe.flush().await
            .map_err(bunctl_core::Error::Io)?;
        
        Ok(())
    }

    pub async fn recv(&mut self) -> bunctl_core::Result<IpcResponse> {
        let mut len_bytes = [0u8; 4];
        self.pipe.read_exact(&mut len_bytes).await
            .map_err(bunctl_core::Error::Io)?;
        
        let len = u32::from_le_bytes(len_bytes) as usize;
        let mut buffer = vec![0u8; len];
        self.pipe.read_exact(&mut buffer).await
            .map_err(bunctl_core::Error::Io)?;
        
        serde_json::from_slice(&buffer)
            .map_err(|e| bunctl_core::Error::Other(e.into()))
    }
}
