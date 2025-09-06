use crate::{
    LineBuffer, LineBufferConfig, LogMetrics, LogRotation, MetricsSnapshot, RotationConfig,
};
use bunctl_core::Result;
use bytes::Bytes;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::{Mutex, Semaphore, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time;
use tracing::{debug, error, warn};

#[derive(Debug, Clone)]
pub struct LogWriterConfig {
    pub path: PathBuf,
    pub rotation: RotationConfig,
    pub buffer_size: usize,
    pub flush_interval: Duration,
    pub max_concurrent_writes: usize,
    pub enable_compression: bool,
}

impl Default for LogWriterConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("app.log"),
            rotation: RotationConfig::default(),
            buffer_size: 65536, // 64KB buffer
            flush_interval: Duration::from_millis(100),
            max_concurrent_writes: 1000,
            enable_compression: true,
        }
    }
}

enum LogCommand {
    Write(Bytes),
    Flush(oneshot::Sender<Result<()>>),
    Rotate(oneshot::Sender<Result<()>>),
    GetMetrics(oneshot::Sender<MetricsSnapshot>),
    Shutdown,
}

pub struct LogWriter {
    path: PathBuf,
    tx: mpsc::UnboundedSender<LogCommand>,
    metrics: Arc<LogMetrics>,
    write_semaphore: Arc<Semaphore>,
    shutdown_complete: Arc<Mutex<Option<oneshot::Receiver<()>>>>,
    task_handle: Option<JoinHandle<()>>,
    is_shutdown: Arc<AtomicBool>,
}

impl std::fmt::Debug for LogWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogWriter")
            .field("path", &self.path)
            .field("metrics", &self.metrics)
            .field("is_shutdown", &self.is_shutdown)
            .finish()
    }
}

impl LogWriter {
    pub async fn new(config: LogWriterConfig) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = config.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let file = Self::open_file(&config.path).await?;
        let file = BufWriter::with_capacity(config.buffer_size, file);
        let rotation = LogRotation::new(config.rotation.clone());
        let buffer = LineBuffer::new(LineBufferConfig {
            max_size: config.buffer_size,
            max_lines: 10000,
        });

        let (tx, mut rx) = mpsc::unbounded_channel::<LogCommand>();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let metrics = Arc::new(LogMetrics::new());
        let write_semaphore = Arc::new(Semaphore::new(config.max_concurrent_writes));
        let is_shutdown = Arc::new(AtomicBool::new(false));

        let file = Arc::new(Mutex::new(file));
        let rotation = Arc::new(Mutex::new(rotation));
        let buffer = Arc::new(buffer);

        // Clone for background task
        let path_clone = config.path.clone();
        let file_clone = file.clone();
        let rotation_clone = rotation.clone();
        let buffer_clone = buffer.clone();
        let metrics_clone = metrics.clone();
        let is_shutdown_clone = is_shutdown.clone();
        let flush_interval = config.flush_interval;

