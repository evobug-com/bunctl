use crate::{DEFAULT_TIMEOUT, IpcMessage, IpcResponse, MAX_MESSAGE_SIZE};
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use tokio::time::timeout;
use tracing::{debug, error, trace};

/// Windows named pipe server for IPC communication
pub struct IpcServer {
    pipe_name: String,
    server: Option<NamedPipeServer>,
}

impl IpcServer {
    /// Create a new IPC server bound to the specified path
    pub async fn bind(path: impl AsRef<Path>) -> bunctl_core::Result<Self> {
        let pipe_name = format!(
            r"\\.\pipe\bunctl_{}",
            path.as_ref()
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("default"))
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

    /// Accept a new client connection
    pub async fn accept(&mut self) -> bunctl_core::Result<IpcConnection> {
        if let Some(server) = self.server.take() {
            debug!(
                "IPC server waiting for client connection on: {}",
                self.pipe_name
            );
            server.connect().await.map_err(|e| {
                error!("Failed to accept connection on {}: {}", self.pipe_name, e);
                bunctl_core::Error::Io(e)
            })?;
            debug!("IPC client connected on: {}", self.pipe_name);

            // Create next server instance for next connection
            debug!("Creating next server instance for: {}", self.pipe_name);
            let next_server = ServerOptions::new().create(&self.pipe_name).map_err(|e| {
                error!(
                    "Failed to create next server instance for {}: {}",
                    self.pipe_name, e
                );
                bunctl_core::Error::Io(e)
            })?;

            let connection = IpcConnection::from_server(server);
            self.server = Some(next_server);
            debug!("Next server instance ready for: {}", self.pipe_name);

            Ok(connection)
        } else {
            error!("IPC server not initialized");
            Err(bunctl_core::Error::Other(anyhow::anyhow!(
                "Server not initialized"
            )))
        }
    }
}

/// Represents an active IPC connection
pub struct IpcConnection {
    inner: ConnectionInner,
    timeout: Duration,
}

enum ConnectionInner {
    Server(NamedPipeServer),
    #[allow(dead_code)]
    Client(NamedPipeClient),
}

impl IpcConnection {
    fn from_server(server: NamedPipeServer) -> Self {
        Self {
            inner: ConnectionInner::Server(server),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    #[allow(dead_code)]
    fn from_client(client: NamedPipeClient) -> Self {
        Self {
            inner: ConnectionInner::Client(client),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Set the timeout for operations on this connection
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Receive a message from the connection
    pub async fn recv(&mut self) -> bunctl_core::Result<IpcMessage> {
        trace!("IPC connection waiting to receive message");

        // Read message length with timeout
        let mut len_bytes = [0u8; 4];
        let read_result = match &mut self.inner {
            ConnectionInner::Server(pipe) => {
                timeout(self.timeout, pipe.read_exact(&mut len_bytes)).await
            }
            ConnectionInner::Client(pipe) => {
                timeout(self.timeout, pipe.read_exact(&mut len_bytes)).await
            }
        };

        read_result
            .map_err(|_| bunctl_core::Error::Other(anyhow::anyhow!("Receive timeout")))?
            .map_err(|e| {
                error!("Failed to read message length from IPC: {}", e);
                bunctl_core::Error::Io(e)
            })?;

        let len = u32::from_le_bytes(len_bytes) as usize;

        // Validate message size to prevent DoS
        if len > MAX_MESSAGE_SIZE {
            error!("Message size {} exceeds maximum {}", len, MAX_MESSAGE_SIZE);
            return Err(bunctl_core::Error::Other(anyhow::anyhow!(
                "Message size {} exceeds maximum allowed size {}",
                len,
                MAX_MESSAGE_SIZE
            )));
        }

        trace!("IPC message length: {} bytes", len);
        let mut buffer = vec![0u8; len];

        let read_result = match &mut self.inner {
            ConnectionInner::Server(pipe) => {
                timeout(self.timeout, pipe.read_exact(&mut buffer)).await
            }
            ConnectionInner::Client(pipe) => {
                timeout(self.timeout, pipe.read_exact(&mut buffer)).await
            }
        };

        read_result
            .map_err(|_| bunctl_core::Error::Other(anyhow::anyhow!("Receive timeout")))?
            .map_err(|e| {
                error!("Failed to read message body from IPC: {}", e);
                bunctl_core::Error::Io(e)
            })?;

        let message = serde_json::from_slice(&buffer).map_err(|e| {
            error!("Failed to deserialize IPC message: {}", e);
            bunctl_core::Error::Other(e.into())
        })?;

        debug!("IPC received message: {:?}", message);
        Ok(message)
    }

    /// Send a response through the connection
    pub async fn send(&mut self, response: &IpcResponse) -> bunctl_core::Result<()> {
        debug!("IPC sending response: {:?}", response);

        let data = serde_json::to_vec(response).map_err(|e| {
            error!("Failed to serialize IPC response: {}", e);
            bunctl_core::Error::Other(e.into())
        })?;

        // Validate message size
        if data.len() > MAX_MESSAGE_SIZE {
            error!(
                "Response size {} exceeds maximum {}",
                data.len(),
                MAX_MESSAGE_SIZE
            );
            return Err(bunctl_core::Error::Other(anyhow::anyhow!(
                "Response size {} exceeds maximum allowed size {}",
                data.len(),
                MAX_MESSAGE_SIZE
            )));
        }

        trace!("IPC response serialized to {} bytes", data.len());
        let len = (data.len() as u32).to_le_bytes();

        // Send with timeout
        let write_result = match &mut self.inner {
            ConnectionInner::Server(pipe) => {
                timeout(self.timeout, async {
                    pipe.write_all(&len).await?;
                    pipe.write_all(&data).await?;
                    pipe.flush().await
                })
                .await
            }
            ConnectionInner::Client(pipe) => {
                timeout(self.timeout, async {
                    pipe.write_all(&len).await?;
                    pipe.write_all(&data).await?;
                    pipe.flush().await
                })
                .await
            }
        };

        write_result
            .map_err(|_| bunctl_core::Error::Other(anyhow::anyhow!("Send timeout")))?
            .map_err(|e| {
                error!("Failed to send IPC response: {}", e);
                bunctl_core::Error::Io(e)
            })?;

        trace!("IPC response sent successfully");
        Ok(())
    }
}

/// Windows named pipe client for IPC communication
pub struct IpcClient {
    pipe: NamedPipeClient,
    timeout: Duration,
}

impl IpcClient {
    /// Connect to an IPC server at the specified path
    pub async fn connect(path: impl AsRef<Path>) -> bunctl_core::Result<Self> {
        let pipe_name = format!(
            r"\\.\pipe\bunctl_{}",
            path.as_ref()
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("default"))
                .to_string_lossy()
        );

        debug!("Connecting to named pipe: {}", pipe_name);

        let pipe = ClientOptions::new().open(&pipe_name).map_err(|e| {
            error!("Failed to connect to IPC server at {}: {}", pipe_name, e);
            bunctl_core::Error::Io(e)
        })?;

        debug!("Successfully connected to IPC server at: {}", pipe_name);
        Ok(Self {
            pipe,
            timeout: DEFAULT_TIMEOUT,
        })
    }

    /// Set the timeout for operations on this client
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Send a message to the server
    pub async fn send(&mut self, msg: &IpcMessage) -> bunctl_core::Result<()> {
        debug!("Sending message: {:?}", msg);

        let data = serde_json::to_vec(msg).map_err(|e| {
            error!("Failed to serialize message: {}", e);
            bunctl_core::Error::Other(e.into())
        })?;

        // Validate message size
        if data.len() > MAX_MESSAGE_SIZE {
            error!(
                "Message size {} exceeds maximum {}",
                data.len(),
                MAX_MESSAGE_SIZE
            );
            return Err(bunctl_core::Error::Other(anyhow::anyhow!(
                "Message size {} exceeds maximum allowed size {}",
                data.len(),
                MAX_MESSAGE_SIZE
            )));
        }

        trace!("Sending message of {} bytes", data.len());
        let len = (data.len() as u32).to_le_bytes();

        // Send with timeout
        timeout(self.timeout, async {
            self.pipe.write_all(&len).await?;
            self.pipe.write_all(&data).await?;
            self.pipe.flush().await
        })
        .await
        .map_err(|_| bunctl_core::Error::Other(anyhow::anyhow!("Send timeout")))?
        .map_err(|e| {
            error!("Failed to send message: {}", e);
            bunctl_core::Error::Io(e)
        })?;

        trace!("Message sent successfully");
        Ok(())
    }

    /// Receive a response from the server
    pub async fn recv(&mut self) -> bunctl_core::Result<IpcResponse> {
        trace!("Waiting to receive response");

        // Read message length with timeout
        let mut len_bytes = [0u8; 4];
        timeout(self.timeout, self.pipe.read_exact(&mut len_bytes))
            .await
            .map_err(|_| bunctl_core::Error::Other(anyhow::anyhow!("Receive timeout")))?
            .map_err(|e| {
                error!("Failed to read response length: {}", e);
                bunctl_core::Error::Io(e)
            })?;

        let len = u32::from_le_bytes(len_bytes) as usize;

        // Validate message size
        if len > MAX_MESSAGE_SIZE {
            error!("Response size {} exceeds maximum {}", len, MAX_MESSAGE_SIZE);
            return Err(bunctl_core::Error::Other(anyhow::anyhow!(
                "Response size {} exceeds maximum allowed size {}",
                len,
                MAX_MESSAGE_SIZE
            )));
        }

        trace!("Receiving response of {} bytes", len);
        let mut buffer = vec![0u8; len];

        timeout(self.timeout, self.pipe.read_exact(&mut buffer))
            .await
            .map_err(|_| bunctl_core::Error::Other(anyhow::anyhow!("Receive timeout")))?
            .map_err(|e| {
                error!("Failed to read response body: {}", e);
                bunctl_core::Error::Io(e)
            })?;

        let response = serde_json::from_slice(&buffer).map_err(|e| {
            error!("Failed to deserialize response: {}", e);
            bunctl_core::Error::Other(e.into())
        })?;

        debug!("Received response: {:?}", response);
        Ok(response)
    }
}
