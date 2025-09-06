use crate::cli::DeleteArgs;
use bunctl_ipc::{IpcClient, IpcMessage, IpcResponse};
use std::path::PathBuf;

pub async fn execute(args: DeleteArgs) -> anyhow::Result<()> {
    if !args.force {
        println!(
            "Are you sure you want to delete app '{}'? This action cannot be undone.",
            args.name
        );
        println!("Use --force to skip this confirmation.");
        return Ok(());
    }

    let socket_path = get_socket_path();

    let mut client = IpcClient::connect(&socket_path)
        .await
        .map_err(|_| anyhow::anyhow!("Daemon not running. No apps to delete."))?;

    let msg = IpcMessage::Delete {
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
