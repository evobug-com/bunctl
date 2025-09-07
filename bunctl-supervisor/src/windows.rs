use async_trait::async_trait;
use bunctl_core::{
    AppConfig, AppId, Error, ExitStatus, ProcessHandle, ProcessInfo, ProcessSupervisor, Result,
    SupervisorEvent,
};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, mpsc};
use tracing::{debug, error, warn};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_TERMINATE, PROCESS_VM_READ,
};

use crate::common::ProcessRegistry;

const MAX_LOG_SIZE: u64 = 10 * 1024 * 1024; // 10MB

#[derive(Clone)]
struct LogRotator {
    log_dir: PathBuf,
    max_size: u64,
}

impl LogRotator {
    fn new(log_dir: PathBuf) -> Self {
        Self {
            log_dir,
            max_size: MAX_LOG_SIZE,
        }
    }

    fn rotate_if_needed(&self, app_id: &AppId) -> Result<()> {
        let stdout_path = self.log_dir.join(format!("{}-out.log", app_id));
        let stderr_path = self.log_dir.join(format!("{}-err.log", app_id));

        self.rotate_file(&stdout_path)?;
        self.rotate_file(&stderr_path)?;
        Ok(())
    }

    fn rotate_file(&self, path: &Path) -> Result<()> {
        if let Ok(metadata) = std::fs::metadata(path)
            && metadata.len() > self.max_size
        {
            let backup_path = path.with_extension("log.old");

            // Remove old backup if exists
            let _ = std::fs::remove_file(&backup_path);

            // Rename current to backup
            std::fs::rename(path, backup_path)?;

            debug!("Rotated log file: {:?}", path);
        }
        Ok(())
    }

    #[allow(dead_code)]
    fn cleanup(&self, app_id: &AppId) {
        let patterns = [
            format!("{}-out.log", app_id),
            format!("{}-out.log.old", app_id),
            format!("{}-err.log", app_id),
            format!("{}-err.log.old", app_id),
        ];

        for pattern in patterns {
            let path = self.log_dir.join(pattern);
            let _ = std::fs::remove_file(path);
        }
    }
}

struct JobObject {
    handle: HANDLE,
}

impl JobObject {
    /// Creates a new Windows Job Object for process management.
    ///
    /// # Safety
    /// This function is marked unsafe because it:
    /// - Calls Windows API functions that require careful handling
    /// - The caller must ensure the returned JobObject is properly dropped
    unsafe fn new(name: Option<&str>) -> Result<Self> {
        // SAFETY: CreateJobObjectW is safe to call with null security attributes
        // and either a null name or a properly null-terminated UTF-16 string
        let handle = unsafe {
            if let Some(name) = name {
                let wide_name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
                CreateJobObjectW(std::ptr::null_mut(), wide_name.as_ptr())
            } else {
                CreateJobObjectW(std::ptr::null_mut(), std::ptr::null())
            }
        };

        if handle.is_null() {
            return Err(Error::Supervisor("Failed to create job object".to_string()));
        }

        // Configure job object to kill all processes when handle is closed
        // SAFETY: zeroed() is safe for JOBOBJECT_EXTENDED_LIMIT_INFORMATION as all
        // zero values are valid for this structure
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = 0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE

        // SAFETY: SetInformationJobObject is safe to call with:
        // - A valid handle from CreateJobObjectW
        // - A properly initialized JOBOBJECT_EXTENDED_LIMIT_INFORMATION structure
        // - Correct size parameter
        let result = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };

        if result == 0 {
            // SAFETY: CloseHandle is safe to call with a valid handle
            unsafe { CloseHandle(handle) };
            return Err(Error::Supervisor(
                "Failed to configure job object".to_string(),
            ));
        }

