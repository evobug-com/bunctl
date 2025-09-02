use bunctl_core::{
    App, AppConfig, AppId, AppState, BackoffStrategy, ConfigWatcher, ProcessSupervisor,
    SupervisorEvent,
};
use bunctl_logging::{LogConfig, LogManager};
use bunctl_supervisor::create_supervisor;
use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time;
use tracing::{error, info, warn};

use crate::cli::DaemonArgs;

pub struct Daemon {
    supervisor: Arc<dyn ProcessSupervisor>,
    apps: Arc<DashMap<AppId, Arc<App>>>,
    log_manager: Arc<LogManager>,
    config_watcher: Option<ConfigWatcher>,
}

impl Daemon {
    pub async fn new(args: DaemonArgs) -> anyhow::Result<Self> {
        let supervisor = create_supervisor().await?;

        let log_config = LogConfig {
            base_dir: PathBuf::from("/var/log/bunctl"),
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

        Ok(Self {
            supervisor,
            apps: Arc::new(DashMap::new()),
            log_manager,
            config_watcher,
        })
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        info!("Bunctl daemon starting...");

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);

        Self::setup_signal_handlers(shutdown_tx.clone());

        if let Some(ref watcher) = self.config_watcher {
            let config = watcher.get();
            for app_config in &config.apps {
                if app_config.auto_start {
                    self.start_app(app_config.clone()).await?;
                }
            }
        }

        let mut events = self.supervisor.events();
        let mut config_check_interval = time::interval(Duration::from_secs(5));
        let mut health_check_interval = time::interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                Some(event) = events.recv() => {
                    self.handle_supervisor_event(event).await;
                }

                _ = config_check_interval.tick() => {
                    if let Some(ref watcher) = self.config_watcher
                        && watcher.check_reload().await? {
                            info!("Configuration reloaded");
                            self.reload_config().await?;
                        }
                }

                _ = health_check_interval.tick() => {
                    self.perform_health_checks().await;
                }

                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received");
                    break;
                }
            }
        }

        self.shutdown().await?;
        Ok(())
    }

    async fn start_app(&self, config: AppConfig) -> anyhow::Result<()> {
        let app_id = AppId::new(&config.name)?;

        if self.apps.contains_key(&app_id) {
            warn!("App {} is already managed", app_id);
            return Ok(());
        }

        let app = Arc::new(App::new(app_id.clone(), config.clone()));
        self.apps.insert(app_id.clone(), app.clone());

        app.set_state(AppState::Starting);

        let handle = self.supervisor.spawn(&config).await?;
        app.set_pid(Some(handle.pid));
        app.set_state(AppState::Running);

        info!("Started app {} with PID {}", app_id, handle.pid);

        let supervisor = self.supervisor.clone();
        let app_clone = app.clone();
        let log_manager = self.log_manager.clone();

        Self::monitor_app(app_clone, handle, supervisor, log_manager);

        Ok(())
    }

    fn monitor_app(
        app: Arc<App>,
        handle: bunctl_core::ProcessHandle,
        supervisor: Arc<dyn ProcessSupervisor>,
        log_manager: Arc<LogManager>,
    ) {
        tokio::spawn(async move {
            Self::monitor_app_impl(app, handle, supervisor, log_manager).await;
        });
    }

    async fn monitor_app_impl(
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

            let mut backoff = BackoffStrategy::new()
                .with_base_delay(Duration::from_millis(config.backoff.base_delay_ms))
                .with_max_delay(Duration::from_millis(config.backoff.max_delay_ms))
                .with_multiplier(config.backoff.multiplier)
                .with_jitter(config.backoff.jitter);

            if let Some(max) = config.backoff.max_attempts {
                backoff = backoff.with_max_attempts(max);
            }

            while let Some(delay) = backoff.next_delay() {
                app.set_state(AppState::Backoff {
                    attempt: backoff.attempt(),
                    next_retry: std::time::Instant::now() + delay,
                });

                time::sleep(delay).await;

                app.set_state(AppState::Starting);
                match supervisor.spawn(&config).await {
                    Ok(new_handle) => {
                        app.set_pid(Some(new_handle.pid));
                        app.set_state(AppState::Running);
                        app.increment_restart_count();
                        info!("Restarted app {} with PID {}", app.id, new_handle.pid);

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
                    }
                }
            }

            warn!("Backoff exhausted for app {}", app.id);
            app.set_state(AppState::Stopped);
        } else {
            app.set_state(AppState::Stopped);
        }
    }

    async fn handle_supervisor_event(&self, event: SupervisorEvent) {
        match event {
            SupervisorEvent::ProcessStarted { app, pid } => {
                info!("Process {} started with PID {}", app, pid);
            }
            SupervisorEvent::ProcessExited { app, status } => {
                info!("Process {} exited with status {:?}", app, status);
            }
            SupervisorEvent::ProcessCrashed { app, reason } => {
                error!("Process {} crashed: {}", app, reason);
            }
            SupervisorEvent::ProcessRestarting {
                app,
                attempt,
                delay,
            } => {
                info!("Restarting {} (attempt {}) after {:?}", app, attempt, delay);
            }
            SupervisorEvent::BackoffExhausted { app } => {
                warn!("Backoff exhausted for {}", app);
            }
            SupervisorEvent::HealthCheckFailed { app, reason } => {
                warn!("Health check failed for {}: {}", app, reason);
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
            }
            _ => {}
        }
    }

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
        #[cfg(not(unix))]
        {
            let _ = shutdown_tx;
        }
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
