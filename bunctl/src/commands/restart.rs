use crate::cli::RestartArgs;
use crate::common::{
    SUCCESS_ICON, connect_to_daemon, daemon_not_running_message, validate_app_name,
};
use anyhow::Context;
use bunctl_ipc::{IpcMessage, IpcResponse};
use std::time::Duration;

pub async fn execute(args: RestartArgs) -> anyhow::Result<()> {
    // Validate the application name
    validate_app_name(&args.name)?;

    let mut client = connect_to_daemon()
        .await
        .context(daemon_not_running_message("restart application"))?;

    let msg = IpcMessage::Restart {
        name: args.name.clone(),
    };

    if args.wait > 0 {
        tokio::time::sleep(Duration::from_millis(args.wait)).await;
    }

    client
        .send(&msg)
        .await
        .context("Failed to send restart command")?;

    match client
        .recv()
        .await
        .context("Failed to receive response from daemon")?
    {
        IpcResponse::Success { message } => {
            println!("{} {}", SUCCESS_ICON, message);
            Ok(())
        }
        IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        _ => Ok(()),
    }
}
