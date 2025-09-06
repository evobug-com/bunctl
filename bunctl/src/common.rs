use anyhow::{Context, Result};
use bunctl_core::config;
use bunctl_ipc::{IpcClient, IpcMessage, IpcResponse};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{debug, warn};

/// Standard success indicator for all commands
pub const SUCCESS_ICON: &str = "✓";
/// Standard failure indicator for all commands  
#[allow(dead_code)]
pub const FAILURE_ICON: &str = "✗";
/// Standard running indicator for status displays
pub const RUNNING_ICON: &str = "●";
/// Standard stopped indicator for status displays
pub const STOPPED_ICON: &str = "○";

/// Get the default socket path for IPC communication
pub fn get_socket_path() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("bunctl")
    } else {
        config::default_socket_path()
    }
}

/// Connect to the daemon with timeout and retry logic
pub async fn connect_to_daemon() -> Result<IpcClient> {
    connect_to_daemon_with_timeout(Duration::from_secs(5)).await
}

/// Connect to the daemon with a custom timeout
pub async fn connect_to_daemon_with_timeout(conn_timeout: Duration) -> Result<IpcClient> {
    let socket_path = get_socket_path();

    debug!("Attempting to connect to daemon at {:?}", socket_path);

    match timeout(conn_timeout, IpcClient::connect(&socket_path)).await {
        Ok(Ok(client)) => {
            debug!("Successfully connected to daemon");
            Ok(client)
        }
        Ok(Err(e)) => Err(e).context("Failed to connect to daemon. Is the daemon running?"),
        Err(_) => {
            anyhow::bail!(
                "Connection to daemon timed out after {} seconds",
                conn_timeout.as_secs()
            )
        }
    }
}

/// Send a message to the daemon and wait for response with timeout
#[allow(dead_code)]
pub async fn send_to_daemon_with_timeout(
    client: &mut IpcClient,
    message: IpcMessage,
    response_timeout: Duration,
) -> Result<IpcResponse> {
    debug!("Sending message to daemon: {:?}", message);

    client
        .send(&message)
        .await
        .context("Failed to send message to daemon")?;

    match timeout(response_timeout, client.recv()).await {
        Ok(Ok(response)) => {
            debug!("Received response from daemon: {:?}", response);
            Ok(response)
        }
        Ok(Err(e)) => Err(e).context("Error receiving response from daemon"),
        Err(_) => {
            anyhow::bail!(
                "Response from daemon timed out after {} seconds",
                response_timeout.as_secs()
            )
        }
    }
}

/// Standard daemon connection error message
pub fn daemon_not_running_message(action: &str) -> String {
    format!(
        "Daemon not running. Cannot {}. Start the daemon with 'bunctl daemon' or start an app.",
        action
    )
}

/// Validate an application name
pub fn validate_app_name(name: &str) -> Result<()> {
    if name.is_empty() {
        anyhow::bail!("Application name cannot be empty");
    }

    if name.len() > 255 {
        anyhow::bail!("Application name too long (max 255 characters)");
    }

    // Check for invalid characters
    if name.contains(['/', '\\', '\0', ':']) {
        anyhow::bail!("Application name contains invalid characters");
    }

    // Warn about special names but don't fail
    if name == "all" || name == "daemon" {
        warn!(
            "Using reserved name '{}' - this may have special behavior",
            name
        );
    }

    Ok(())
}

/// Validate a timeout value
#[allow(dead_code)]
pub fn validate_timeout(timeout_secs: u64) -> Result<u64> {
    if timeout_secs == 0 {
        anyhow::bail!("Timeout must be greater than 0");
    }

    if timeout_secs > 3600 {
        anyhow::bail!("Timeout too large (max 3600 seconds / 1 hour)");
    }

    Ok(timeout_secs)
}

