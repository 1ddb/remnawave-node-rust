use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rw_core::version::{read_core_version, CoreVersion};
use thiserror::Error;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

pub const CORE_LINK: &str = "/usr/local/bin/rw-core";
pub const XRAY_LOG_FILE: &str = "/var/log/xray/current";

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to spawn xray: {0}")]
    Spawn(String),
}

#[derive(Debug, Clone)]
pub struct ProcessStatus {
    pub up: bool,
    pub pid: Option<u32>,
    pub raw: String,
}

struct State {
    child: Option<Child>,
    restart_count: u32,
    monitor_running: bool,
}

#[derive(Clone)]
pub struct XrayProcess {
    core_link: String,
    log_path: PathBuf,
    config_arg: String,
    state: Arc<Mutex<State>>,
    stopping: Arc<AtomicBool>,
}

impl XrayProcess {
    pub fn new(
        core_link: impl Into<String>,
        log_path: impl Into<PathBuf>,
        xtls_api_socket_path: &str,
        internal_socket_path: &str,
        internal_token: &str,
    ) -> Self {
        let config_arg = format!(
            "@{internal_socket_path}:/internal/get-config?token={internal_token}"
        );
        let _ = xtls_api_socket_path;
        Self {
            core_link: core_link.into(),
            log_path: log_path.into(),
            config_arg,
            state: Arc::new(Mutex::new(State {
                child: None,
                restart_count: 0,
                monitor_running: false,
            })),
            stopping: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn core_link(&self) -> &str {
        &self.core_link
    }

    pub async fn start(&self) -> Result<u32, ProcessError> {
        self.stopping.store(false, Ordering::SeqCst);
        let mut state = self.state.lock().await;

        if let Some(child) = state.child.as_mut() {
            if child.try_wait()?.is_none() {
                return Ok(child.id().unwrap_or_default());
            }
            state.child = None;
        }

        let pid = self.spawn_child(&mut state).await?;
        state.restart_count = 0;

        if !state.monitor_running {
            state.monitor_running = true;
            let this = self.clone();
            tokio::spawn(async move { this.monitor().await });
        }

        Ok(pid)
    }

    async fn spawn_child(&self, state: &mut State) -> Result<u32, ProcessError> {
        if let Some(parent) = self.log_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;
        let log_err = log.try_clone()?;

        let mut cmd = Command::new(&self.core_link);
        cmd.arg("-config")
            .arg(&self.config_arg)
            .arg("-format")
            .arg("json")
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_err))
            .kill_on_drop(true);

        let child = cmd
            .spawn()
            .map_err(|e| ProcessError::Spawn(e.to_string()))?;
        let pid = child.id().unwrap_or_default();
        info!("Xray process started (pid {pid})");
        state.child = Some(child);
        Ok(pid)
    }

    async fn monitor(&self) {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;

            let exited = {
                let mut state = self.state.lock().await;
                // Check `stopping` under the state lock so a concurrent
                // `start()` (which also takes the lock) cannot resurrect the
                // process between the check and `monitor_running = false`.
                if self.stopping.load(Ordering::SeqCst) {
                    state.monitor_running = false;
                    return;
                }
                match state.child.as_mut() {
                    None => true,
                    Some(child) => match child.try_wait() {
                        Ok(Some(status)) => {
                            warn!("Xray process exited: {status}");
                            state.child = None;
                            true
                        }
                        Ok(None) => false,
                        Err(e) => {
                            error!("Failed to poll Xray process: {e}");
                            state.child = None;
                            true
                        }
                    },
                }
            };

            if !exited {
                continue;
            }

            let delay = {
                let mut state = self.state.lock().await;
                if self.stopping.load(Ordering::SeqCst) {
                    state.monitor_running = false;
                    return;
                }
                let delay = backoff(state.restart_count);
                state.restart_count = state.restart_count.saturating_add(1);
                delay
            };
            warn!("Restarting Xray in {:?}", delay);
            tokio::time::sleep(delay).await;

            let mut state = self.state.lock().await;
            if self.stopping.load(Ordering::SeqCst) {
                state.monitor_running = false;
                return;
            }
            if state.child.is_none() {
                if let Err(e) = self.spawn_child(&mut state).await {
                    error!("Failed to restart Xray: {e}; will retry with backoff");
                }
            }
        }
    }

    pub async fn stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        let mut state = self.state.lock().await;
        if let Some(mut child) = state.child.take() {
            let _ = child.start_kill();
            let _ = child.wait().await;
            info!("Xray process stopped");
        }
        state.restart_count = 0;
    }

    pub async fn status(&self) -> ProcessStatus {
        let mut state = self.state.lock().await;
        let (up, pid) = match state.child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(None) => (true, child.id()),
                Ok(Some(_)) | Err(_) => {
                    state.child = None;
                    (false, None)
                }
            },
            None => (false, None),
        };
        let raw = match pid {
            Some(pid) => format!("up {pid}"),
            None => "down".to_string(),
        };
        ProcessStatus { up, pid, raw }
    }

    pub async fn is_up(&self) -> bool {
        self.status().await.up
    }

    pub async fn get_core_version(&self) -> Option<CoreVersion> {
        match read_core_version(&self.core_link).await {
            Ok(v) => Some(v),
            Err(e) => {
                warn!("Failed to read xray core version: {e}");
                None
            }
        }
    }

    pub async fn tail_log_lines(&self, n: usize) -> Vec<String> {
        tail_lines(&self.log_path, n).await
    }

    pub async fn tail_status_line(&self) -> String {
        let status = self.status().await;
        match status.pid {
            Some(pid) => format!("up (pid {pid})"),
            None => "down".to_string(),
        }
    }
}

fn backoff(restart_count: u32) -> Duration {
    let secs = 1u64 << restart_count.min(5);
    Duration::from_secs(secs.min(30))
}

pub async fn tail_lines(path: impl AsRef<Path>, n: usize) -> Vec<String> {
    match tokio::fs::read_to_string(path).await {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
            let start = lines.len().saturating_sub(n);
            lines[start..].iter().map(|s| s.to_string()).collect()
        }
        Err(_) => Vec::new(),
    }
}
