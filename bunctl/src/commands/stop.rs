use crate::cli::StopArgs;
use crate::common::{
    SUCCESS_ICON, connect_to_daemon, daemon_not_running_message, validate_app_name,
};
use anyhow::Context;
use bunctl_core::config::ConfigLoader;
use bunctl_ipc::{IpcMessage, IpcResponse};

pub async fn execute(args: StopArgs) -> anyhow::Result<()> {
    // If no name provided, try to auto-discover from config
    let name = if let Some(name) = args.name {
        name
    } else {
        // Try to load config from current directory
        let loader = ConfigLoader::new();
        let config = loader
            .load()
            .await
            .context("No app name provided and no config file found in current directory")?;

        if config.apps.is_empty() {
            return Err(anyhow::anyhow!("No apps found in config file"));
        }

        if config.apps.len() > 1 {
            let app_names: Vec<String> = config.apps.iter().map(|a| a.name.clone()).collect();
            return Err(anyhow::anyhow!(
                "Multiple apps found in config: {}. Please specify which app to stop.",
                app_names.join(", ")
            ));
        }

        config.apps[0].name.clone()
    };

    // Validate the application name
    validate_app_name(&name)?;

    let mut client = connect_to_daemon()
        .await
        .context(daemon_not_running_message("stop application"))?;

    let msg = IpcMessage::Stop { name: name.clone() };

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
        IpcResponse::Error { message } => {
            Err(anyhow::anyhow!("Failed to stop app {}: {}", name, message))
        }
        _ => Ok(()),
    }
}
