use bunctl_core::{
    App, AppConfig, AppId, AppState, ConfigWatcher, ProcessSupervisor, SupervisorEvent,
};
use bunctl_ipc::{IpcConnection, IpcMessage, IpcResponse, IpcServer, SubscriptionType};
use bunctl_logging::{LogConfig, LogManager};
use bunctl_supervisor::create_supervisor;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time;
use tracing::{debug, error, info, warn};

use crate::cli::DaemonArgs;

#[derive(Debug)]
struct Subscriber {
    id: u64,
    subscription: SubscriptionType,
    sender: mpsc::UnboundedSender<IpcResponse>,
}

impl Subscriber {
    fn should_receive_event(&self, event_type: &str, app_name: &Option<AppId>) -> bool {
        match &self.subscription {
            SubscriptionType::AllEvents { app_name: filter } => filter
                .as_ref()
                .map(|f| app_name.as_ref().map(|a| a.to_string()) == Some(f.clone()))
                .unwrap_or(true),
            SubscriptionType::StatusEvents { app_name: filter } => {
                if !matches!(
                    event_type,
                    "status_change"
                        | "process_started"
                        | "process_exited"
                        | "process_crashed"
                        | "process_restarting"
                ) {
                    return false;
                }
                filter
                    .as_ref()
                    .map(|f| app_name.as_ref().map(|a| a.to_string()) == Some(f.clone()))
                    .unwrap_or(true)
            }
            SubscriptionType::LogEvents { app_name: filter } => {
                if event_type != "log_line" {
                    return false;
                }
                filter
                    .as_ref()
                    .map(|f| app_name.as_ref().map(|a| a.to_string()) == Some(f.clone()))
                    .unwrap_or(true)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStatus {
    pub name: String,
    pub state: String,
    pub pid: Option<u32>,
    pub restarts: u32,
    pub auto_start: bool,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub restart_policy: String,
    pub last_exit_code: Option<i32>,
    pub uptime_seconds: Option<u64>,
    pub max_memory: Option<u64>,
    pub max_cpu_percent: Option<f32>,
    pub max_restart_attempts: Option<u32>,
    pub backoff_exhausted: bool,
    pub env: HashMap<String, String>,
    pub memory_bytes: Option<u64>,
    pub cpu_percent: Option<f64>,
}

impl AppStatus {
    pub async fn from_app_and_supervisor(
        app: &App,
        config: &AppConfig,
        supervisor: &Arc<dyn ProcessSupervisor>,
    ) -> Self {
        let mut status = Self {
            name: app.id.to_string(),
            state: format!("{:?}", app.get_state()),
            pid: app.get_pid(),
            restarts: *app.restart_count.read(),
            auto_start: config.auto_start,
            command: config.command.clone(),
            args: config.args.clone(),
            cwd: config.cwd.clone(),
            restart_policy: format!("{:?}", config.restart_policy),
            last_exit_code: *app.last_exit_code.read(),
            uptime_seconds: app.uptime().map(|d| d.as_secs()),
            max_memory: config.max_memory,
            max_cpu_percent: config.max_cpu_percent,
            max_restart_attempts: config.backoff.max_attempts,
            backoff_exhausted: app.is_backoff_exhausted(),
            env: HashMap::new(),
            memory_bytes: None,
            cpu_percent: None,
        };

        // Add key environment variables
        for (key, value) in &config.env {
            if key.starts_with("PORT")
                || key.starts_with("NODE_ENV")
                || key.starts_with("DATABASE")
                || key.starts_with("REDIS")
                || key.contains("URL")
                || key.contains("HOST")
            {
                status.env.insert(key.clone(), value.clone());
            }
        }

        // Add process info if running
        if let Some(pid) = app.get_pid()
            && let Ok(process_info) = supervisor.get_process_info(pid).await
        {
            status.memory_bytes = process_info.memory_bytes;
            status.cpu_percent = process_info.cpu_percent.map(|f| f as f64);
        }

        status
    }
}

pub struct Daemon {
    supervisor: Arc<dyn ProcessSupervisor>,
    apps: Arc<DashMap<AppId, Arc<App>>>,
    log_manager: Arc<LogManager>,
    config_watcher: Option<ConfigWatcher>,
    ipc_server: Option<IpcServer>,
    subscribers: Arc<DashMap<u64, Subscriber>>,
    next_subscriber_id: Arc<std::sync::atomic::AtomicU64>,
}

impl Daemon {
    fn broadcast_event(&self, event_type: &str, app_name: Option<&AppId>, data: serde_json::Value) {
        Self::broadcast_event_static(&self.subscribers, event_type, app_name, data);
    }

    fn broadcast_event_static(
        subscribers: &Arc<DashMap<u64, Subscriber>>,
        event_type: &str,
        app_name: Option<&AppId>,
        data: serde_json::Value,
    ) {
        let event_response = IpcResponse::Event {
            event_type: event_type.to_string(),
            data,
        };

        // Remove disconnected subscribers while broadcasting
        let mut to_remove = Vec::new();

        for subscriber in subscribers.iter() {
            if subscriber.should_receive_event(event_type, &app_name.cloned())
                && subscriber.sender.send(event_response.clone()).is_err()
            {
                to_remove.push(subscriber.id);
            }
        }

        // Clean up disconnected subscribers
        for id in to_remove {
            subscribers.remove(&id);
        }
    }

    pub async fn new(args: DaemonArgs) -> anyhow::Result<Self> {
        let supervisor = create_supervisor().await?;

        let base_dir = if cfg!(windows) {
            // Use AppData/Roaming for user logs on Windows
            if let Some(appdata) = std::env::var_os("APPDATA") {
                PathBuf::from(appdata).join("bunctl").join("logs")
            } else {
                PathBuf::from("C:\\ProgramData\\bunctl\\logs")
            }
        } else {
            PathBuf::from("/var/log/bunctl")
        };

        // Ensure log directory exists
        if let Err(e) = std::fs::create_dir_all(&base_dir) {
            warn!("Failed to create log directory {:?}: {}", base_dir, e);
        }

        let log_config = LogConfig {
            base_dir,
            max_file_size: 50 * 1024 * 1024,
            max_files: 10,
            compression: true,
            buffer_size: 16384,
            flush_interval_ms: 100,
        };

        let log_manager = Arc::new(LogManager::new(log_config));

        let config_watcher = if let Some(config_path) = args.config {
            Some(ConfigWatcher::new(config_path).await?)
        } else {
            None
        };

        let ipc_server = if let Some(ref socket_path) = args.socket {
            Some(IpcServer::bind(socket_path).await?)
        } else {
            let default_socket = if cfg!(windows) {
                PathBuf::from("bunctl")
            } else {
                bunctl_core::config::default_socket_path()
            };
            Some(IpcServer::bind(default_socket).await?)
        };

        Ok(Self {
            supervisor,
            apps: Arc::new(DashMap::new()),
            log_manager,
            config_watcher,
            ipc_server,
            subscribers: Arc::new(DashMap::new()),
            next_subscriber_id: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        })
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        info!("Bunctl daemon starting...");
        debug!("Setting up main event loop");

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);

        Self::setup_signal_handlers(shutdown_tx.clone());

        if let Some(ref watcher) = self.config_watcher {
            let config = watcher.get();
            debug!(
                "Found {} apps in config for auto-start evaluation",
                config.apps.len()
            );
            for app_config in &config.apps {
                if app_config.auto_start {
                    debug!("Auto-starting app: {}", app_config.name);
                    self.start_app(app_config.clone()).await?;
                } else {
                    debug!("App {} has auto_start=false, skipping", app_config.name);
                }
            }
        } else {
            debug!("No config watcher available, skipping auto-start");
        }

        let mut events = self.supervisor.events();
        let _config_check_interval = time::interval(Duration::from_secs(5));
        let _health_check_interval = time::interval(Duration::from_secs(30));

        debug!("Event loop initialized with intervals - config check: 5s, health check: 30s");

        // IPC server ENABLED temporarily to reproduce issue
        if let Some(mut ipc_server) = self.ipc_server.take() {
            debug!("Starting IPC server task");
            let apps = self.apps.clone();
            let supervisor = self.supervisor.clone();
            let log_manager = self.log_manager.clone();
            let subscribers = self.subscribers.clone();
            let next_subscriber_id = self.next_subscriber_id.clone();

            tokio::spawn(async move {
                debug!("IPC server task started, waiting for connections");
                loop {
                    match ipc_server.accept().await {
                        Ok(connection) => {
                            debug!("Accepted new IPC connection");
                            let apps = apps.clone();
                            let supervisor = supervisor.clone();
                            let log_manager = log_manager.clone();
                            let subscribers = subscribers.clone();
                            let next_subscriber_id = next_subscriber_id.clone();

                            tokio::spawn(async move {
                                debug!("Spawning connection handler task");
                                Self::handle_ipc_connection(
                                    connection,
                                    apps,
                                    supervisor,
                                    log_manager,
                                    subscribers,
                                    next_subscriber_id,
                                )
                                .await;
                                debug!("Connection handler task completed");
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept IPC connection: {}", e);
                            break;
                        }
                    }
                }
                debug!("IPC server task terminated");
            });
        } else {
            debug!("No IPC server configured");
        }

        debug!("Entering FIXED event loop (capture tasks disabled, supervisor events enabled)");
        loop {
            tokio::select! {
                // ENABLED: Supervisor events (needed for process monitoring)
                Some(event) = events.recv() => {
                    debug!("Received supervisor event: {:?}", event);
                    self.handle_supervisor_event(event).await;
                    debug!("Finished handling supervisor event");
                }

                // DISABLED: All intervals (not needed for zero-overhead operation)
                // _ = config_check_interval.tick(), if !self.apps.is_empty() => { ... }
                // _ = health_check_interval.tick(), if !self.apps.is_empty() => { ... }

                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received");
                    debug!("Breaking out of main event loop");
                    break;
                }
            }
            debug!("Fixed event loop iteration completed");
        }

        self.shutdown().await?;
        Ok(())
    }

    async fn start_app(&self, config: AppConfig) -> anyhow::Result<()> {
        let app_id = AppId::new(&config.name)?;
        debug!(
            "Starting app: {} with command: {} {:?}",
            app_id, config.command, config.args
        );

        if self.apps.contains_key(&app_id) {
            warn!("App {} is already managed", app_id);
            debug!("App {} already exists in apps registry, skipping", app_id);
            return Ok(());
        }

        debug!("Creating new app instance for: {}", app_id);
        let app = Arc::new(App::new(app_id.clone(), config.clone()));
        self.apps.insert(app_id.clone(), app.clone());
        debug!(
            "App {} added to registry, total apps: {}",
            app_id,
            self.apps.len()
        );

        debug!("Setting app {} state to Starting", app_id);
        app.set_state(AppState::Starting);

        debug!("Spawning process for app: {}", app_id);
        let handle = self.supervisor.spawn(&config).await?;
        let pid = handle.pid;
        debug!(
            "Process spawned successfully for app: {} with PID: {}",
            app_id, pid
        );

        app.set_pid(Some(pid));
        app.set_state(AppState::Running);
        debug!("App {} state set to Running", app_id);

        info!("Started app {} with PID {}", app_id, pid);

        let supervisor = self.supervisor.clone();
        let app_clone = app.clone();
        let log_manager = self.log_manager.clone();
        let subscribers = self.subscribers.clone();

        debug!("Setting up monitoring for app: {}", app_id);
        Self::monitor_app_with_subscribers(
            app_clone,
            handle,
            supervisor,
            log_manager,
            subscribers,
            self.apps.clone(),
        );
        debug!("App {} monitoring setup completed", app_id);

        Ok(())
    }

    fn monitor_app_with_subscribers(
        app: Arc<App>,
        mut handle: bunctl_core::ProcessHandle,
        supervisor: Arc<dyn ProcessSupervisor>,
        log_manager: Arc<LogManager>,
        subscribers: Arc<DashMap<u64, Subscriber>>,
        apps: Arc<DashMap<AppId, Arc<App>>>,
    ) {
        debug!("monitor_app_with_subscribers starting for app: {}", app.id);

        // NEW APPROACH: Output should be redirected at spawn time, not captured here
        debug!(
            "Using OS-level redirection instead of capture tasks for app: {}",
            app.id
        );

        // These should be None if redirection was configured properly at spawn
        let stdout = handle.take_stdout();
        let stderr = handle.take_stderr();

        if stdout.is_some() || stderr.is_some() {
            debug!(
                "WARNING: app {} still has stdout/stderr pipes - redirection not configured",
                app.id
            );
        } else {
            debug!(
                "SUCCESS: app {} has no pipes - output is redirected to files",
                app.id
            );
        }

        let subscribers = subscribers.clone();
        let app_id = app.id.clone(); // Fix: was missing this line
        debug!("Spawning main monitoring task for app: {}", app_id);
        tokio::spawn(async move {
            debug!("monitor_app_impl task started for app: {}", app.id);
            Self::monitor_app_impl(
                app.clone(),
                handle,
                supervisor,
                log_manager,
                subscribers,
                apps,
            )
            .await;
            debug!("monitor_app_impl task completed for app: {}", app.id);
        });
    }

    fn monitor_app(
        app: Arc<App>,
        mut handle: bunctl_core::ProcessHandle,
        supervisor: Arc<dyn ProcessSupervisor>,
        log_manager: Arc<LogManager>,
    ) {
        // Take stdout and stderr for log capture
        let stdout = handle.take_stdout();
        let stderr = handle.take_stderr();
        let app_id = app.id.clone();
        let log_manager_clone = log_manager.clone();

        // Spawn tasks to capture output (without event broadcasting)
        if let Some(stdout) = stdout {
            let app_id = app_id.clone();
            let log_manager = log_manager_clone.clone();
            tokio::spawn(async move {
                Self::capture_output_simple(stdout, app_id, log_manager, "stdout").await;
            });
        }

        if let Some(stderr) = stderr {
            let app_id = app_id.clone();
            let log_manager = log_manager_clone.clone();
            tokio::spawn(async move {
                Self::capture_output_simple(stderr, app_id, log_manager, "stderr").await;
            });
        }

        // Spawn task to reset backoff after stable runtime
        let app_for_backoff_reset = app.clone();
        tokio::spawn(async move {
            // Wait for 10 seconds of stable runtime
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;

            // Check if process is still running after 10 seconds
            if matches!(app_for_backoff_reset.get_state(), AppState::Running) {
                app_for_backoff_reset.reset_backoff();
                info!(
                    "Reset backoff for app {} after 10 seconds of stable runtime",
                    app_for_backoff_reset.id
                );
            }
        });

        tokio::spawn(async move {
            Self::monitor_app_impl_simple(app, handle, supervisor, log_manager).await;
        });
    }

    async fn capture_output_simple<R>(
        reader: R,
        app_id: AppId,
        log_manager: Arc<LogManager>,
        stream_type: &str,
    ) where
        R: tokio::io::AsyncRead + Unpin,
    {
        use tokio::io::{AsyncBufReadExt, BufReader};

        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        let writer = match log_manager.get_writer(&app_id).await {
            Ok(w) => w,
            Err(e) => {
                error!("Failed to get log writer for {}: {}", app_id, e);
                return;
            }
        };

        loop {
            line.clear();

            // Add timeout to prevent infinite loops on broken pipes
            let read_result =
                tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut line)).await;

            match read_result {
                Ok(Ok(0)) => {
                    debug!("EOF reached for {} stream of {}", stream_type, app_id);
                    break; // EOF
                }
                Ok(Ok(_)) => {
                    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
                    let trimmed_line = line.trim_end();

                    // Skip completely empty lines to reduce noise
                    if !trimmed_line.is_empty() {
                        let log_line = format!(
                            "[{}] [{}] [{}] {}",
                            app_id, timestamp, stream_type, trimmed_line
                        );
                        if let Err(e) = writer.write_line(&log_line) {
                            error!("Failed to write log for {}: {}", app_id, e);
                        }
                    }
                }
                Ok(Err(e)) => {
                    error!("Error reading {} from {}: {}", stream_type, app_id, e);
                    break;
                }
                Err(_) => {
                    debug!(
                        "Timeout reading {} from {} - terminating capture task",
                        stream_type, app_id
                    );
                    break;
                }
            }
        }
    }

    async fn monitor_app_impl(
        app: Arc<App>,
        mut handle: bunctl_core::ProcessHandle,
        supervisor: Arc<dyn ProcessSupervisor>,
        log_manager: Arc<LogManager>,
        subscribers: Arc<DashMap<u64, Subscriber>>,
        apps: Arc<DashMap<AppId, Arc<App>>>,
    ) {
        debug!(
            "monitor_app_impl waiting for process to exit for app: {}",
            app.id
        );
        let status = match handle.wait().await {
            Ok(status) => {
                debug!(
                    "Process for app {} exited with status: {:?}",
                    app.id, status
                );
                status
            }
            Err(e) => {
                error!("Failed to wait for app {}: {}", app.id, e);
                debug!(
                    "Returning early from monitor_app_impl due to wait error for app: {}",
                    app.id
                );
                return;
            }
        };

        debug!("Updating app {} exit code to: {:?}", app.id, status.code());
        *app.last_exit_code.write() = status.code();
        app.set_pid(None);
        debug!("Cleared PID for app: {}", app.id);

        // Broadcast process exited event
        Self::broadcast_event_static(
            &subscribers,
            "process_exited",
            Some(&app.id),
            serde_json::json!({
                "app": app.id.to_string(),
                "exit_code": status.code()
            }),
        );

        let config = app.config.read().clone();
        if status.should_restart(config.restart_policy) {
            app.set_state(AppState::Crashed);

            // Broadcast crashed state
            Self::broadcast_event_static(
                &subscribers,
                "status_change",
                Some(&app.id),
                serde_json::json!({
                    "app": app.id.to_string(),
                    "state": "crashed"
                }),
            );

            // Get the persistent backoff strategy (or create a new one if this is the first crash)
            let mut backoff = app.get_or_create_backoff(&config);

            info!(
                "App {} backoff state: attempt={}, exhausted={}",
                app.id,
                backoff.attempt(),
                backoff.is_exhausted()
            );

            // Check if backoff is already exhausted before entering the loop
            if backoff.is_exhausted() {
                warn!("Backoff already exhausted for app {}", app.id);
                app.set_state(AppState::Stopped);

                // Broadcast backoff exhausted event
                Self::broadcast_event_static(
                    &subscribers,
                    "status_change",
                    Some(&app.id),
                    serde_json::json!({
                        "app": app.id.to_string(),
                        "state": "backoff_exhausted"
                    }),
                );
                return;
            }

            while let Some(delay) = backoff.next_delay() {
                let attempt = backoff.attempt();
                app.set_state(AppState::Backoff {
                    attempt,
                    next_retry: std::time::Instant::now() + delay,
                });

                // Update the persistent backoff state
                app.update_backoff(backoff.clone());

                // Broadcast restarting event
                Self::broadcast_event_static(
                    &subscribers,
                    "process_restarting",
                    Some(&app.id),
                    serde_json::json!({
                        "app": app.id.to_string(),
                        "attempt": attempt,
                        "delay_ms": delay.as_millis()
                    }),
                );

                tokio::time::sleep(delay).await;

                app.set_state(AppState::Starting);

                // Broadcast starting state
                Self::broadcast_event_static(
                    &subscribers,
                    "status_change",
                    Some(&app.id),
                    serde_json::json!({
                        "app": app.id.to_string(),
                        "state": "starting"
                    }),
                );
                match supervisor.spawn(&config).await {
                    Ok(new_handle) => {
                        let pid = new_handle.pid;
                        app.set_pid(Some(pid));
                        app.set_state(AppState::Running);
                        app.increment_restart_count();
                        info!("Restarted app {} with PID {}", app.id, pid);

                        // Broadcast process started event
                        Self::broadcast_event_static(
                            &subscribers,
                            "process_started",
                            Some(&app.id),
                            serde_json::json!({
                                "app": app.id.to_string(),
                                "pid": pid
                            }),
                        );

                        // Broadcast running state
                        Self::broadcast_event_static(
                            &subscribers,
                            "status_change",
                            Some(&app.id),
                            serde_json::json!({
                                "app": app.id.to_string(),
                                "state": "running"
                            }),
                        );

                        let app_clone = app.clone();
                        let supervisor_clone = supervisor.clone();
                        let log_manager_clone = log_manager.clone();

                        Self::monitor_app_with_subscribers(
                            app_clone,
                            new_handle,
                            supervisor_clone,
                            log_manager_clone,
                            subscribers.clone(),
                            apps.clone(),
                        );

                        return;
                    }
                    Err(e) => {
                        error!("Failed to restart app {}: {}", app.id, e);
                        // Update backoff state after failed spawn attempt
                        app.update_backoff(backoff.clone());
                    }
                }
            }

            warn!("Backoff exhausted for app {}", app.id);

            // Clean up log writer to prevent resource leak
            let _ = log_manager.remove_writer(&app.id).await;

            // Handle exhausted action based on configuration
            match config.backoff.exhausted_action {
                bunctl_core::config::ExhaustedAction::Stop => {
                    app.set_state(AppState::Stopped);

                    // Broadcast backoff exhausted event
                    Self::broadcast_event_static(
                        &subscribers,
                        "status_change",
                        Some(&app.id),
                        serde_json::json!({
                            "app": app.id.to_string(),
                            "state": "backoff_exhausted"
                        }),
                    );
                }
                bunctl_core::config::ExhaustedAction::Remove => {
                    app.set_state(AppState::Stopped);

                    // Broadcast app removal event
                    Self::broadcast_event_static(
                        &subscribers,
                        "status_change",
                        Some(&app.id),
                        serde_json::json!({
                            "app": app.id.to_string(),
                            "state": "removed",
                            "reason": "backoff_exhausted"
                        }),
                    );

                    // Remove app from registry
                    apps.remove(&app.id);
                    info!(
                        "App {} removed from registry due to backoff exhaustion",
                        app.id
                    );
                }
            }
        } else {
            app.set_state(AppState::Stopped);

            // Clean up log writer when app stops
            let _ = log_manager.remove_writer(&app.id).await;

            // Broadcast stopped state
            Self::broadcast_event_static(
                &subscribers,
                "status_change",
                Some(&app.id),
                serde_json::json!({
                    "app": app.id.to_string(),
                    "state": "stopped"
                }),
            );
        }
    }

    async fn monitor_app_impl_simple(
        app: Arc<App>,
        mut handle: bunctl_core::ProcessHandle,
        supervisor: Arc<dyn ProcessSupervisor>,
        log_manager: Arc<LogManager>,
    ) {
        let status = match handle.wait().await {
            Ok(status) => status,
            Err(e) => {
                error!("Failed to wait for app {}: {}", app.id, e);
                return;
            }
        };

        *app.last_exit_code.write() = status.code();
        app.set_pid(None);

        let config = app.config.read().clone();
        if status.should_restart(config.restart_policy) {
            app.set_state(AppState::Crashed);

            // Get or create persistent backoff strategy from the app
            let mut backoff = app.get_or_create_backoff(&config);

            // Check if backoff is already exhausted before entering the loop
            if backoff.is_exhausted() {
                warn!("Backoff already exhausted for app {}", app.id);
                app.set_state(AppState::Stopped);
                return;
            }

            while let Some(delay) = backoff.next_delay() {
                app.set_state(AppState::Backoff {
                    attempt: backoff.attempt(),
                    next_retry: std::time::Instant::now() + delay,
                });

                tokio::time::sleep(delay).await;

                app.set_state(AppState::Starting);
                match supervisor.spawn(&config).await {
                    Ok(new_handle) => {
                        let pid = new_handle.pid;
                        app.set_pid(Some(pid));
                        app.set_state(AppState::Running);
                        app.increment_restart_count();
                        info!("Restarted app {} with PID {}", app.id, pid);

                        // Don't reset backoff immediately - let it persist until process proves stable
                        // Update backoff state to track this spawn attempt
                        app.update_backoff(backoff.clone());

                        let app_clone = app.clone();
                        let supervisor_clone = supervisor.clone();
                        let log_manager_clone = log_manager.clone();

                        Self::monitor_app(
                            app_clone,
                            new_handle,
                            supervisor_clone,
                            log_manager_clone,
                        );

                        return;
                    }
                    Err(e) => {
                        error!("Failed to restart app {}: {}", app.id, e);
                        // Update persistent backoff state after failed spawn attempt
                        app.update_backoff(backoff.clone());
                    }
                }
            }

            warn!("Backoff exhausted for app {}", app.id);
            app.set_state(AppState::Stopped);

            // Clean up log writer to prevent resource leak
            let _ = log_manager.remove_writer(&app.id).await;
        } else {
            app.set_state(AppState::Stopped);

            // Clean up log writer when app stops
            let _ = log_manager.remove_writer(&app.id).await;
        }
    }

    async fn handle_supervisor_event(&self, event: SupervisorEvent) {
        debug!("Processing supervisor event: {:?}", event);
        match event.clone() {
            SupervisorEvent::ProcessStarted { app, pid } => {
                info!("Process {} started with PID {}", app, pid);
                debug!("Broadcasting process_started event for app: {}", app);
                self.broadcast_event(
                    "process_started",
                    Some(&app),
                    serde_json::json!({
                        "app": app.to_string(),
                        "pid": pid
                    }),
                );
                debug!("process_started event broadcast completed");
            }
            SupervisorEvent::ProcessExited { app, status } => {
                info!("Process {} exited with status {:?}", app, status);
                debug!(
                    "Broadcasting process_exited event for app: {}, exit_code: {:?}",
                    app,
                    status.code()
                );
                self.broadcast_event(
                    "process_exited",
                    Some(&app),
                    serde_json::json!({
                        "app": app.to_string(),
                        "exit_code": status.code()
                    }),
                );
                debug!("process_exited event broadcast completed");
            }
            SupervisorEvent::ProcessCrashed { app, reason } => {
                error!("Process {} crashed: {}", app, reason);
                debug!(
                    "Broadcasting process_crashed event for app: {}, reason: {}",
                    app, reason
                );
                self.broadcast_event(
                    "process_crashed",
                    Some(&app),
                    serde_json::json!({
                        "app": app.to_string(),
                        "reason": reason
                    }),
                );
                debug!("process_crashed event broadcast completed");
            }
            SupervisorEvent::ProcessRestarting {
                app,
                attempt,
                delay,
            } => {
                info!("Restarting {} (attempt {}) after {:?}", app, attempt, delay);
                self.broadcast_event(
                    "process_restarting",
                    Some(&app),
                    serde_json::json!({
                        "app": app.to_string(),
                        "attempt": attempt,
                        "delay_ms": delay.as_millis()
                    }),
                );
            }
            SupervisorEvent::BackoffExhausted { app } => {
                warn!("Backoff exhausted for {}", app);
                self.broadcast_event(
                    "status_change",
                    Some(&app),
                    serde_json::json!({
                        "app": app.to_string(),
                        "state": "backoff_exhausted"
                    }),
                );
            }
            SupervisorEvent::HealthCheckFailed { app, reason } => {
                warn!("Health check failed for {}: {}", app, reason);
                self.broadcast_event(
                    "health_check_failed",
                    Some(&app),
                    serde_json::json!({
                        "app": app.to_string(),
                        "reason": reason
                    }),
                );
            }
            SupervisorEvent::ResourceLimitExceeded {
                app,
                resource,
                limit,
                current,
            } => {
                warn!(
                    "Resource limit exceeded for {}: {} (limit: {}, current: {})",
                    app, resource, limit, current
                );
                self.broadcast_event(
                    "resource_limit_exceeded",
                    Some(&app),
                    serde_json::json!({
                        "app": app.to_string(),
                        "resource": resource,
                        "limit": limit,
                        "current": current
                    }),
                );
            }
            _ => {}
        }
    }

    #[allow(dead_code)]
    async fn reload_config(&self) -> anyhow::Result<()> {
        if let Some(ref watcher) = self.config_watcher {
            let config = watcher.get();

            for app_config in &config.apps {
                let app_id = AppId::new(&app_config.name)?;

                if let Some(app) = self.apps.get(&app_id) {
                    *app.config.write() = app_config.clone();
                } else if app_config.auto_start {
                    self.start_app(app_config.clone()).await?;
                }
            }
        }

        Ok(())
    }

    #[allow(dead_code)]
    async fn perform_health_checks(&self) {
        for app in self.apps.iter() {
            if app.get_state() == AppState::Running
                && let Some(pid) = app.get_pid()
            {
                match self.supervisor.get_process_info(pid).await {
                    Ok(info) => {
                        let config = app.config.read();
                        if let Some(max_memory) = config.max_memory
                            && let Some(memory) = info.memory_bytes
                            && memory > max_memory
                        {
                            warn!(
                                "App {} exceeds memory limit: {} > {}",
                                app.id, memory, max_memory
                            );
                        }
                    }
                    Err(e) => {
                        error!("Failed to get process info for {}: {}", app.id, e);
                    }
                }
            }
        }
    }

    fn setup_signal_handlers(shutdown_tx: mpsc::Sender<()>) {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};

            tokio::spawn(async move {
                let mut sigterm = signal(SignalKind::terminate()).unwrap();
                let mut sigint = signal(SignalKind::interrupt()).unwrap();

                tokio::select! {
                    _ = sigterm.recv() => {
                        info!("Received SIGTERM");
                    }
                    _ = sigint.recv() => {
                        info!("Received SIGINT");
                    }
                }

                let _ = shutdown_tx.send(()).await;
            });
        }
        #[cfg(windows)]
        {
            tokio::spawn(async move {
                // On Windows, use ctrl_c for graceful shutdown
                if (tokio::signal::ctrl_c().await).is_ok() {
                    info!("Received Ctrl+C signal");
                    let _ = shutdown_tx.send(()).await;
                }
            });
        }
    }

