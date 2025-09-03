use bunctl_ipc::{IpcClient, IpcMessage, IpcResponse};
use std::path::PathBuf;

pub async fn execute() -> anyhow::Result<()> {
    let socket_path = get_socket_path();
    
    let mut client = match IpcClient::connect(&socket_path).await {
        Ok(client) => client,
        Err(_) => {
            println!("No daemon running");
            return Ok(());
        }
    };
    
    let msg = IpcMessage::List;
    
    client.send(&msg).await?;
    
    match client.recv().await? {
        IpcResponse::Data { data } => {
            if let Some(apps) = data.as_array() {
                if apps.is_empty() {
                    println!("No apps configured");
                } else {
                    println!("Apps:");
                    for app in apps {
                        if let Some(name) = app.as_str() {
                            println!("  - {}", name);
                        }
                    }
                }
            }
            Ok(())
        }
        IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        _ => Ok(()),
    }
}

fn get_socket_path() -> PathBuf {
    bunctl_core::config::default_socket_path()
}
