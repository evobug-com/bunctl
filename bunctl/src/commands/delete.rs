use crate::cli::DeleteArgs;
use crate::common::{
    SUCCESS_ICON, connect_to_daemon, daemon_not_running_message, validate_app_name,
};
use anyhow::Context;
use bunctl_ipc::{IpcMessage, IpcResponse};

pub async fn execute(args: DeleteArgs) -> anyhow::Result<()> {
    // Validate the application name
    validate_app_name(&args.name)?;

    if !args.force {
        println!(
            "Are you sure you want to delete app '{}'? This action cannot be undone.",
            args.name
        );
        println!("Use --force to skip this confirmation.");
        return Ok(());
    }

    let mut client = connect_to_daemon()
        .await
        .context(daemon_not_running_message("delete application"))?;

    let msg = IpcMessage::Delete {
        name: args.name.clone(),
    };

    client
        .send(&msg)
        .await
        .context("Failed to send delete command")?;

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
