use crate::{LineBuffer, LineBufferConfig, LogRotation, RotationConfig};
use bunctl_core::Result;
use bytes::Bytes;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time;

#[derive(Debug, Clone)]
pub struct LogWriterConfig {
    pub path: PathBuf,
    pub rotation: RotationConfig,
    pub buffer_size: usize,
    pub flush_interval: Duration,
}

pub struct LogWriter {
    _path: PathBuf,
    file: Arc<Mutex<BufWriter<File>>>,
    rotation: Arc<Mutex<LogRotation>>,
    buffer: Arc<LineBuffer>,
    tx: mpsc::Sender<LogCommand>,
    shutdown_rx: Option<oneshot::Receiver<()>>,
}

impl std::fmt::Debug for LogWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogWriter")
            .field("_path", &self._path)
            .field("file", &"<BufWriter>")
            .field("rotation", &"<LogRotation>")
            .field("buffer", &"<LineBuffer>")
            .field("tx", &"<Sender>")
            .field("shutdown_rx", &"<Receiver>")
            .finish()
    }
}

enum LogCommand {
    Write(Bytes),
    FlushAndWait(oneshot::Sender<()>),
    Rotate,
    Close,
}

impl LogWriter {
    pub async fn new(config: LogWriterConfig) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(true)
            .open(&config.path)
            .await?;

        let file = BufWriter::with_capacity(config.buffer_size, file);
        let rotation = LogRotation::new(config.rotation);
        let buffer = LineBuffer::new(LineBufferConfig {
            max_size: config.buffer_size,
            max_lines: 1000,
        });

        let (tx, mut rx) = mpsc::channel::<LogCommand>(10000);
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let writer = Self {
            _path: config.path.clone(),
            file: Arc::new(Mutex::new(file)),
            rotation: Arc::new(Mutex::new(rotation)),
            buffer: Arc::new(buffer),
            tx,
            shutdown_rx: Some(shutdown_rx),
        };

        let file_clone = writer.file.clone();
        let rotation_clone = writer.rotation.clone();
        let buffer_clone = writer.buffer.clone();
        let path_clone = config.path.clone();
        let flush_interval = config.flush_interval;

        tokio::spawn(async move {
            let mut interval = time::interval(flush_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if !buffer_clone.is_empty() {
                            Self::flush_buffer(&buffer_clone, &file_clone).await.ok();
                        }
                    }
                    Some(cmd) = rx.recv() => {
                        match cmd {
                            LogCommand::Write(data) => {
                                buffer_clone.write(&data);
                            }
                            LogCommand::FlushAndWait(done_tx) => {
                                Self::flush_buffer(&buffer_clone, &file_clone).await.ok();
                                // Signal that flush is complete
                                let _ = done_tx.send(());
                            }
                            LogCommand::Rotate => {
                                Self::rotate_file(&path_clone, &file_clone, &rotation_clone).await.ok();
                            }
                            LogCommand::Close => {
                                Self::flush_buffer(&buffer_clone, &file_clone).await.ok();
                                break;
                            }
                        }
                    }
                }
            }

