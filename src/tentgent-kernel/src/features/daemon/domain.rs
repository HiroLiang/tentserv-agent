//! Daemon bind, process metadata, runtime-state, and path domain types.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::foundation::net::{format_host_for_url_authority, http_url_from_host_port};

pub const DEFAULT_DAEMON_HOST: &str = "127.0.0.1";
pub const DEFAULT_DAEMON_PORT: u16 = 8790;

pub const DAEMON_PROCESS_METADATA_FILENAME: &str = "daemon.toml";
pub const DAEMON_PID_FILENAME: &str = "tentgent.pid";
pub const DAEMON_STDOUT_LOG_FILENAME: &str = "daemon.stdout.log";
pub const DAEMON_STDERR_LOG_FILENAME: &str = "daemon.stderr.log";

pub const DAEMON_WARNING_RUNTIME_HOME_MISSING: &str = "runtime_home_missing";
pub const DAEMON_WARNING_RUNTIME_DIR_MISSING: &str = "runtime_dir_missing";
pub const DAEMON_WARNING_PROCESS_PATH_MISSING: &str = "process_path_missing";
pub const DAEMON_WARNING_PID_PATH_STALE: &str = "pid_path_stale";
pub const DAEMON_WARNING_PROCESS_METADATA_STALE: &str = "process_metadata_stale";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DaemonRuntimeStatus {
    Running,
    Stopped,
}

impl DaemonRuntimeStatus {
    pub const fn from_running(running: bool) -> Self {
        if running {
            Self::Running
        } else {
            Self::Stopped
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Stopped => "stopped",
        }
    }
}

impl fmt::Display for DaemonRuntimeStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonProcessMetadata {
    pub pid: u32,
    pub host: String,
    pub port: u16,
    pub started_at: String,
}

impl DaemonProcessMetadata {
    pub fn bind(&self) -> DaemonBind {
        DaemonBind {
            host: self.host.clone(),
            port: self.port,
        }
    }

    pub fn daemon_url(&self) -> String {
        daemon_url(&self.host, self.port)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonBind {
    pub host: String,
    pub port: u16,
}

impl DaemonBind {
    pub fn from_optional(host: Option<&str>, port: Option<u16>) -> Result<Self, DaemonHostError> {
        Ok(Self {
            host: normalize_daemon_host(host)?,
            port: port.unwrap_or(DEFAULT_DAEMON_PORT),
        })
    }

    pub fn daemon_url(&self) -> String {
        daemon_url(&self.host, self.port)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonInspection {
    pub home_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub log_dir: PathBuf,
    pub process_path: PathBuf,
    pub pid_path: PathBuf,
    pub stdout_log_path: PathBuf,
    pub stderr_log_path: PathBuf,
    pub running: bool,
    pub process: Option<DaemonProcessMetadata>,
    pub warnings: Vec<DaemonWarning>,
}

impl DaemonInspection {
    pub fn status(&self) -> DaemonRuntimeStatus {
        DaemonRuntimeStatus::from_running(self.running)
    }

    pub fn daemon_url(&self) -> String {
        self.process
            .as_ref()
            .map(DaemonProcessMetadata::daemon_url)
            .unwrap_or_else(|| daemon_url(DEFAULT_DAEMON_HOST, DEFAULT_DAEMON_PORT))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonWarning {
    pub code: String,
    pub message: String,
    pub path: Option<PathBuf>,
}

impl DaemonWarning {
    pub fn new(code: impl Into<String>, message: impl Into<String>, path: Option<PathBuf>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            path,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStoreLayout {
    pub home_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl DaemonStoreLayout {
    pub fn from_home_runtime_log_dirs(
        home_dir: impl Into<PathBuf>,
        runtime_dir: impl Into<PathBuf>,
        log_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            home_dir: home_dir.into(),
            runtime_dir: runtime_dir.into(),
            log_dir: log_dir.into(),
        }
    }

    pub fn process_metadata_path(&self) -> PathBuf {
        self.runtime_dir.join(DAEMON_PROCESS_METADATA_FILENAME)
    }

    pub fn pid_path(&self) -> PathBuf {
        self.runtime_dir.join(DAEMON_PID_FILENAME)
    }

    pub fn stdout_log_path(&self) -> PathBuf {
        self.log_dir.join(DAEMON_STDOUT_LOG_FILENAME)
    }

    pub fn stderr_log_path(&self) -> PathBuf {
        self.log_dir.join(DAEMON_STDERR_LOG_FILENAME)
    }

    pub fn stopped_inspection(self, warnings: Vec<DaemonWarning>) -> DaemonInspection {
        DaemonInspection {
            process_path: self.process_metadata_path(),
            pid_path: self.pid_path(),
            stdout_log_path: self.stdout_log_path(),
            stderr_log_path: self.stderr_log_path(),
            home_dir: self.home_dir,
            runtime_dir: self.runtime_dir,
            log_dir: self.log_dir,
            running: false,
            process: None,
            warnings,
        }
    }
}

pub fn normalize_daemon_host(host: Option<&str>) -> Result<String, DaemonHostError> {
    let host = host.unwrap_or(DEFAULT_DAEMON_HOST).trim();
    if host.is_empty() {
        return Err(DaemonHostError::Empty);
    }

    Ok(host.to_string())
}

pub fn daemon_url(host: &str, port: u16) -> String {
    http_url_from_host_port(host, port)
}

pub fn host_for_daemon_url(host: &str) -> String {
    format_host_for_url_authority(host)
}

pub fn daemon_status_probe_warning(
    status_code: u16,
    status_success: bool,
    status_label: impl fmt::Display,
) -> Option<String> {
    if status_success {
        None
    } else if status_code == 401 {
        Some("daemon ready but status requires a valid token".to_string())
    } else {
        Some(format!(
            "daemon ready but /v1/status returned {status_label}"
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DaemonHostError {
    #[error("daemon host must not be empty")]
    Empty,
}
