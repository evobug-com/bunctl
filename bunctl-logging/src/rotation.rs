use chrono::{DateTime, Local, Timelike};
use flate2::Compression;
use flate2::write::GzEncoder;
use std::fs;
use std::path::Path;
use tokio::fs as tokio_fs;
use tracing::{debug, error, warn};

#[derive(Debug, Clone)]
pub enum RotationStrategy {
    Size(u64),
    Daily,
    Hourly,
    Never,
}

#[derive(Debug, Clone)]
pub struct RotationConfig {
    pub strategy: RotationStrategy,
    pub max_files: u32,
    pub compression: bool,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            strategy: RotationStrategy::Size(10 * 1024 * 1024),
            max_files: 10,
            compression: true,
        }
    }
}

#[derive(Debug)]
pub struct LogRotation {
    config: RotationConfig,
    current_size: u64,
    last_rotation: DateTime<Local>,
}

impl LogRotation {
    pub fn new(config: RotationConfig) -> Self {
        Self {
            config,
            current_size: 0,
            last_rotation: Local::now(),
        }
    }

    pub fn should_rotate(&self, current_size: u64) -> bool {
        match self.config.strategy {
            RotationStrategy::Size(max_size) => current_size >= max_size,
            RotationStrategy::Daily => {
                let now = Local::now();
                now.date_naive() != self.last_rotation.date_naive()
            }
            RotationStrategy::Hourly => {
                let now = Local::now();
                now.date_naive() != self.last_rotation.date_naive()
                    || now.time().hour() != self.last_rotation.time().hour()
            }
            RotationStrategy::Never => false,
        }
    }

    pub async fn rotate(&mut self, log_path: &Path) -> bunctl_core::Result<()> {
        if !log_path.exists() {
            return Ok(());
        }

        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let rotated_name = if self.config.compression {
            format!(
                "{}.{}.log.gz",
                log_path.file_stem().unwrap_or_default().to_string_lossy(),
                timestamp
            )
        } else {
            format!(
                "{}.{}.log",
                log_path.file_stem().unwrap_or_default().to_string_lossy(),
                timestamp
            )
        };

        let parent = log_path
            .parent()
            .ok_or_else(|| bunctl_core::Error::Other(anyhow::anyhow!("Invalid log path")))?;
        let rotated_path = parent.join(&rotated_name);

        debug!("Rotating log from {:?} to {:?}", log_path, rotated_path);

        // Perform rotation based on configuration
        if self.config.compression {
            self.compress_and_rotate(log_path, &rotated_path).await?;
        } else {
            self.rename_or_copy(log_path, &rotated_path).await?;
        }

        // Sync parent directory on Unix to ensure directory entry is persisted
        #[cfg(unix)]
        {
            if let Ok(dir) = std::fs::File::open(parent) {
                use std::os::unix::io::AsRawFd;
                unsafe {
                    libc::fsync(dir.as_raw_fd());
                }
            }
        }

        // Clean up old files
        self.cleanup_old_files(log_path).await?;

        // Update rotation state
        self.last_rotation = Local::now();
        self.current_size = 0;

        debug!("Log rotation completed successfully");
        Ok(())
    }

    async fn rename_or_copy(&self, source: &Path, dest: &Path) -> bunctl_core::Result<()> {
        // Try atomic rename first
        match tokio_fs::rename(source, dest).await {
            Ok(_) => {
                debug!("Successfully renamed log file");
                Ok(())
            }
            Err(e) => {
                // On Windows, rename might fail if file is open
                #[cfg(windows)]
                {
                    debug!("Rename failed, trying copy+truncate: {}", e);
                    // Copy the file
                    tokio_fs::copy(source, dest).await?;
                    // Truncate the original
                    tokio_fs::write(source, b"").await?;
                    Ok(())
                }

                #[cfg(not(windows))]
                {
                    Err(e.into())
                }
            }
        }
    }

