use chrono::{DateTime, Local, Timelike};
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::{self};
use std::path::Path;
use tokio::fs as tokio_fs;

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
                now.date_naive() != self.last_rotation.date_naive() ||
                now.time().hour() != self.last_rotation.time().hour()
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
                log_path.file_stem().unwrap().to_string_lossy(),
                timestamp
            )
        } else {
            format!(
                "{}.{}.log",
                log_path.file_stem().unwrap().to_string_lossy(),
                timestamp
            )
        };
        
        let rotated_path = log_path.parent().unwrap().join(&rotated_name);
        
        if self.config.compression {
            self.compress_and_rotate(log_path, &rotated_path).await?;
        } else {
            // Try to rename, on Windows this might fail if file is in use
            match tokio_fs::rename(log_path, &rotated_path).await {
                Ok(_) => {},
                Err(_) if cfg!(windows) => {
                    // On Windows, copy and truncate instead
                    tokio_fs::copy(log_path, &rotated_path).await?;
                    tokio_fs::write(log_path, b"").await?;
                },
                Err(e) => return Err(e.into()),
            }
        }
        
        self.cleanup_old_files(log_path).await?;
        self.last_rotation = Local::now();
        self.current_size = 0;
        
        Ok(())
    }
    
    async fn compress_and_rotate(&self, source: &Path, dest: &Path) -> bunctl_core::Result<()> {
        let source_path = source.to_path_buf();
        let dest_path = dest.to_path_buf();
        
        tokio::task::spawn_blocking(move || -> bunctl_core::Result<()> {
            let input = fs::File::open(&source_path)?;
            let output = fs::File::create(&dest_path)?;
            let mut encoder = GzEncoder::new(output, Compression::default());
            
            let mut reader = std::io::BufReader::new(input);
            std::io::copy(&mut reader, &mut encoder)?;
            encoder.finish()?;
            
            fs::remove_file(&source_path)?;
            Ok(())
        })
        .await
        .map_err(|e| bunctl_core::Error::Other(e.into()))?
    }
    
    async fn cleanup_old_files(&self, log_path: &Path) -> bunctl_core::Result<()> {
        let parent = log_path.parent().unwrap();
        let base_name = log_path.file_stem().unwrap().to_string_lossy();
        
        let mut entries = tokio_fs::read_dir(parent).await?;
        let mut log_files = Vec::new();
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if let Some(name) = path.file_name() {
                let name_str = name.to_string_lossy();
                if name_str.starts_with(&*base_name) && name_str != base_name {
                    if let Ok(metadata) = entry.metadata().await {
                        if let Ok(modified) = metadata.modified() {
                            log_files.push((path, modified));
                        }
                    }
                }
            }
        }
        
        log_files.sort_by(|a, b| b.1.cmp(&a.1));
        
        for (path, _) in log_files.iter().skip(self.config.max_files as usize) {
            let _ = tokio_fs::remove_file(path).await;
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