    async fn handle_ipc_connection(
        mut connection: IpcConnection,
        apps: Arc<DashMap<AppId, Arc<App>>>,
        supervisor: Arc<dyn ProcessSupervisor>,
        log_manager: Arc<LogManager>,
        subscribers: Arc<DashMap<u64, Subscriber>>,
        next_subscriber_id: Arc<std::sync::atomic::AtomicU64>,
    ) {
        let mut subscriber_id: Option<u64> = None;
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<IpcResponse>();

        loop {
            tokio::select! {
                msg_result = connection.recv() => {
                    match msg_result {
                        Ok(msg) => {
                            match msg {
                                IpcMessage::Subscribe { subscription } => {
                                    // Unsubscribe from previous subscription if any
                                    if let Some(id) = subscriber_id {
                                        subscribers.remove(&id);
                                    }

                                    let id = next_subscriber_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                    let subscriber = Subscriber {
                                        id,
                                        subscription,
                                        sender: event_tx.clone(),
                                    };

                                    subscribers.insert(id, subscriber);
                                    subscriber_id = Some(id);

                                    let response = IpcResponse::Success {
                                        message: "Subscribed to events".to_string(),
                                    };
                                    if (connection.send(&response).await).is_err() {
                                        break;
                                    }
                                }
                                IpcMessage::Unsubscribe => {
                                    if let Some(id) = subscriber_id {
                                        subscribers.remove(&id);
                                        subscriber_id = None;
                                    }

                                    let response = IpcResponse::Success {
                                        message: "Unsubscribed from events".to_string(),
                                    };
                                    if (connection.send(&response).await).is_err() {
                                        break;
                                    }
                                }
                                _ => {
                                    let response = Self::handle_ipc_message(msg, &apps, &supervisor, &log_manager, &subscribers).await;
                                    if (connection.send(&response).await).is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            // Check if this is just a client disconnect (EOF)
                            if e.to_string().contains("eof") || e.to_string().contains("EOF") {
                                tracing::debug!("IPC client disconnected normally");
                            } else {
                                error!("Failed to receive IPC message: {}", e);
                            }
                            break;
                        }
                    }
                }

                Some(event) = event_rx.recv() => {
                    // Forward event to the client
                    if (connection.send(&event).await).is_err() {
                        break;
                    }
                }
            }
        }

        // Clean up subscriber when connection is lost
        if let Some(id) = subscriber_id {
            subscribers.remove(&id);
        }
    }

    async fn handle_ipc_message(
        msg: IpcMessage,
        apps: &Arc<DashMap<AppId, Arc<App>>>,
        supervisor: &Arc<dyn ProcessSupervisor>,
        log_manager: &Arc<LogManager>,
        subscribers: &Arc<DashMap<u64, Subscriber>>,
    ) -> IpcResponse {
        match msg {
            IpcMessage::Start { name, config } => {
                match serde_json::from_str::<AppConfig>(&config) {
                    Ok(app_config) => {
                        match Self::start_app_static(
                            apps,
                            supervisor,
                            app_config,
                            Some(subscribers.clone()),
                        )
                        .await
                        {
                            Ok(_) => IpcResponse::Success {
                                message: format!("Started app {}", name),
                            },
                            Err(e) => IpcResponse::Error {
                                message: format!("Failed to start app {}: {}", name, e),
                            },
                        }
                    }
                    Err(e) => IpcResponse::Error {
                        message: format!("Invalid config: {}", e),
                    },
                }
            }
            IpcMessage::Stop { name } => match AppId::new(&name) {
                Ok(app_id) => {
                    if let Some(app) = apps.get(&app_id) {
                        if let Some(pid) = app.get_pid() {
                            app.set_state(AppState::Stopping);
                            let handle = bunctl_core::ProcessHandle {
                                pid,
                                app_id: app_id.clone(),
                                inner: None,
                                stdout: None,
                                stderr: None,
                            };
                            let stop_timeout = app.config.read().stop_timeout;
                            match supervisor
                                .graceful_stop(&mut handle.clone(), stop_timeout)
                                .await
                            {
                                Ok(_) => {
                                    app.set_state(AppState::Stopped);
                                    IpcResponse::Success {
                                        message: format!("Stopped app {}", name),
                                    }
                                }
                                Err(e) => IpcResponse::Error {
                                    message: format!("Failed to stop app {}: {}", name, e),
                                },
                            }
                        } else {
                            IpcResponse::Error {
                                message: format!("App {} is not running", name),
                            }
                        }
                    } else {
                        IpcResponse::Error {
                            message: format!("App {} not found", name),
                        }
                    }
                }
                Err(e) => IpcResponse::Error {
                    message: format!("Invalid app name: {}", e),
                },
            },
            IpcMessage::Status { name } => {
                let status = if let Some(name) = name {
                    match AppId::new(&name) {
                        Ok(app_id) => {
                            if let Some(app) = apps.get(&app_id) {
                                let config = app.config.read().clone();
                                let app_status =
                                    AppStatus::from_app_and_supervisor(&app, &config, supervisor)
                                        .await;
                                serde_json::to_value(app_status).unwrap()
                            } else {
                                return IpcResponse::Error {
                                    message: format!("App {} not found", name),
                                };
                            }
                        }
                        Err(e) => {
                            return IpcResponse::Error {
                                message: format!("Invalid app name: {}", e),
                            };
                        }
                    }
                } else {
                    let mut all_status = Vec::new();

                    for entry in apps.iter() {
                        let config = entry.config.read().clone();
                        let app_status =
                            AppStatus::from_app_and_supervisor(&entry, &config, supervisor).await;
                        all_status.push(serde_json::to_value(app_status).unwrap());
                    }
                    serde_json::json!(all_status)
                };
                IpcResponse::Data { data: status }
            }
            IpcMessage::Restart { name } => {
                match AppId::new(&name) {
                    Ok(app_id) => {
                        // First stop the app
                        if let Some(app) = apps.get(&app_id) {
                            if let Some(pid) = app.get_pid() {
                                app.set_state(AppState::Stopping);
                                let handle = bunctl_core::ProcessHandle {
                                    pid,
                                    app_id: app_id.clone(),
                                    inner: None,
                                    stdout: None,
                                    stderr: None,
                                };
                                let stop_timeout = app.config.read().stop_timeout;
                                let config = app.config.read().clone();

                                match supervisor
                                    .graceful_stop(&mut handle.clone(), stop_timeout)
                                    .await
                                {
                                    Ok(_) => {
                                        app.set_state(AppState::Stopped);
                                        // Now restart it
                                        app.set_state(AppState::Starting);
                                        match supervisor.spawn(&config).await {
                                            Ok(new_handle) => {
                                                let pid = new_handle.pid;
                                                app.set_pid(Some(pid));
                                                app.set_state(AppState::Running);
                                                app.increment_restart_count();
                                                IpcResponse::Success {
                                                    message: format!(
                                                        "Restarted app {} with PID {}",
                                                        name, pid
                                                    ),
                                                }
                                            }
                                            Err(e) => IpcResponse::Error {
                                                message: format!(
                                                    "Failed to restart app {}: {}",
                                                    name, e
                                                ),
                                            },
                                        }
                                    }
                                    Err(e) => IpcResponse::Error {
                                        message: format!("Failed to stop app {}: {}", name, e),
                                    },
                                }
                            } else {
                                // App is not running, just start it
                                let config = app.config.read().clone();
                                app.set_state(AppState::Starting);
                                match supervisor.spawn(&config).await {
                                    Ok(handle) => {
                                        let pid = handle.pid;
                                        app.set_pid(Some(pid));
                                        app.set_state(AppState::Running);
                                        IpcResponse::Success {
                                            message: format!(
                                                "Started app {} with PID {}",
                                                name, pid
                                            ),
                                        }
                                    }
                                    Err(e) => IpcResponse::Error {
                                        message: format!("Failed to start app {}: {}", name, e),
                                    },
                                }
                            }
                        } else {
                            IpcResponse::Error {
                                message: format!("App {} not found", name),
                            }
                        }
                    }
                    Err(e) => IpcResponse::Error {
                        message: format!("Invalid app name: {}", e),
                    },
                }
            }
            IpcMessage::List => {
                let app_list: Vec<String> = apps.iter().map(|entry| entry.id.to_string()).collect();
                IpcResponse::Data {
                    data: serde_json::json!(app_list),
                }
            }
            IpcMessage::Delete { name } => {
                match AppId::new(&name) {
                    Ok(app_id) => {
                        if let Some((_, app)) = apps.remove(&app_id) {
                            // Stop the app if it's running
                            if let Some(pid) = app.get_pid() {
                                app.set_state(AppState::Stopping);
                                let handle = bunctl_core::ProcessHandle {
                                    pid,
                                    app_id: app_id.clone(),
                                    inner: None,
                                    stdout: None,
                                    stderr: None,
                                };
                                let stop_timeout = app.config.read().stop_timeout;
                                let _ = supervisor
                                    .graceful_stop(&mut handle.clone(), stop_timeout)
                                    .await;
                            }
                            IpcResponse::Success {
                                message: format!("Deleted app {}", name),
                            }
                        } else {
                            IpcResponse::Error {
                                message: format!("App {} not found", name),
                            }
                        }
                    }
                    Err(e) => IpcResponse::Error {
                        message: format!("Invalid app name: {}", e),
                    },
                }
            }
            IpcMessage::Logs { name, lines } => {
                if let Some(name) = name {
                    // Single app logs
                    match AppId::new(&name) {
                        Ok(app_id) => {
                            if apps.contains_key(&app_id) {
                                // Try to read structured logs from the log manager
                                match log_manager.read_structured_logs(&app_id, lines).await {
                                    Ok(structured_logs) => IpcResponse::Data {
                                        data: serde_json::json!({
                                            "type": "single",
                                            "app": name,
                                            "logs": structured_logs
                                        }),
                                    },
                                    Err(e) => {
                                        tracing::debug!("Failed to read logs for {}: {}", name, e);
                                        IpcResponse::Data {
                                            data: serde_json::json!({
                                                "type": "single",
                                                "app": name,
                                                "logs": {
                                                    "errors": vec![format!("No logs available for {}", name)],
                                                    "output": Vec::<String>::new()
                                                }
                                            }),
                                        }
                                    }
                                }
                            } else {
                                IpcResponse::Error {
                                    message: format!("App {} not found", name),
                                }
                            }
                        }
                        Err(e) => IpcResponse::Error {
                            message: format!("Invalid app name: {}", e),
                        },
                    }
                } else {
                    // All apps logs
                    match log_manager.read_all_apps_logs(lines).await {
                        Ok(all_logs) => IpcResponse::Data {
                            data: serde_json::json!({
                                "type": "all",
                                "apps": all_logs
                            }),
                        },
                        Err(e) => {
                            tracing::debug!("Failed to read all logs: {}", e);
                            IpcResponse::Data {
                                data: serde_json::json!({
                                    "type": "all",
                                    "apps": Vec::<(String, serde_json::Value)>::new()
                                }),
                            }
                        }
                    }
                }
            }
            IpcMessage::Subscribe { .. } | IpcMessage::Unsubscribe => {
                // These should not reach this function as they are handled in handle_ipc_connection
                IpcResponse::Error {
                    message: "Subscription commands should not reach this handler".to_string(),
                }
            }
        }
    }

    async fn start_app_static(
        apps: &Arc<DashMap<AppId, Arc<App>>>,
        supervisor: &Arc<dyn ProcessSupervisor>,
        config: AppConfig,
        subscribers: Option<Arc<DashMap<u64, Subscriber>>>,
    ) -> anyhow::Result<()> {
        let app_id = AppId::new(&config.name)?;

        if apps.contains_key(&app_id) {
            warn!("App {} is already managed", app_id);
            return Ok(());
        }

        let app = Arc::new(App::new(app_id.clone(), config.clone()));
        apps.insert(app_id.clone(), app.clone());

        app.set_state(AppState::Starting);

        let handle = supervisor.spawn(&config).await?;
        let pid = handle.pid;
        app.set_pid(Some(pid));
        app.set_state(AppState::Running);

        info!("Started app {} with PID {}", app_id, pid);

        // Set up monitoring with a minimal log manager for static context
        let base_dir = if cfg!(windows) {
            if let Some(appdata) = std::env::var_os("APPDATA") {
                PathBuf::from(appdata).join("bunctl").join("logs")
            } else {
                PathBuf::from("C:\\ProgramData\\bunctl\\logs")
            }
        } else {
            PathBuf::from("/var/log/bunctl")
        };

        if let Err(e) = std::fs::create_dir_all(&base_dir) {
            warn!("Failed to create log directory {:?}: {}", base_dir, e);
        }

        let log_config = LogConfig {
            base_dir,
            max_file_size: 50 * 1024 * 1024,
            max_files: 10,
            compression: true,
            buffer_size: 16384,
            flush_interval_ms: 100,
        };
        let log_manager = Arc::new(LogManager::new(log_config));

        let app_clone = app.clone();
        let supervisor_clone = supervisor.clone();
        let log_manager_clone = log_manager.clone();

        if let Some(subscribers) = subscribers {
            Self::monitor_app_with_subscribers(
                app_clone,
                handle,
                supervisor_clone,
                log_manager_clone,
                subscribers,
                apps.clone(),
            );
        } else {
            Self::monitor_app(app_clone, handle, supervisor_clone, log_manager_clone);
        }

        Ok(())
    }

    async fn shutdown(&mut self) -> anyhow::Result<()> {
        info!("Shutting down daemon...");

        for app in self.apps.iter() {
            if let Some(pid) = app.get_pid() {
                info!("Stopping app {} (PID {})", app.id, pid);
                app.set_state(AppState::Stopping);

                let handle = bunctl_core::ProcessHandle {
                    pid,
                    app_id: app.id.clone(),
                    inner: None,
                    stdout: None,
                    stderr: None,
                };

                let stop_timeout = app.config.read().stop_timeout;
                match self
                    .supervisor
                    .graceful_stop(&mut handle.clone(), stop_timeout)
                    .await
                {
                    Ok(_) => info!("App {} stopped gracefully", app.id),
                    Err(e) => error!("Failed to stop app {}: {}", app.id, e),
                }
            }
        }

        self.log_manager.flush_all().await?;
        info!("Daemon shutdown complete");
        Ok(())
    }
}

pub async fn run(args: DaemonArgs) -> anyhow::Result<()> {
    let daemon = Daemon::new(args).await?;
    daemon.run().await
}