        let task_handle = tokio::spawn(async move {
            let mut interval = time::interval(flush_interval);
            let mut consecutive_errors = 0u32;
            let max_consecutive_errors = 10;

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Auto-flush on interval
                        if let Err(e) = Self::flush_buffer_with_retry(
                            &buffer_clone,
                            &file_clone,
                            &metrics_clone
                        ).await {
                            error!("Auto-flush failed: {}", e);
                            consecutive_errors += 1;
                            if consecutive_errors >= max_consecutive_errors {
                                warn!("Too many consecutive flush errors, entering degraded mode");
                                time::sleep(Duration::from_secs(10)).await;
                            }
                        } else {
                            consecutive_errors = 0;
                        }
                    }
                    Some(cmd) = rx.recv() => {
                        match cmd {
                            LogCommand::Write(data) => {
                                let start = Instant::now();
                                buffer_clone.write(&data);
                                metrics_clone.record_write(data.len() as u64);
                                metrics_clone.record_write_duration(start.elapsed().as_micros() as u64);

                                // Check if rotation needed
                                let size = Self::estimate_file_size(&file_clone).await;
                                if rotation_clone.lock().await.should_rotate(size)
                                    && let Err(e) = Self::rotate_file_internal(
                                        &path_clone,
                                        &file_clone,
                                        &rotation_clone,
                                        &metrics_clone
                                    ).await {
                                    error!("Auto-rotation failed: {}", e);
                                }
                            }
                            LogCommand::Flush(reply) => {
                                let result = Self::flush_buffer_with_retry(
                                    &buffer_clone,
                                    &file_clone,
                                    &metrics_clone
                                ).await;
                                let _ = reply.send(result);
                            }
                            LogCommand::Rotate(reply) => {
                                let result = Self::rotate_file_internal(
                                    &path_clone,
                                    &file_clone,
                                    &rotation_clone,
                                    &metrics_clone
                                ).await;
                                let _ = reply.send(result);
                            }
                            LogCommand::GetMetrics(reply) => {
                                let _ = reply.send(metrics_clone.snapshot());
                            }
                            LogCommand::Shutdown => {
                                debug!("Shutting down log writer for {:?}", path_clone);
                                // Final flush before shutdown
                                let _ = Self::flush_buffer_with_retry(
                                    &buffer_clone,
                                    &file_clone,
                                    &metrics_clone
                                ).await;
                                break;
                            }
                        }
                    }
                }
            }

            // Mark as shutdown
            is_shutdown_clone.store(true, Ordering::SeqCst);
            // Signal completion
            let _ = shutdown_tx.send(());
        });

        Ok(Self {
            path: config.path,
            tx,
            metrics,
            write_semaphore,
            shutdown_complete: Arc::new(Mutex::new(Some(shutdown_rx))),
            task_handle: Some(task_handle),
            is_shutdown,
        })
    }

    async fn open_file(path: &Path) -> Result<File> {
        let path = path.to_path_buf();
        let mut retry_count = 0;
        loop {
            match OpenOptions::new()
                .create(true)
                .write(true)
                .append(true)
                .open(&path)
                .await
            {
                Ok(file) => return Ok(file),
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    return Err(bunctl_core::Error::Io(e));
                }
                Err(e) => {
                    retry_count += 1;
                    if retry_count >= 5 {
                        return Err(bunctl_core::Error::Io(e));
                    }
                    tokio::time::sleep(Duration::from_millis(100 * retry_count)).await;
                }
            }
        }
    }

    async fn estimate_file_size(file: &Arc<Mutex<BufWriter<File>>>) -> u64 {
        let file = file.lock().await;
        file.get_ref()
            .metadata()
            .await
            .map(|m| m.len())
            .unwrap_or(0)
    }

    async fn flush_buffer_with_retry(
        buffer: &Arc<LineBuffer>,
        file: &Arc<Mutex<BufWriter<File>>>,
        metrics: &Arc<LogMetrics>,
    ) -> Result<()> {
        let start = Instant::now();

        let lines = buffer.get_lines();
        let incomplete = buffer.flush_incomplete();

        if lines.is_empty() && incomplete.is_none() {
            return Ok(());
        }

        let mut retry_count = 0;
        let result = loop {
            let mut file = file.lock().await;

            let write_result = async {
                // Write all complete lines
                for line in &lines {
                    file.write_all(line).await?;
                }

                // Write incomplete line if exists
                if let Some(ref incomplete_data) = incomplete {
                    file.write_all(incomplete_data).await?;
                    file.write_all(b"\n").await?;
                }

                // Flush to OS buffer
                file.flush().await?;

                // Force sync to disk
                file.get_mut().sync_all().await?;
                Ok::<(), std::io::Error>(())
            }
            .await;

            match write_result {
                Ok(()) => break Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::StorageFull => {
                    metrics.record_buffer_overflow();
                    break Err(bunctl_core::Error::Io(e));
                }
                Err(e) => {
                    retry_count += 1;
                    if retry_count >= 5 {
                        break Err(bunctl_core::Error::Io(e));
                    }
                    drop(file); // Release lock before sleeping
                    tokio::time::sleep(Duration::from_millis(100 * retry_count)).await;
                }
            }
        };

        let duration = start.elapsed().as_micros() as u64;
        metrics.record_flush(duration);

        if result.is_err() {
            metrics.record_write_error();
        }
        result
    }

    async fn rotate_file_internal(
        path: &PathBuf,
        file: &Arc<Mutex<BufWriter<File>>>,
        rotation: &Arc<Mutex<LogRotation>>,
        metrics: &Arc<LogMetrics>,
    ) -> Result<()> {
        debug!("Starting log rotation for {:?}", path);

        // Flush before rotation
        {
            let mut file_guard = file.lock().await;
            file_guard.flush().await?;
        }

        // Perform rotation
        let mut rotation_guard = rotation.lock().await;
        rotation_guard.rotate(path).await?;

        // Open new file
        let new_file = Self::open_file(path).await?;

        // Replace file handle
        {
            let mut file_guard = file.lock().await;
            *file_guard = BufWriter::with_capacity(65536, new_file);
        }

        metrics.record_rotation();
        debug!("Log rotation completed for {:?}", path);

        Ok(())
    }

    pub fn write(&self, data: impl Into<Bytes>) -> Result<()> {
        if self.is_shutdown.load(Ordering::Acquire) {
            return Err(bunctl_core::Error::Other(anyhow::anyhow!(
                "LogWriter is shut down"
            )));
        }

        // Try to acquire write permit with timeout
        let permit = match self.write_semaphore.try_acquire() {
            Ok(permit) => permit,
            Err(_) => {
                // Channel is full, drop the message and record it
                self.metrics.record_dropped_message();
                warn!("Write buffer full, dropping log message");
                return Ok(()); // Graceful degradation
            }
        };

        let data = data.into();
        self.tx.send(LogCommand::Write(data)).map_err(|_| {
            self.metrics.record_write_error();
            bunctl_core::Error::Other(anyhow::anyhow!("Log writer channel closed"))
        })?;

        drop(permit); // Release permit
        Ok(())
    }

    pub fn write_line(&self, line: impl AsRef<str>) -> Result<()> {
        let mut data = line.as_ref().as_bytes().to_vec();
        if !data.ends_with(b"\n") {
            data.push(b'\n');
        }
        self.write(data)
    }

    pub async fn flush(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(LogCommand::Flush(tx))
            .map_err(|_| bunctl_core::Error::Other(anyhow::anyhow!("Log writer channel closed")))?;

        match time::timeout(Duration::from_secs(5), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(bunctl_core::Error::Other(anyhow::anyhow!(
                "Flush operation cancelled"
            ))),
            Err(_) => Err(bunctl_core::Error::Other(anyhow::anyhow!(
                "Flush operation timed out"
            ))),
        }
    }

    pub async fn rotate(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(LogCommand::Rotate(tx))
            .map_err(|_| bunctl_core::Error::Other(anyhow::anyhow!("Log writer channel closed")))?;

        match time::timeout(Duration::from_secs(30), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(bunctl_core::Error::Other(anyhow::anyhow!(
                "Rotation operation cancelled"
            ))),
            Err(_) => Err(bunctl_core::Error::Other(anyhow::anyhow!(
                "Rotation operation timed out"
            ))),
        }
    }

    pub async fn get_metrics(&self) -> MetricsSnapshot {
        let (tx, rx) = oneshot::channel();
        if self.tx.send(LogCommand::GetMetrics(tx)).is_err() {
            return self.metrics.snapshot();
        }

        match time::timeout(Duration::from_secs(1), rx).await {
            Ok(Ok(snapshot)) => snapshot,
            _ => self.metrics.snapshot(),
        }
    }

    pub async fn shutdown(mut self) -> Result<()> {
        if self.is_shutdown.load(Ordering::Acquire) {
            return Ok(());
        }

        debug!("Initiating graceful shutdown for {:?}", self.path);

        // Send shutdown command
        let _ = self.tx.send(LogCommand::Shutdown);

        // Wait for the background task to complete
        if let Some(handle) = self.task_handle.take() {
            match time::timeout(Duration::from_secs(10), handle).await {
                Ok(Ok(())) => debug!("Background task completed successfully"),
                Ok(Err(e)) => warn!("Background task panicked: {:?}", e),
                Err(_) => {
                    warn!("Background task did not complete within timeout, aborting");
                    // handle is already consumed by timeout, can't abort
                }
            }
        }

        // Wait for shutdown confirmation
        if let Some(rx) = self.shutdown_complete.lock().await.take() {
            let _ = time::timeout(Duration::from_secs(5), rx).await;
        }

        Ok(())
    }
}

impl Drop for LogWriter {
    fn drop(&mut self) {
        if !self.is_shutdown.load(Ordering::Acquire) {
            // Send shutdown signal
            let _ = self.tx.send(LogCommand::Shutdown);

            // Abort the task if it's still running
            if let Some(handle) = self.task_handle.take() {
                handle.abort();
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct AsyncLogWriter {
    inner: Arc<LogWriter>,
}

impl AsyncLogWriter {
    pub async fn new(config: LogWriterConfig) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(LogWriter::new(config).await?),
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

    pub async fn get_metrics(&self) -> MetricsSnapshot {
        self.inner.get_metrics().await
    }

    pub async fn close(self) -> Result<()> {
        match Arc::try_unwrap(self.inner) {
            Ok(writer) => writer.shutdown().await,
            Err(arc) => {
                // Still has references, try graceful shutdown
                arc.flush().await?;
                Ok(())
            }
        }
    }
}

// Safe to send across threads
unsafe impl Send for LogWriter {}
unsafe impl Sync for LogWriter {}
