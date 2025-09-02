use crate::{LineBuffer, LineBufferConfig, LogRotation, RotationConfig};
use bunctl_core::Result;
use bytes::Bytes;
use crossbeam_channel::{bounded, Sender};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::Mutex;
use tokio::time;

#[derive(Debug, Clone)]
pub struct LogWriterConfig {
    pub path: PathBuf,
    pub rotation: RotationConfig,
    pub buffer_size: usize,
    pub flush_interval: Duration,
}

#[derive(Debug)]
pub struct LogWriter {
    path: PathBuf,
    file: Arc<Mutex<BufWriter<File>>>,
    rotation: Arc<Mutex<LogRotation>>,
    buffer: Arc<LineBuffer>,
    tx: Sender<LogCommand>,
}

enum LogCommand {
    Write(Bytes),
    Flush,
    Rotate,
    Close,
}

impl LogWriter {
    pub async fn new(config: LogWriterConfig) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&config.path)
            .await?;
        
        let file = BufWriter::with_capacity(config.buffer_size, file);
        let rotation = LogRotation::new(config.rotation);
        let buffer = LineBuffer::new(LineBufferConfig {
            max_size: config.buffer_size,
            max_lines: 1000,
        });
        
        let (tx, rx) = bounded::<LogCommand>(10000);
        
        let writer = Self {
            path: config.path.clone(),
            file: Arc::new(Mutex::new(file)),
            rotation: Arc::new(Mutex::new(rotation)),
            buffer: Arc::new(buffer),
            tx,
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
                    cmd = tokio::task::spawn_blocking({
                        let rx = rx.clone();
                        move || rx.recv()
                    }) => {
                        match cmd {
                            Ok(Ok(LogCommand::Write(data))) => {
                                buffer_clone.write(&data);
                            }
                            Ok(Ok(LogCommand::Flush)) => {
                                Self::flush_buffer(&buffer_clone, &file_clone).await.ok();
                            }
                            Ok(Ok(LogCommand::Rotate)) => {
                                Self::rotate_file(&path_clone, &file_clone, &rotation_clone).await.ok();
                            }
                            Ok(Ok(LogCommand::Close)) => {
                                Self::flush_buffer(&buffer_clone, &file_clone).await.ok();
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
        });
        
        Ok(writer)
    }
    
    async fn flush_buffer(buffer: &Arc<LineBuffer>, file: &Arc<Mutex<BufWriter<File>>>) -> Result<()> {
        let lines = buffer.get_lines();
        if lines.is_empty() {
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
            .append(true)
            .open(path)
            .await?;
        
        let mut file_guard = file.lock().await;
        *file_guard = BufWriter::new(new_file);
        
        Ok(())
    }
    
    pub fn write(&self, data: impl Into<Bytes>) -> Result<()> {
        self.tx
            .send(LogCommand::Write(data.into()))
            .map_err(|e| bunctl_core::Error::Other(anyhow::anyhow!("Failed to send log command: {}", e)))
    }
    
    pub fn write_line(&self, line: impl AsRef<str>) -> Result<()> {
        let mut data = line.as_ref().as_bytes().to_vec();
        if !data.ends_with(b"\n") {
            data.push(b'\n');
        }
        self.write(data)
    }
    
    pub async fn flush(&self) -> Result<()> {
        self.tx
            .send(LogCommand::Flush)
            .map_err(|e| bunctl_core::Error::Other(anyhow::anyhow!("Failed to send flush command: {}", e)))?;
        time::sleep(Duration::from_millis(10)).await;
        Ok(())
    }
    
    pub async fn rotate(&self) -> Result<()> {
        self.tx
            .send(LogCommand::Rotate)
            .map_err(|e| bunctl_core::Error::Other(anyhow::anyhow!("Failed to send rotate command: {}", e)))?;
        time::sleep(Duration::from_millis(10)).await;
        Ok(())
    }
    
    pub async fn close(self) -> Result<()> {
        self.tx
            .send(LogCommand::Close)
            .map_err(|e| bunctl_core::Error::Other(anyhow::anyhow!("Failed to send close command: {}", e)))?;
        time::sleep(Duration::from_millis(100)).await;
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
        Ok(())
    }
}