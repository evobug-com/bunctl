mod buffer;
mod rotation;
mod writer;

pub use buffer::{LineBuffer, LineBufferConfig};
pub use rotation::{LogRotation, RotationConfig, RotationStrategy};
pub use writer::{AsyncLogWriter, LogWriter, LogWriterConfig};

use bunctl_core::{AppId, Result};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct LogManager {
    writers: Arc<dashmap::DashMap<AppId, Arc<AsyncLogWriter>>>,
    config: LogConfig,
}

#[derive(Debug, Clone)]
pub struct LogConfig {
    pub base_dir: PathBuf,
    pub max_file_size: u64,
    pub max_files: u32,
    pub compression: bool,
    pub buffer_size: usize,
    pub flush_interval_ms: u64,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("/var/log/bunctl"),
            max_file_size: 10 * 1024 * 1024,
            max_files: 10,
            compression: true,
            buffer_size: 8192,
            flush_interval_ms: 100,
        }
    }
}

impl LogManager {
    pub fn new(config: LogConfig) -> Self {
        Self {
            writers: Arc::new(dashmap::DashMap::new()),
            config,
        }
    }

    pub async fn get_writer(&self, app_id: &AppId) -> Result<Arc<AsyncLogWriter>> {
        if let Some(writer) = self.writers.get(app_id) {
            return Ok(writer.clone());
        }

        let log_path = self.config.base_dir.join(format!("{}.log", app_id));
        let writer_config = LogWriterConfig {
            path: log_path,
            rotation: RotationConfig {
                strategy: RotationStrategy::Size(self.config.max_file_size),
                max_files: self.config.max_files,
                compression: self.config.compression,
            },
            buffer_size: self.config.buffer_size,
            flush_interval: std::time::Duration::from_millis(self.config.flush_interval_ms),
        };

        let writer = Arc::new(AsyncLogWriter::new(writer_config).await?);
        self.writers.insert(app_id.clone(), writer.clone());
        Ok(writer)
    }

    pub async fn remove_writer(&self, app_id: &AppId) {
        if let Some((_, writer)) = self.writers.remove(app_id) {
            let _ = writer.flush().await;
        }
    }

    pub async fn flush_all(&self) -> Result<()> {
        for writer in self.writers.iter() {
            writer.flush().await?;
        }
        Ok(())
    }

    pub async fn rotate_all(&self) -> Result<()> {
        for writer in self.writers.iter() {
            writer.rotate().await?;
        }
        Ok(())
    }
}
