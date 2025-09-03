use crate::cli::StopArgs;
use bunctl_ipc::{IpcClient, IpcMessage, IpcResponse};
use std::path::PathBuf;

pub async fn execute(args: StopArgs) -> anyhow::Result<()> {
    let socket_path = get_socket_path();
    
    let mut client = IpcClient::connect(&socket_path).await
        .map_err(|_| anyhow::anyhow!("Daemon not running. No apps to stop."))?;
    
    let msg = IpcMessage::Stop {
        name: args.name.clone(),
    };
    
    client.send(&msg).await?;
    
    match client.recv().await? {
        IpcResponse::Success { message } => {
            println!("✔ {}", message);
            Ok(())
        }
        IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        _ => Ok(()),
    }
}

fn get_socket_path() -> PathBuf {
    bunctl_core::config::default_socket_path()
}
