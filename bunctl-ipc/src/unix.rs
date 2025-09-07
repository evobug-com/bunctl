use crate::{DEFAULT_TIMEOUT, IpcMessage, IpcResponse, MAX_MESSAGE_SIZE};
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::time::timeout;
use tracing::{debug, error, trace, warn};

/// Unix domain socket server for IPC communication
pub struct IpcServer {
    listener: UnixListener,
}

impl IpcServer {
    /// Create a new IPC server bound to the specified path
    pub async fn bind(path: impl AsRef<Path>) -> bunctl_core::Result<Self> {
        let path = path.as_ref();

        // Try to remove existing socket file, log if it fails
        if let Err(e) = std::fs::remove_file(path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            warn!("Failed to remove existing socket file: {}", e);
        }

        debug!("Binding Unix domain socket to: {:?}", path);
        let listener = UnixListener::bind(path).map_err(|e| {
            error!("Failed to bind Unix domain socket: {}", e);
            bunctl_core::Error::Io(e)
        })?;

        debug!("IPC server successfully bound to: {:?}", path);
        Ok(Self { listener })
    }

    /// Accept a new client connection
    pub async fn accept(&mut self) -> bunctl_core::Result<IpcConnection> {
        debug!("Waiting for client connection");
        let (stream, _addr) = self.listener.accept().await.map_err(|e| {
            error!("Failed to accept connection: {}", e);
            bunctl_core::Error::Io(e)
        })?;
        debug!("Client connected");
        Ok(IpcConnection::new(stream))
    }
}

/// Represents an active IPC connection
pub struct IpcConnection {
    stream: UnixStream,
    timeout: Duration,
}

impl IpcConnection {
    fn new(stream: UnixStream) -> Self {
        Self {
            stream,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Set the timeout for operations on this connection
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Receive a message from the connection
    pub async fn recv(&mut self) -> bunctl_core::Result<IpcMessage> {
        trace!("Waiting to receive message");

        // Read message length with timeout
        let mut len_bytes = [0u8; 4];
        timeout(self.timeout, self.stream.read_exact(&mut len_bytes))
            .await
            .map_err(|_| bunctl_core::Error::Other(anyhow::anyhow!("Receive timeout")))?
            .map_err(|e| {
                error!("Failed to read message length: {}", e);
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

        trace!("Receiving message of {} bytes", len);
        let mut buffer = vec![0u8; len];

        timeout(self.timeout, self.stream.read_exact(&mut buffer))
            .await
            .map_err(|_| bunctl_core::Error::Other(anyhow::anyhow!("Receive timeout")))?
            .map_err(|e| {
                error!("Failed to read message body: {}", e);
                bunctl_core::Error::Io(e)
            })?;

        let message = serde_json::from_slice(&buffer).map_err(|e| {
            error!("Failed to deserialize message: {}", e);
            bunctl_core::Error::Other(e.into())
        })?;

        debug!("Received message: {:?}", message);
        Ok(message)
    }

    /// Send a response through the connection
    pub async fn send(&mut self, response: &IpcResponse) -> bunctl_core::Result<()> {
        debug!("Sending response: {:?}", response);

        let data = serde_json::to_vec(response).map_err(|e| {
            error!("Failed to serialize response: {}", e);
            bunctl_core::Error::Other(e.into())
        })?;

        // Validate our own message size
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

        trace!("Sending response of {} bytes", data.len());
        let len = (data.len() as u32).to_le_bytes();

        // Send with timeout
        timeout(self.timeout, async {
            self.stream.write_all(&len).await?;
            self.stream.write_all(&data).await?;
            self.stream.flush().await
        })
        .await
        .map_err(|_| bunctl_core::Error::Other(anyhow::anyhow!("Send timeout")))?
        .map_err(|e| {
            error!("Failed to send response: {}", e);
            bunctl_core::Error::Io(e)
        })?;

        trace!("Response sent successfully");
        Ok(())
    }
}

/// Unix domain socket client for IPC communication
pub struct IpcClient {
    stream: UnixStream,
    timeout: Duration,
}

impl IpcClient {
    /// Connect to an IPC server at the specified path
    pub async fn connect(path: impl AsRef<Path>) -> bunctl_core::Result<Self> {
        let path = path.as_ref();
        debug!("Connecting to Unix domain socket at: {:?}", path);

        let stream = UnixStream::connect(path).await.map_err(|e| {
            error!("Failed to connect to IPC server: {}", e);
            bunctl_core::Error::Io(e)
        })?;

        debug!("Successfully connected to IPC server");
        Ok(Self {
            stream,
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
            self.stream.write_all(&len).await?;
            self.stream.write_all(&data).await?;
            self.stream.flush().await
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
        timeout(self.timeout, self.stream.read_exact(&mut len_bytes))
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

        timeout(self.timeout, self.stream.read_exact(&mut buffer))
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
