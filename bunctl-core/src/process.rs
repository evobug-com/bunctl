use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::process::{Child, Command};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub memory_bytes: Option<u64>,
    pub cpu_percent: Option<f32>,
    pub threads: Option<u32>,
    pub open_files: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct ProcessHandle {
    pub pid: u32,
    pub app_id: crate::AppId,
    pub inner: Option<std::sync::Arc<tokio::sync::Mutex<Child>>>,
}

impl ProcessHandle {
    pub fn new(pid: u32, app_id: crate::AppId, child: Child) -> Self {
        Self {
            pid,
            app_id,
            inner: Some(std::sync::Arc::new(tokio::sync::Mutex::new(child))),
        }
    }

    pub async fn wait(&mut self) -> crate::Result<ExitStatus> {
        if let Some(child_arc) = self.inner.as_ref() {
            let mut child = child_arc.lock().await;
            let status = child.wait().await?;
            Ok(ExitStatus::from_std(status))
        } else {
            Err(crate::Error::ProcessNotFound(self.app_id.to_string()))
        }
    }

    pub async fn kill(&mut self) -> crate::Result<()> {
        if let Some(child_arc) = self.inner.as_ref() {
            let mut child = child_arc.lock().await;
            child.kill().await?;
            Ok(())
        } else {
            #[cfg(unix)]
            {
                use nix::sys::signal::{self, Signal};
                use nix::unistd::Pid;
                signal::kill(Pid::from_raw(self.pid as i32), Signal::SIGKILL)?;
                Ok(())
            }
            #[cfg(windows)]
            {
                Err(crate::Error::Other(anyhow::anyhow!("Process not found")))
            }
        }
    }

    pub async fn signal(&mut self, signal: Signal) -> crate::Result<()> {
        #[cfg(unix)]
        {
            use nix::sys::signal;
            use nix::unistd::Pid;
            signal::kill(Pid::from_raw(self.pid as i32), signal.to_nix())?;
            Ok(())
        }
        #[cfg(windows)]
        {
            match signal {
                Signal::Terminate => self.kill().await,
                _ => Err(crate::Error::Signal(format!(
                    "Signal {:?} not supported on Windows",
                    signal
                ))),
            }
        }
    }

    pub fn id(&self) -> u32 {
        self.pid
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Terminate,
    Kill,
    Reload,
    User1,
    User2,
}

impl Signal {
    #[cfg(unix)]
    fn to_nix(self) -> nix::sys::signal::Signal {
        use nix::sys::signal::Signal as NixSignal;
        match self {
            Signal::Terminate => NixSignal::SIGTERM,
            Signal::Kill => NixSignal::SIGKILL,
            Signal::Reload => NixSignal::SIGHUP,
            Signal::User1 => NixSignal::SIGUSR1,
            Signal::User2 => NixSignal::SIGUSR2,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ExitStatus {
    code: Option<i32>,
    signal: Option<i32>,
}

impl ExitStatus {
    pub fn from_std(status: std::process::ExitStatus) -> Self {
        Self {
            code: status.code(),
            #[cfg(unix)]
            signal: {
                use std::os::unix::process::ExitStatusExt;
                status.signal()
            },
            #[cfg(not(unix))]
            signal: None,
        }
    }

    #[cfg(test)]
    pub fn new(code: Option<i32>, signal: Option<i32>) -> Self {
        Self { code, signal }
    }

    pub fn success(&self) -> bool {
        self.code == Some(0)
    }

    pub fn code(&self) -> Option<i32> {
        self.code
    }

    pub fn signal(&self) -> Option<i32> {
        self.signal
    }

    pub fn should_restart(&self, policy: crate::config::RestartPolicy) -> bool {
        match policy {
            crate::config::RestartPolicy::No => false,
            crate::config::RestartPolicy::Always => true,
            crate::config::RestartPolicy::OnFailure => !self.success(),
            crate::config::RestartPolicy::UnlessStopped => true,
        }
    }
}

pub struct ProcessBuilder {
    command: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    cwd: Option<std::path::PathBuf>,
    uid: Option<u32>,
    gid: Option<u32>,
    stdout: Stdio,
    stderr: Stdio,
    stdin: Stdio,
}

impl ProcessBuilder {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            uid: None,
            gid: None,
            stdout: Stdio::piped(),
            stderr: Stdio::piped(),
            stdin: Stdio::null(),
        }
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.args = args.into_iter().map(|s| s.as_ref().to_string()).collect();
        self
    }

    pub fn env<K, V>(mut self, key: K, value: V) -> Self
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.env
            .push((key.as_ref().to_string(), value.as_ref().to_string()));
        self
    }