        Ok(Self { handle })
    }

    /// Assigns a process to this job object.
    ///
    /// # Safety
    /// The caller must ensure that process_handle is a valid handle to a process
    /// that hasn't been assigned to another job object.
    unsafe fn assign_process(&self, process_handle: HANDLE) -> Result<()> {
        // SAFETY: AssignProcessToJobObject is safe when both handles are valid
        // The caller ensures process_handle validity
        let result = unsafe { AssignProcessToJobObject(self.handle, process_handle) };
        if result == 0 {
            return Err(Error::Supervisor(
                "Failed to assign process to job object".to_string(),
            ));
        }
        Ok(())
    }

    /// Terminates all processes in the job object.
    ///
    /// # Safety
    /// This function is safe to call but marked unsafe to indicate it
    /// forcefully terminates processes without cleanup.
    unsafe fn terminate(&self, exit_code: u32) -> Result<()> {
        // SAFETY: TerminateJobObject is safe with a valid job handle
        let result = unsafe { TerminateJobObject(self.handle, exit_code) };
        if result == 0 {
            return Err(Error::Supervisor(
                "Failed to terminate job object".to_string(),
            ));
        }
        Ok(())
    }

    #[allow(dead_code)]
    /// Gets the list of process IDs in the job object.
    ///
    /// # Safety
    /// This function safely queries the job object for process information.
    /// Currently returns empty vector as implementation is pending.
    unsafe fn get_process_list(&self) -> Result<Vec<u32>> {
        // This function is not currently used but kept for future enhancement
        // The Windows API for querying job object process list is complex
        // and requires careful handling of variable-sized structures
        Ok(Vec::new())
    }
}

impl Drop for JobObject {
    fn drop(&mut self) {
        // SAFETY: CloseHandle is safe to call with a valid handle.
        // We check that the handle is not null or invalid before calling.
        unsafe {
            if !self.handle.is_null() && self.handle != INVALID_HANDLE_VALUE {
                CloseHandle(self.handle);
            }
        }
    }
}

// SAFETY: JobObject contains only a HANDLE which is safe to send between threads.
// Windows job object handles can be safely accessed from multiple threads.
unsafe impl Send for JobObject {}
// SAFETY: JobObject operations are thread-safe at the Windows API level.
// Multiple threads can safely call job object functions with the same handle.
unsafe impl Sync for JobObject {}

pub struct WindowsSupervisor {
    registry: Arc<ProcessRegistry>,
    event_tx: mpsc::Sender<SupervisorEvent>,
    event_rx: parking_lot::Mutex<Option<mpsc::Receiver<SupervisorEvent>>>,
    job_objects: Arc<RwLock<HashMap<AppId, Arc<JobObject>>>>,
    log_rotator: LogRotator,
    log_handles: Arc<RwLock<HashMap<AppId, (File, File)>>>,
}

impl WindowsSupervisor {
    pub async fn new() -> Result<Self> {
        debug!("Initializing Windows supervisor with Job Objects");
        let (event_tx, event_rx) = mpsc::channel(1024);

        let log_dir = std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("bunctl")
            .join("logs");

        // Ensure log directory exists
        std::fs::create_dir_all(&log_dir)?;

        debug!("Log directory: {:?}", log_dir);

        Ok(Self {
            registry: Arc::new(ProcessRegistry::new()),
            event_tx,
            event_rx: parking_lot::Mutex::new(Some(event_rx)),
            job_objects: Arc::new(RwLock::new(HashMap::new())),
            log_rotator: LogRotator::new(log_dir),
            log_handles: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    async fn spawn_process(&self, config: &AppConfig) -> Result<ProcessHandle> {
        let app_id = AppId::new(&config.name)?;
        debug!(
            "Spawning process for app: {} with command: {} {:?}",
            app_id, config.command, config.args
        );

        // Rotate logs if needed
        self.log_rotator.rotate_if_needed(&app_id)?;

        // Create Job Object for this process
        let job_object = Arc::new(unsafe { JobObject::new(Some(&app_id.to_string()))? });

        let mut builder = bunctl_core::process::ProcessBuilder::new(&config.command);

        // Build environment variables
        let mut env_vars = config.env.clone();
        let important_env_vars = [
            "RUST_LOG",
            "PATH",
            "HOME",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
            "TEMP",
            "TMP",
            "SystemRoot",
            "ProgramFiles",
            "ProgramFiles(x86)",
        ];

        for env_var in &important_env_vars {
            if let Ok(value) = std::env::var(env_var) {
                env_vars.entry(env_var.to_string()).or_insert(value);
            }
        }

        builder = builder
            .args(&config.args)
            .current_dir(&config.cwd)
            .envs(&env_vars);

        if let Some(uid) = config.uid {
            builder = builder.uid(uid);
        }
        if let Some(gid) = config.gid {
            builder = builder.gid(gid);
        }

        // Set up log files
        let stdout_path = self.log_rotator.log_dir.join(format!("{}-out.log", app_id));
        let stderr_path = self.log_rotator.log_dir.join(format!("{}-err.log", app_id));

        let stdout_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&stdout_path)
            .map_err(|e| {
                error!("Failed to open stdout log file {:?}: {}", stdout_path, e);
                Error::Io(e)
            })?;

        let stderr_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&stderr_path)
            .map_err(|e| {
                error!("Failed to open stderr log file {:?}: {}", stderr_path, e);
                Error::Io(e)
            })?;

        // Store log handles for later cleanup
        self.log_handles.write().await.insert(
            app_id.clone(),
            (stdout_file.try_clone()?, stderr_file.try_clone()?),
        );

        builder = builder
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file));

