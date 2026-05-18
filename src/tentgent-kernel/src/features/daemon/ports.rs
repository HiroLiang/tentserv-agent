//! Daemon feature package ports.

use std::{future::Future, path::PathBuf, pin::Pin};

use crate::foundation::error::KernelResult;

use super::domain::{DaemonBind, DaemonProcessMetadata, DaemonStoreLayout};

pub type DaemonPortFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStoreSnapshot {
    pub home_dir_exists: bool,
    pub runtime_dir_exists: bool,
    pub log_dir_exists: bool,
    pub process_path_exists: bool,
    pub pid_path_exists: bool,
    pub process: Option<DaemonProcessMetadata>,
    pub pid_file: Option<DaemonPidFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonPidFile {
    Valid(u32),
    Invalid { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonBindHostClass {
    Loopback,
    Wildcard,
    NonLoopback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonBindSafetyRequest {
    pub bind: DaemonBind,
    pub token_enabled: bool,
    pub allow_unsafe_bind: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonBindSafetyReport {
    pub host_class: DaemonBindHostClass,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonDetachedCommand {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub stdout_log_path: PathBuf,
    pub stderr_log_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStatusProbeOutcome {
    pub status_warning: Option<String>,
}

/// Ensures daemon runtime and log directories exist for mutating operations.
pub trait DaemonStoreLayoutInitializer: Sync {
    /// Creates daemon runtime and log directories for the resolved layout.
    fn ensure_daemon_store_layout(&self, layout: &DaemonStoreLayout) -> KernelResult<()>;
}

/// Observes and mutates daemon process metadata under runtime-home.
pub trait DaemonStateStore: Sync {
    /// Reads daemon metadata, pid-file state, and relevant path existence facts.
    fn inspect_daemon_store(&self, layout: &DaemonStoreLayout)
        -> KernelResult<DaemonStoreSnapshot>;

    /// Writes process metadata and the matching pid file after a daemon starts.
    fn record_process_start(
        &self,
        layout: &DaemonStoreLayout,
        metadata: &DaemonProcessMetadata,
    ) -> KernelResult<()>;

    /// Clears process metadata and pid state when absent or matching the expected pid.
    fn clear_process_if_matches(
        &self,
        layout: &DaemonStoreLayout,
        expected_pid: Option<u32>,
    ) -> KernelResult<()>;
}

/// Probes local process liveness from a persisted process id.
pub trait DaemonProcessProbe: Sync {
    /// Returns true when the operating system reports the process is still running.
    fn is_process_running(&self, pid: u32) -> KernelResult<bool>;
}

/// Controls local daemon processes.
pub trait DaemonProcessController: Sync {
    /// Sends a graceful termination signal and waits briefly for exit.
    fn terminate_process(&self, pid: u32) -> KernelResult<()>;
}

/// Supplies timestamps for daemon process metadata.
pub trait DaemonClock: Sync {
    /// Returns the current UTC timestamp formatted as RFC3339.
    fn now_rfc3339(&self) -> KernelResult<String>;
}

/// Checks whether a daemon bind target is safe for the selected auth state.
pub trait DaemonBindSafetyChecker: Sync {
    /// Validates daemon bind safety and returns any warnings to render upstream.
    fn check_bind_safety(
        &self,
        request: DaemonBindSafetyRequest,
    ) -> KernelResult<DaemonBindSafetyReport>;
}

/// Launches hidden detached daemon child processes.
pub trait DaemonDetachedLauncher: Sync {
    /// Starts a detached daemon command and returns the child pid.
    fn launch_detached(&self, command: &DaemonDetachedCommand) -> KernelResult<u32>;
}

/// Probes HTTP readiness for a launched or already-running daemon.
pub trait DaemonHttpReadinessProbe: Sync {
    /// Checks the public health endpoint.
    fn probe_healthz<'a>(&'a self, daemon_url: &'a str) -> DaemonPortFuture<'a, ()>;

    /// Checks authenticated status when a token is available.
    fn probe_status<'a>(
        &'a self,
        daemon_url: &'a str,
        token: &'a str,
    ) -> DaemonPortFuture<'a, DaemonStatusProbeOutcome>;
}
