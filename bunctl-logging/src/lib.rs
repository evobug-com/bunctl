mod buffer;
mod rotation;
mod writer;

pub use buffer::{LineBuffer, LineBufferConfig};
pub use rotation::{LogRotation, RotationConfig, RotationStrategy};
pub use writer::{AsyncLogWriter, LogWriter, LogWriterConfig};

use bunctl_core::{AppId, Result};
use std::path::PathBuf;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, trace};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredLogs {
    pub errors: Vec<String>,
    pub output: Vec<String>,
}

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
        debug!("Creating LogManager with base_dir: {:?}, max_file_size: {}, max_files: {}", 
               config.base_dir, config.max_file_size, config.max_files);
        Self {
            writers: Arc::new(dashmap::DashMap::new()),
            config,
        }
    }

    pub async fn get_writer(&self, app_id: &AppId) -> Result<Arc<AsyncLogWriter>> {
        if let Some(writer) = self.writers.get(app_id) {
            trace!("Using existing log writer for app: {}", app_id);
            return Ok(writer.clone());
        }

        debug!("Creating new log writer for app: {}", app_id);
        let log_path = self.config.base_dir.join(format!("{}.log", app_id));
        debug!("Log file path for app {}: {:?}", app_id, log_path);
        
        let writer_config = LogWriterConfig {
            path: log_path.clone(),
            rotation: RotationConfig {
                strategy: RotationStrategy::Size(self.config.max_file_size),
                max_files: self.config.max_files,
                compression: self.config.compression,
            },
            buffer_size: self.config.buffer_size,
            flush_interval: std::time::Duration::from_millis(self.config.flush_interval_ms),
        };

        let writer = Arc::new(AsyncLogWriter::new(writer_config).await.map_err(|e| {
            error!("Failed to create log writer for app {} at {:?}: {}", app_id, log_path, e);
            e
        })?);
        
        self.writers.insert(app_id.clone(), writer.clone());
        debug!("Log writer created successfully for app: {} (total writers: {})", app_id, self.writers.len());
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
    
    pub async fn read_logs(&self, app_id: &AppId, lines: usize) -> Result<Vec<String>> {
        let log_path = self.config.base_dir.join(format!("{}.log", app_id));
        
        if !log_path.exists() {
            // Return a helpful message instead of empty array
            return Ok(vec![
                format!("No log file found for app '{}' at {:?}", app_id, log_path),
                "This could mean:".to_string(),
                "1. The app hasn't produced any output yet".to_string(),
                "2. The app was started without a daemon (logs only work with daemon mode)".to_string(),
                "3. Log directory permissions issue".to_string(),
            ]);
        }
        
        // Check if log file is empty
        let metadata = tokio::fs::metadata(&log_path).await?;
        if metadata.len() == 0 {
            return Ok(vec![format!("Log file for '{}' exists but is empty", app_id)]);
        }
        
        // Read the log file
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::fs::File;
        
        let file = File::open(&log_path).await.map_err(|e| {
            bunctl_core::Error::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("Cannot open log file {:?}: {}", log_path, e)
            ))
        })?;
        
        let reader = BufReader::new(file);
        let mut all_lines = Vec::new();
        let mut lines_reader = reader.lines();
        
        while let Ok(Some(line)) = lines_reader.next_line().await {
            all_lines.push(line);
        }
        
        if all_lines.is_empty() {
            return Ok(vec![format!("Log file for '{}' exists but contains no readable lines", app_id)]);
        }
        
        // Return the last N lines
        let start = if all_lines.len() > lines {
            all_lines.len() - lines
        } else {
            0
        };
        
        Ok(all_lines[start..].to_vec())
    }

    pub async fn read_structured_logs(&self, app_id: &AppId, lines: usize) -> Result<StructuredLogs> {
        let log_path = self.config.base_dir.join(format!("{}.log", app_id));
        
        if !log_path.exists() {
            return Ok(StructuredLogs {
                errors: vec![format!("No log file found for app '{}'", app_id)],
                output: vec![],
            });
        }

        // Check if log file is empty
        let metadata = tokio::fs::metadata(&log_path).await?;
        if metadata.len() == 0 {
            return Ok(StructuredLogs {
                errors: vec![],
                output: vec![format!("Log file for '{}' exists but is empty", app_id)],
            });
        }

        // Read and parse the log file
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::fs::File;
        
        let file = File::open(&log_path).await.map_err(|e| {
            bunctl_core::Error::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("Cannot open log file {:?}: {}", log_path, e)
            ))
        })?;
        
        let reader = BufReader::new(file);
        let mut all_lines = Vec::new();
        let mut lines_reader = reader.lines();
        
        while let Ok(Some(line)) = lines_reader.next_line().await {
            all_lines.push(line);
        }
        
        if all_lines.is_empty() {
            return Ok(StructuredLogs {
                errors: vec![format!("Log file for '{}' contains no readable lines", app_id)],
                output: vec![],
            });
        }
        
        // Get the last N lines
        let start = if all_lines.len() > lines {
            all_lines.len() - lines
        } else {
            0
        };
        let recent_lines = &all_lines[start..];
        
        // Separate errors from output based on stream type
        let mut errors = Vec::new();
        let mut output = Vec::new();
        
        for line in recent_lines {
            // Parse format: [app_name] [timestamp] [stream_type] message
            if line.contains("[stderr]") {
                errors.push(line.clone());
            } else {
                output.push(line.clone());
            }
        }
        
        Ok(StructuredLogs { errors, output })
    }

    pub async fn read_all_apps_logs(&self, lines: usize) -> Result<Vec<(String, StructuredLogs)>> {
        let mut all_logs = Vec::new();
        
        // Read the log directory to find all log files
        let mut dir_reader = match tokio::fs::read_dir(&self.config.base_dir).await {
            Ok(reader) => reader,
            Err(_) => return Ok(all_logs), // Empty if directory doesn't exist
        };

        while let Ok(Some(entry)) = dir_reader.next_entry().await {
            let path = entry.path();
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str())
                && file_name.ends_with(".log") {
                    let app_name = file_name.trim_end_matches(".log").to_string();
                    if let Ok(app_id) = AppId::new(&app_name) {
                        let logs = self.read_structured_logs(&app_id, lines).await?;
                        all_logs.push((app_name, logs));
                    }
                }
        }

        // Sort by app name for consistent output
        all_logs.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(all_logs)
    }
}