        debug!("Spawning child process for app: {}", app_id);
        let child = builder.spawn().await.map_err(|e| {
            error!("Failed to spawn process for app {}: {}", app_id, e);
            e
        })?;

        let pid = child.id().unwrap();
        debug!("Child process spawned with PID: {}", pid);

        // Try to assign process to job object
        // Note: This may fail if the process has already been assigned to another job
        // or if the process has already exited
        {
            let process_handle =
                unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_TERMINATE, 0, pid) };

            if !process_handle.is_null() {
                if let Err(e) = unsafe { job_object.assign_process(process_handle) } {
                    debug!(
                        "Failed to assign process {} to job object: {}. Process will run without job object isolation.",
                        pid, e
                    );
                    // Don't fail the spawn - the process is running, just without job object benefits
                } else {
                    debug!("Process {} assigned to job object", pid);
                }
                unsafe { CloseHandle(process_handle) };
            }
        }

        // Store job object
        self.job_objects
            .write()
            .await
            .insert(app_id.clone(), job_object);

        let handle = ProcessHandle::new(pid, app_id.clone(), child);
        self.registry.register(app_id.clone(), handle.clone());

        // Send event
        let _ = self
            .event_tx
            .send(SupervisorEvent::ProcessStarted {
                app: app_id.clone(),
                pid,
            })
            .await;

        debug!("Process spawn completed successfully for app: {}", app_id);
        Ok(handle)
    }

    async fn get_process_info_impl(&self, pid: u32) -> Result<ProcessInfo> {
        // SAFETY: OpenProcess is safe to call with valid PID and access rights
        let handle = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid) };

        if handle.is_null() {
            return Err(Error::ProcessNotFound(pid.to_string()));
        }

        // SAFETY: zeroed() is safe for PROCESS_MEMORY_COUNTERS_EX as all zero values are valid
        let mut mem_counters: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
        // SAFETY: GetProcessMemoryInfo is safe with:
        // - A valid process handle from OpenProcess
        // - A properly sized buffer for PROCESS_MEMORY_COUNTERS_EX
        let mem_info_result = unsafe {
            GetProcessMemoryInfo(
                handle,
                &mut mem_counters as *mut _ as *mut _,
                std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            )
        };

        let memory_bytes = if mem_info_result != 0 {
            Some(mem_counters.WorkingSetSize as u64)
        } else {
            None
        };

        // SAFETY: CloseHandle is safe to call with a valid handle
        unsafe { CloseHandle(handle) };

        // Get command line from WMI or other sources if needed
        // For now, return basic info
        Ok(ProcessInfo {
            pid,
            name: format!("Process-{}", pid),
            command: String::new(),
            args: Vec::new(),
            memory_bytes,
            cpu_percent: None,
            threads: None,
            open_files: None,
        })
    }
}

#[async_trait]
impl ProcessSupervisor for WindowsSupervisor {
    async fn spawn(&self, config: &AppConfig) -> Result<ProcessHandle> {
        self.spawn_process(config).await
    }