/// Parse memory string (e.g., "512M", "1G") to bytes
#[allow(dead_code)]
pub fn parse_memory_string(memory: &str) -> Result<u64> {
    let memory = memory.trim().to_uppercase();

    if memory.is_empty() {
        anyhow::bail!("Memory value cannot be empty");
    }

    let (value_str, unit) = if memory.ends_with("GB") || memory.ends_with("G") {
        let unit = if memory.ends_with("GB") { "GB" } else { "G" };
        (&memory[..memory.len() - unit.len()], 1024 * 1024 * 1024)
    } else if memory.ends_with("MB") || memory.ends_with("M") {
        let unit = if memory.ends_with("MB") { "MB" } else { "M" };
        (&memory[..memory.len() - unit.len()], 1024 * 1024)
    } else if memory.ends_with("KB") || memory.ends_with("K") {
        let unit = if memory.ends_with("KB") { "KB" } else { "K" };
        (&memory[..memory.len() - unit.len()], 1024)
    } else if memory.ends_with("B") {
        (&memory[..memory.len() - 1], 1)
    } else {
        // Assume bytes if no unit
        (memory.as_str(), 1)
    };

    let value: u64 = value_str
        .parse()
        .with_context(|| format!("Invalid memory value: {}", value_str))?;

    if value == 0 {
        anyhow::bail!("Memory value must be greater than 0");
    }

    let bytes = value.saturating_mul(unit);

    // Sanity check - minimum 1MB, maximum 1TB
    if bytes < 1024 * 1024 {
        anyhow::bail!("Memory limit too small (minimum 1MB)");
    }

    if bytes > 1024u64 * 1024 * 1024 * 1024 {
        anyhow::bail!("Memory limit too large (maximum 1TB)");
    }

    Ok(bytes)
}

/// Validate CPU percentage
#[allow(dead_code)]
pub fn validate_cpu_percent(cpu: f32) -> Result<f32> {
    if cpu <= 0.0 {
        anyhow::bail!("CPU percentage must be greater than 0");
    }

    if cpu > 100.0 * num_cpus::get() as f32 {
        anyhow::bail!("CPU percentage exceeds available CPU cores");
    }

    Ok(cpu)
}

/// Format bytes to human-readable string
#[allow(dead_code)]
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];

    if bytes == 0 {
        return "0 B".to_string();
    }

    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if size.fract() == 0.0 {
        format!("{:.0} {}", size, UNITS[unit_idx])
    } else {
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}

/// Format duration to human-readable string
#[allow(dead_code)]
pub fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_app_name() {
        assert!(validate_app_name("myapp").is_ok());
        assert!(validate_app_name("my-app").is_ok());
        assert!(validate_app_name("my_app").is_ok());
        assert!(validate_app_name("app123").is_ok());

        assert!(validate_app_name("").is_err());
        assert!(validate_app_name("my/app").is_err());
        assert!(validate_app_name("my\\app").is_err());
        assert!(validate_app_name("my:app").is_err());
        assert!(validate_app_name(&"a".repeat(256)).is_err());
    }

    #[test]
    fn test_parse_memory_string() {
        assert_eq!(
            parse_memory_string("512M").expect("Failed to parse 512M"),
            512 * 1024 * 1024
        );
        assert_eq!(
            parse_memory_string("1G").expect("Failed to parse 1G"),
            1024 * 1024 * 1024
        );
        assert_eq!(
            parse_memory_string("2GB").expect("Failed to parse 2GB"),
            2 * 1024 * 1024 * 1024
        );
        assert_eq!(
            parse_memory_string("100MB").expect("Failed to parse 100MB"),
            100 * 1024 * 1024
        );
        assert_eq!(
            parse_memory_string("1024K").expect("Failed to parse 1024K"),
            1024 * 1024
        );

        assert!(parse_memory_string("").is_err());
        assert!(parse_memory_string("0M").is_err());
        assert!(parse_memory_string("100B").is_err()); // Too small
        assert!(parse_memory_string("2000G").is_err()); // Too large
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1048576), "1 MB");
        assert_eq!(format_bytes(1073741824), "1 GB");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(59), "59s");
        assert_eq!(format_duration(60), "1m 0s");
        assert_eq!(format_duration(125), "2m 5s");
        assert_eq!(format_duration(3600), "1h 0m");
        assert_eq!(format_duration(3665), "1h 1m");
        assert_eq!(format_duration(86400), "1d 0h");
        assert_eq!(format_duration(90061), "1d 1h");
    }

    #[test]
    fn test_validate_timeout() {
        assert_eq!(
            validate_timeout(10).expect("Failed to validate timeout 10"),
            10
        );
        assert_eq!(
            validate_timeout(3600).expect("Failed to validate timeout 3600"),
            3600
        );

        assert!(validate_timeout(0).is_err());
        assert!(validate_timeout(3601).is_err());
    }

    #[test]
    fn test_validate_cpu_percent() {
        assert!(validate_cpu_percent(50.0).is_ok());
        assert!(validate_cpu_percent(100.0).is_ok());

        assert!(validate_cpu_percent(0.0).is_err());
        assert!(validate_cpu_percent(-10.0).is_err());
        // Note: upper bound depends on number of CPUs
    }
}
