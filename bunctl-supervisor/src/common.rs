use bunctl_core::{AppId, ProcessHandle};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct ProcessRegistry {
    processes: Arc<RwLock<HashMap<AppId, ProcessHandle>>>,
    pid_to_app: Arc<RwLock<HashMap<u32, AppId>>>,
}

impl ProcessRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, app_id: AppId, handle: ProcessHandle) {
        let pid = handle.pid;

        // If there was a previous handle for this app_id, remove its PID mapping
        if let Some(old_handle) = self.processes.read().get(&app_id) {
            self.pid_to_app.write().remove(&old_handle.pid);
        }

        self.processes.write().insert(app_id.clone(), handle);
        self.pid_to_app.write().insert(pid, app_id);
    }

    pub fn unregister(&self, app_id: &AppId) -> Option<ProcessHandle> {
        let handle = self.processes.write().remove(app_id);
        if let Some(ref h) = handle {
            self.pid_to_app.write().remove(&h.pid);
        }
        handle
    }

    pub fn get(&self, app_id: &AppId) -> Option<ProcessHandle> {
        self.processes.read().get(app_id).cloned()
    }

    pub fn get_by_pid(&self, pid: u32) -> Option<AppId> {
        self.pid_to_app.read().get(&pid).cloned()
    }

    pub fn list(&self) -> Vec<(AppId, u32)> {
        self.processes
            .read()
            .iter()
            .map(|(id, handle)| (id.clone(), handle.pid))
            .collect()
    }

    pub fn count(&self) -> usize {
        self.processes.read().len()
    }

    pub fn clear(&self) {
        self.processes.write().clear();
        self.pid_to_app.write().clear();
    }
}