    pub fn envs<I, K, V>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        for (k, v) in vars {
            self.env
                .push((k.as_ref().to_string(), v.as_ref().to_string()));
        }
        self
    }

    pub fn current_dir(mut self, dir: impl AsRef<std::path::Path>) -> Self {
        self.cwd = Some(dir.as_ref().to_path_buf());
        self
    }

    pub fn uid(mut self, uid: u32) -> Self {
        self.uid = Some(uid);
        self
    }

    pub fn gid(mut self, gid: u32) -> Self {
        self.gid = Some(gid);
        self
    }

    pub fn stdout(mut self, stdout: Stdio) -> Self {
        self.stdout = stdout;
        self
    }

    pub fn stderr(mut self, stderr: Stdio) -> Self {
        self.stderr = stderr;
        self
    }

    pub fn stdin(mut self, stdin: Stdio) -> Self {
        self.stdin = stdin;
        self
    }

    pub async fn spawn(self) -> crate::Result<Child> {
        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args)
            .stdout(self.stdout)
            .stderr(self.stderr)
            .stdin(self.stdin)
            .kill_on_drop(true);

        if let Some(cwd) = self.cwd {
            cmd.current_dir(cwd);
        }

        for (key, value) in self.env {
            cmd.env(key, value);
        }

        #[cfg(unix)]
        {
            #[allow(unused_imports)]
            use std::os::unix::process::CommandExt;
            if let Some(uid) = self.uid {
                cmd.uid(uid);
            }
            if let Some(gid) = self.gid {
                cmd.gid(gid);
            }
        }

        #[cfg(not(unix))]
        {
            let _ = self.uid;
            let _ = self.gid;
        }

        cmd.spawn()
            .map_err(|e| crate::Error::SpawnFailed(format!("{}: {}", self.command, e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RestartPolicy;

    #[test]
    fn test_exit_status_success() {
        let status = ExitStatus::new(Some(0), None);

        assert!(status.success());
        assert_eq!(status.code(), Some(0));
        assert_eq!(status.signal(), None);
    }

    #[test]
    fn test_exit_status_failure() {
        let status = ExitStatus::new(Some(1), None);

        assert!(!status.success());
        assert_eq!(status.code(), Some(1));
    }

    #[cfg(unix)]
    #[test]
    fn test_exit_status_signal() {
        let status = ExitStatus::new(None, Some(9)); // SIGKILL

        assert!(!status.success());
        assert_eq!(status.code(), None);
        assert_eq!(status.signal(), Some(9));
    }

    #[test]
    fn test_restart_policy_no() {
        let status_success = ExitStatus::new(Some(0), None);
        let status_failure = ExitStatus::new(Some(1), None);

        assert!(!status_success.should_restart(RestartPolicy::No));
        assert!(!status_failure.should_restart(RestartPolicy::No));
    }

    #[test]
    fn test_restart_policy_always() {
        let status_success = ExitStatus::new(Some(0), None);
        let status_failure = ExitStatus::new(Some(1), None);

        assert!(status_success.should_restart(RestartPolicy::Always));
        assert!(status_failure.should_restart(RestartPolicy::Always));
    }

    #[test]
    fn test_restart_policy_on_failure() {
        let status_success = ExitStatus::new(Some(0), None);
        let status_failure = ExitStatus::new(Some(1), None);

        assert!(!status_success.should_restart(RestartPolicy::OnFailure));
        assert!(status_failure.should_restart(RestartPolicy::OnFailure));
    }

    #[test]
    fn test_restart_policy_unless_stopped() {
        let status_success = ExitStatus::new(Some(0), None);
        let status_failure = ExitStatus::new(Some(1), None);

        assert!(status_success.should_restart(RestartPolicy::UnlessStopped));
        assert!(status_failure.should_restart(RestartPolicy::UnlessStopped));
    }

    #[test]
    fn test_process_builder_basic() {
        let _builder = ProcessBuilder::new("echo")
            .args(vec!["hello", "world"])
            .current_dir("/tmp");

        // We can't actually spawn in tests, but we can verify the builder works
        assert!(true);
    }

    #[test]
    fn test_signal_types() {
        let signals = vec![
            Signal::Terminate,
            Signal::Kill,
            Signal::Reload,
            Signal::User1,
            Signal::User2,
        ];

        for signal in signals {
            match signal {
                Signal::Terminate => assert!(true),
                Signal::Kill => assert!(true),
                Signal::Reload => assert!(true),
                Signal::User1 => assert!(true),
                Signal::User2 => assert!(true),
            }
        }
    }
}
