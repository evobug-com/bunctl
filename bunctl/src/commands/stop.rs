use crate::cli::StopArgs;
use crate::common::{
    SUCCESS_ICON, connect_to_daemon, daemon_not_running_message, validate_app_name,
};
use anyhow::Context;
use bunctl_ipc::{IpcMessage, IpcResponse};

pub async fn execute(args: StopArgs) -> anyhow::Result<()> {
    // Validate the application name
    validate_app_name(&args.name)?;

    let mut client = connect_to_daemon()
        .await
        .context(daemon_not_running_message("stop application"))?;

    let msg = IpcMessage::Stop {
        name: args.name.clone(),
    };

    client
        .send(&msg)
        .await
        .context("Failed to send stop command")?;

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
