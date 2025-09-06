use crate::common::connect_to_daemon;
use anyhow::Context;
use bunctl_ipc::{IpcMessage, IpcResponse};

pub async fn execute() -> anyhow::Result<()> {
    let mut client = match connect_to_daemon().await {
        Ok(client) => client,
        Err(_) => {
            println!("No daemon running");
            return Ok(());
        }
    };

    let msg = IpcMessage::List;

    client
        .send(&msg)
        .await
        .context("Failed to send list command")?;

    match client
        .recv()
        .await
        .context("Failed to receive response from daemon")?
    {
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
