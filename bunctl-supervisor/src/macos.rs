use async_trait::async_trait;
use bunctl_core::{
    AppConfig, AppId, Error, ExitStatus, ProcessHandle, ProcessInfo, ProcessSupervisor, Result,
    SupervisorEvent,
};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::common::ProcessRegistry;

pub struct MacOSSupervisor {
    registry: Arc<ProcessRegistry>,
    event_tx: mpsc::Sender<SupervisorEvent>,
    event_rx: parking_lot::Mutex<Option<mpsc::Receiver<SupervisorEvent>>>,
    process_groups: dashmap::DashMap<AppId, i32>,
}

impl MacOSSupervisor {
    pub async fn new() -> Result<Self> {
        let (event_tx, event_rx) = mpsc::channel(1024);

        Ok(Self {
            registry: Arc::new(ProcessRegistry::new()),
            event_tx,
            event_rx: parking_lot::Mutex::new(Some(event_rx)),
            process_groups: dashmap::DashMap::new(),
        })
    }

    async fn spawn_with_process_group(&self, config: &AppConfig) -> Result<ProcessHandle> {
        let app_id = AppId::new(&config.name)?;

        let mut builder = bunctl_core::process::ProcessBuilder::new(&config.command);
        builder = builder
            .args(&config.args)
            .current_dir(&config.cwd)
            .envs(&config.env);

        if let Some(uid) = config.uid {
            builder = builder.uid(uid);
        }
        if let Some(gid) = config.gid {
            builder = builder.gid(gid);
        }

        let child = builder.spawn().await?;
        let pid = child.id().unwrap();

        #[cfg(target_os = "macos")]
        unsafe {
            let pgid = libc::setpgid(pid as i32, 0);
            if pgid == -1 {
                return Err(Error::Supervisor(format!(
                    "Failed to create process group: {}",
                    std::io::Error::last_os_error()
                )));
            }
            self.process_groups.insert(app_id.clone(), pid as i32);
        }

        let handle = ProcessHandle::new(pid, app_id.clone(), child);
        self.registry.register(
            app_id.clone(),
            ProcessHandle {
                pid,
                app_id: app_id.clone(),
                inner: None,
            },
        );

        let _ = self
            .event_tx
            .send(SupervisorEvent::ProcessStarted { app: app_id, pid })
            .await;

        Ok(handle)
    }

    fn kill_process_group(&self, pgid: i32) -> Result<()> {
        signal::kill(Pid::from_raw(-pgid), Signal::SIGKILL)?;
        Ok(())
    }

    fn get_process_info_sysctl(&self, pid: u32) -> Result<ProcessInfo> {
        use libc::{CTL_KERN, KERN_PROC, KERN_PROC_PID, c_int, size_t, sysctl};
        use std::mem;

        #[repr(C)]
        struct KInfoProc {
            kp_proc: ExternProc,
            kp_eproc: EProc,
        }

        #[repr(C)]
        struct ExternProc {
            p_un: [u8; 16],
            p_vmspace: u64,
            p_sigacts: u64,
            p_flag: i32,
            p_stat: u8,
            p_pid: i32,
            p_oppid: i32,
            p_dupfd: i32,
            p_pgid: i32,
            p_fpgid: i32,
            p_sid: i32,
            p_tsid: i32,
            p_uid: u32,
            p_gid: u32,
            p_ruid: u32,
            p_rgid: u32,
            p_svuid: u32,
            p_svgid: u32,
            p_ngroups: i16,
            p_groups: [u32; 16],
            p_spare: [u8; 8],
        }

        #[repr(C)]
        struct EProc {
            e_paddr: u64,
            e_sess: u64,
            e_pcred: PCred,
            e_ucred: UCred,
            e_vm: VMSpace,
            e_ppid: i32,
            e_pgid: i32,
            e_jobc: i16,
            e_tdev: i32,
            e_tpgid: i32,
            e_tsess: u64,
            e_wmesg: [u8; 8],
            e_xsize: i32,
            e_xrssize: i16,
            e_xccount: i16,
            e_xswrss: i16,
            e_flag: i32,
            e_login: [u8; 12],
            e_spare: [i32; 4],
        }

        #[repr(C)]
        struct PCred {
            pc_lock: [u8; 72],
            pc_ucred: u64,
            p_ruid: u32,
            p_svuid: u32,
            p_rgid: u32,
            p_svgid: u32,
            p_refcnt: i32,
        }

        #[repr(C)]
        struct UCred {
            cr_ref: i32,
            cr_uid: u32,
            cr_ngroups: i16,
            cr_groups: [u32; 16],
        }

        #[repr(C)]
        struct VMSpace {
            vm_refcnt: i32,
            vm_shm: u64,
            vm_rssize: u32,
            vm_swrss: u32,
            vm_tsize: u32,
            vm_dsize: u32,
            vm_ssize: u32,
            vm_taddr: u64,
            vm_daddr: u64,
            vm_maxsaddr: u64,
        }

        unsafe {
            let mut mib = [CTL_KERN, KERN_PROC, KERN_PROC_PID, pid as c_int];
            let mut info: KInfoProc = mem::zeroed();
            let mut size = mem::size_of::<KInfoProc>() as size_t;

            let result = sysctl(
                mib.as_mut_ptr(),
                mib.len() as u32,
                &mut info as *mut _ as *mut _,
                &mut size,
                std::ptr::null_mut(),
                0,
            );

            if result != 0 {
                return Err(Error::ProcessNotFound(pid.to_string()));
            }

            let memory_bytes = (info.kp_eproc.e_vm.vm_rssize as u64) * 4096;

            Ok(ProcessInfo {
                pid,
                name: String::new(),
                command: String::new(),
                args: Vec::new(),
                memory_bytes: Some(memory_bytes),
                cpu_percent: None,
                threads: None,
                open_files: None,
            })
        }
    }
}

#[async_trait]
impl ProcessSupervisor for MacOSSupervisor {
    async fn spawn(&self, config: &AppConfig) -> Result<ProcessHandle> {
        self.spawn_with_process_group(config).await
    }

    async fn kill_tree(&self, handle: &ProcessHandle) -> Result<()> {
        if let Some(pgid) = self.process_groups.get(&handle.app_id) {
            self.kill_process_group(*pgid.value())?;
            self.process_groups.remove(&handle.app_id);
        } else {
            signal::kill(Pid::from_raw(handle.pid as i32), Signal::SIGKILL)?;
        }

        self.registry.unregister(&handle.app_id);
        Ok(())
    }

    async fn wait(&self, handle: &mut ProcessHandle) -> Result<ExitStatus> {
        handle.wait().await
    }

    async fn get_process_info(&self, pid: u32) -> Result<ProcessInfo> {
        self.get_process_info_sysctl(pid)
    }

    async fn set_resource_limits(
        &self,
        _handle: &ProcessHandle,
        _config: &AppConfig,
    ) -> Result<()> {
        Ok(())
    }

    fn events(&self) -> mpsc::Receiver<SupervisorEvent> {
        self.event_rx
            .lock()
            .take()
            .expect("Events receiver already taken")
    }
}