    async fn compress_and_rotate(&self, source: &Path, dest: &Path) -> bunctl_core::Result<()> {
        let source_path = source.to_path_buf();
        let dest_path = dest.to_path_buf();

        // Use spawn_blocking for CPU-intensive compression
        let result = tokio::task::spawn_blocking(move || -> bunctl_core::Result<()> {
            // Open source file
            let input = fs::File::open(&source_path)?;
            let mut reader = std::io::BufReader::with_capacity(65536, input);

            // Create compressed output
            let output = fs::File::create(&dest_path)?;
            let mut encoder = GzEncoder::new(output, Compression::default());

            // Copy and compress
            std::io::copy(&mut reader, &mut encoder)?;
            encoder.finish()?;

            // On Windows, truncate instead of removing
            #[cfg(windows)]
            {
                // Truncate the original file
                fs::write(&source_path, b"")?;
            }

            #[cfg(not(windows))]
            {
                // Remove the original file
                fs::remove_file(&source_path)?;
            }

            Ok(())
        })
        .await;

        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => {
                error!("Compression failed: {}", e);
                // Fallback to simple rename
                self.rename_or_copy(source, dest).await
            }
            Err(e) => {
                error!("Compression task panicked: {}", e);
                // Fallback to simple rename
                self.rename_or_copy(source, dest).await
            }
        }
    }

    async fn cleanup_old_files(&self, log_path: &Path) -> bunctl_core::Result<()> {
        let parent = log_path
            .parent()
            .ok_or_else(|| bunctl_core::Error::Other(anyhow::anyhow!("Invalid log path")))?;

        let base_name = log_path.file_stem().unwrap_or_default().to_string_lossy();

        let mut entries = tokio_fs::read_dir(parent).await?;
        let mut log_files = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if let Some(name) = path.file_name() {
                let name_str = name.to_string_lossy();

                // Skip the current log file
                if path == log_path {
                    continue;
                }

                // Check if this is a rotated log file
                if name_str.starts_with(&*base_name)
                    && (name_str.contains(".log") || name_str.contains(".log.gz"))
                {
                    // Try to get metadata
                    match entry.metadata().await {
                        Ok(metadata) => {
                            if let Ok(modified) = metadata.modified() {
                                log_files.push((path, modified));
                            }
                        }
                        Err(e) => {
                            warn!("Failed to get metadata for {:?}: {}", path, e);
                        }
                    }
                }
            }
        }

        // Sort by modification time (newest first)
        log_files.sort_by(|a, b| b.1.cmp(&a.1));

        // Remove old files exceeding max_files limit
        for (path, _) in log_files.iter().skip(self.config.max_files as usize) {
            match tokio_fs::remove_file(path).await {
                Ok(_) => debug!("Removed old log file: {:?}", path),
                Err(e) => warn!("Failed to remove old log file {:?}: {}", path, e),
            }
        }

        Ok(())
    }

    pub fn update_size(&mut self, bytes_written: u64) {
        self.current_size += bytes_written;
    }

    pub fn reset(&mut self) {
        self.current_size = 0;
        self.last_rotation = Local::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_rotation_by_size() {
        let config = RotationConfig {
            strategy: RotationStrategy::Size(100),
            max_files: 3,
            compression: false,
        };

        let rotation = LogRotation::new(config);

        assert!(!rotation.should_rotate(50));
        assert!(!rotation.should_rotate(99));
        assert!(rotation.should_rotate(100));
        assert!(rotation.should_rotate(101));
    }

    #[tokio::test]
    async fn test_rotation_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("test.log");

        // Create some old log files
        for i in 0..5 {
            let old_log = temp_dir.path().join(format!("test.{}.log", i));
            tokio::fs::write(&old_log, format!("old log {}", i))
                .await
                .unwrap();
        }

        let config = RotationConfig {
            strategy: RotationStrategy::Size(100),
            max_files: 2,
            compression: false,
        };

        let mut rotation = LogRotation::new(config);

        // Create current log file
        tokio::fs::write(&log_path, "current log").await.unwrap();

        // Perform rotation
        rotation.rotate(&log_path).await.unwrap();

        // Count remaining files
        let mut entries = tokio::fs::read_dir(temp_dir.path()).await.unwrap();
        let mut file_count = 0;

        while let Some(entry) = entries.next_entry().await.unwrap() {
            if entry.path().extension().map_or(false, |ext| ext == "log") {
                file_count += 1;
            }
        }

        // Should have current log + max_files rotated logs
        assert!(file_count <= 3);
    }
}
