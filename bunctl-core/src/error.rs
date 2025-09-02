use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Process spawn failed: {0}")]
    SpawnFailed(String),
    
    #[error("Process {0} not found")]
    ProcessNotFound(String),
    
    #[error("Config error: {0}")]
    Config(String),
    
    #[error("Invalid app name: {0}")]
    InvalidAppName(String),
    
    #[error("App {0} already exists")]
    AppAlreadyExists(String),
    
    #[error("Supervisor error: {0}")]
    Supervisor(String),
    
    #[error("Timeout waiting for {0}")]
    Timeout(String),
    
    #[error("Signal handling error: {0}")]
    Signal(String),
    
    #[cfg(unix)]
    #[error("Unix error: {0}")]
    Unix(#[from] nix::errno::Errno),
    
    #[cfg(windows)]
    #[error("Windows error: {0}")]
    Windows(u32),
    
    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, Error>;