            // Signal that the background task has completed
            let _ = shutdown_tx.send(());
        });

        Ok(writer)
    }

    async fn flush_buffer(
        buffer: &Arc<LineBuffer>,
        file: &Arc<Mutex<BufWriter<File>>>,
    ) -> Result<()> {
        let lines = buffer.get_lines();
        let has_incomplete = !buffer.is_empty(); // Check before draining

        if lines.is_empty() && !has_incomplete {
            return Ok(());
        }

        let mut file = file.lock().await;
        for line in lines {
            file.write_all(&line).await?;
        }

        if let Some(incomplete) = buffer.flush_incomplete() {
            file.write_all(&incomplete).await?;
            file.write_all(b"\n").await?;
        }

        file.flush().await?;
        file.get_mut().sync_all().await?; // Force sync to disk

        Ok(())
    }

    async fn rotate_file(
        path: &PathBuf,
        file: &Arc<Mutex<BufWriter<File>>>,
        rotation: &Arc<Mutex<LogRotation>>,
    ) -> Result<()> {
        let mut file_guard = file.lock().await;
        file_guard.flush().await?;
        drop(file_guard);

        let mut rotation_guard = rotation.lock().await;
        rotation_guard.rotate(path).await?;

        let new_file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(true)
            .open(path)
            .await?;

        let mut file_guard = file.lock().await;
        *file_guard = BufWriter::new(new_file);

        Ok(())
    }

    pub fn write(&self, data: impl Into<Bytes>) -> Result<()> {
        // Use try_send for non-blocking write
        self.tx
            .try_send(LogCommand::Write(data.into()))
            .map_err(|e| {
                bunctl_core::Error::Other(anyhow::anyhow!("Failed to send log command: {}", e))
            })
    }

    pub fn write_line(&self, line: impl AsRef<str>) -> Result<()> {
        let mut data = line.as_ref().as_bytes().to_vec();
        if !data.ends_with(b"\n") {
            data.push(b'\n');
        }
        self.write(data)
    }

    pub async fn flush(&self) -> Result<()> {
        // Create a oneshot channel to wait for flush completion
        let (done_tx, done_rx) = oneshot::channel();

        // Send flush command with completion notification
        self.tx
            .send(LogCommand::FlushAndWait(done_tx))
            .await
            .map_err(|e| {
                bunctl_core::Error::Other(anyhow::anyhow!("Failed to send flush command: {}", e))
            })?;

        // Wait for flush to complete (with timeout)
        match tokio::time::timeout(Duration::from_secs(1), done_rx).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => {
                // Channel was dropped
                Err(bunctl_core::Error::Other(anyhow::anyhow!(
                    "Flush operation failed"
                )))
            }
            Err(_) => {
                // Timeout
                Err(bunctl_core::Error::Other(anyhow::anyhow!(
                    "Flush operation timed out"
                )))
            }
        }
    }

    pub async fn rotate(&self) -> Result<()> {
        self.tx.send(LogCommand::Rotate).await.map_err(|e| {
            bunctl_core::Error::Other(anyhow::anyhow!("Failed to send rotate command: {}", e))
        })?;
        // Give time for rotation to process
        time::sleep(Duration::from_millis(10)).await;
        Ok(())
    }

    pub async fn close(mut self) -> Result<()> {
        // Send close command to the background task
        self.tx.send(LogCommand::Close).await.map_err(|e| {
            bunctl_core::Error::Other(anyhow::anyhow!("Failed to send close command: {}", e))
        })?;

        // Force a flush of the buffer before closing
        let _ = Self::flush_buffer(&self.buffer, &self.file).await;

        // Wait for the background task to actually finish
        if let Some(shutdown_rx) = self.shutdown_rx.take() {
            // Wait for the shutdown signal with a timeout
            match tokio::time::timeout(Duration::from_secs(5), shutdown_rx).await {
                Ok(Ok(())) => {
                    // Background task completed successfully
                }
                Ok(Err(_)) => {
                    // Channel was dropped without sending (shouldn't happen)
                    eprintln!("Warning: LogWriter background task ended unexpectedly");
                }
                Err(_) => {
                    // Timeout - background task didn't finish in time
                    eprintln!("Warning: LogWriter background task didn't finish within timeout");
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct AsyncLogWriter {
    inner: std::sync::Arc<LogWriter>,
}

impl AsyncLogWriter {
    pub async fn new(config: LogWriterConfig) -> Result<Self> {
        Ok(Self {
            inner: std::sync::Arc::new(LogWriter::new(config).await?),
        })
    }

    pub fn write(&self, data: impl Into<Bytes>) -> Result<()> {
        self.inner.write(data)
    }

    pub fn write_line(&self, line: impl AsRef<str>) -> Result<()> {
        self.inner.write_line(line)
    }

    pub async fn flush(&self) -> Result<()> {
        self.inner.flush().await
    }

    pub async fn rotate(&self) -> Result<()> {
        self.inner.rotate().await
    }

    pub async fn close(self) -> Result<()> {
        // Get the inner Arc and try to get the owned LogWriter
        // If we're the only reference, we can close it properly
        match std::sync::Arc::try_unwrap(self.inner) {
            Ok(inner) => inner.close().await,
            Err(arc) => {
                // If there are other references, we can't take ownership to call close()
                // Just send the close command - the background task will clean up eventually
                arc.tx.send(LogCommand::Close).await.map_err(|e| {
                    bunctl_core::Error::Other(anyhow::anyhow!(
                        "Failed to send close command: {}",
                        e
                    ))
                })?;
                // Note: We can't wait for completion here since we don't own the shutdown_rx
                // This is a design limitation when there are multiple references
                eprintln!(
                    "Warning: AsyncLogWriter has multiple references, cannot wait for clean shutdown"
                );
                Ok(())
            }
        }
    }
}
