use crate::cli::RestartArgs;
use bunctl_ipc::{IpcClient, IpcMessage, IpcResponse};
use std::path::PathBuf;

pub async fn execute(args: RestartArgs) -> anyhow::Result<()> {
    let socket_path = get_socket_path();

    let mut client = IpcClient::connect(&socket_path)
        .await
        .map_err(|_| anyhow::anyhow!("Daemon not running. No apps to restart."))?;

    let msg = IpcMessage::Restart {
        name: args.name.clone(),
    };

    if args.wait > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(args.wait)).await;
    }

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