    async fn kill_tree(&self, handle: &ProcessHandle) -> Result<()> {
        debug!("Killing process tree for app: {}", handle.app_id);

        // Use job object to kill entire process tree
        if let Some(job_object) = self.job_objects.read().await.get(&handle.app_id) {
            // SAFETY: terminate is safe to call with a valid job object
            // It will forcefully terminate all processes in the job
            unsafe {
                job_object.terminate(1)?;
            }
            debug!("Terminated job object for app: {}", handle.app_id);
        } else {
            // Fallback to killing single process
            let mut h = handle.clone();
            h.kill().await?;
        }

        // Clean up resources
        self.job_objects.write().await.remove(&handle.app_id);
        self.registry.unregister(&handle.app_id);

        // Close and clean up log files
        if let Some(handles) = self.log_handles.write().await.remove(&handle.app_id) {
            drop(handles); // Explicitly drop to close files
        }

        Ok(())
    }

    async fn graceful_stop(
        &self,
        handle: &mut ProcessHandle,
        timeout: Duration,
    ) -> Result<ExitStatus> {
        debug!(
            "Attempting graceful stop for app: {} with timeout: {:?}",
            handle.app_id, timeout
        );

        // Windows doesn't have SIGTERM, so we implement a timeout-based approach
        // Future enhancement: send WM_CLOSE message to all windows

        // Use a timeout to wait for the process
        match tokio::time::timeout(timeout, handle.wait()).await {
            Ok(Ok(status)) => {
                // Process exited within timeout
                Ok(status)
            }
            Ok(Err(e)) => {
                // Error waiting for process
                Err(e)
            }
            Err(_) => {
                // Timeout elapsed, force kill
                debug!("Graceful stop timeout exceeded, force killing");
                self.kill_tree(handle).await?;
                handle.wait().await
            }
        }
    }

    async fn wait(&self, handle: &mut ProcessHandle) -> Result<ExitStatus> {
        handle.wait().await
    }

    async fn get_process_info(&self, pid: u32) -> Result<ProcessInfo> {
        self.get_process_info_impl(pid).await
    }

    async fn set_resource_limits(&self, _handle: &ProcessHandle, config: &AppConfig) -> Result<()> {
        // Windows Job Objects support memory and CPU limits
        // This can be implemented using SetInformationJobObject
        // with JobObjectExtendedLimitInformation

        if let Some(job_object) = self.job_objects.read().await.get(&_handle.app_id) {
            // SAFETY: zeroed() is safe for JOBOBJECT_EXTENDED_LIMIT_INFORMATION structure
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };

            if let Some(max_memory) = config.max_memory {
                info.BasicLimitInformation.LimitFlags |= 0x0200; // JOB_OBJECT_LIMIT_JOB_MEMORY
                info.JobMemoryLimit = max_memory as usize;
            }

            if let Some(_cpu_percent) = config.max_cpu_percent {
                // CPU rate control requires Windows 8+ and different API
                // For now, log that it's not implemented
                debug!("CPU rate limiting not yet implemented on Windows");
            }

            if info.BasicLimitInformation.LimitFlags != 0 {
                // SAFETY: SetInformationJobObject is safe with:
                // - A valid job object handle
                // - A properly initialized limit information structure
                // - Correct size parameter
                let result = unsafe {
                    SetInformationJobObject(
                        job_object.handle,
                        JobObjectExtendedLimitInformation,
                        &info as *const _ as *const _,
                        std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                    )
                };

                if result == 0 {
                    warn!("Failed to set resource limits for job object");
                }
            }
        }

        Ok(())
    }

    fn events(&self) -> mpsc::Receiver<SupervisorEvent> {
        // Take the receiver if available, otherwise return a dummy channel
        // that immediately closes (indicating the receiver was already taken)
        self.event_rx.lock().take().unwrap_or_else(|| {
            // Create a closed channel to indicate the receiver was already taken
            // This avoids panicking and allows the caller to handle the situation
            let (_, rx) = mpsc::channel(1);
            rx
        })
    }

    fn get_handle(&self, app_id: &AppId) -> Option<ProcessHandle> {
        self.registry.get(app_id)
    }
}

impl Drop for WindowsSupervisor {
    fn drop(&mut self) {
        // Best effort cleanup - we can't use async operations in Drop
        // Job objects will automatically terminate all processes when closed
        // due to JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE flag

        // Clear registry
        self.registry.clear();
    }
}
