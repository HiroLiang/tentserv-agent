//! Daemon use case ports.

use std::{fmt, future::Future, path::PathBuf, pin::Pin, time::Duration};

use crate::features::daemon::domain::{DaemonBind, DaemonInspection, DaemonStoreLayout};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

pub type DaemonUseCaseFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + Send + 'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonInspectionMode {
    CleanupStale,
    Observational,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStatusRequest {
    pub layout: RuntimeLayoutInput,
    pub mode: DaemonInspectionMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStatusResult {
    pub layout: RuntimeLayout,
    pub store: DaemonStoreLayout,
    pub inspection: DaemonInspection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonPrepareRunRequest {
    pub layout: RuntimeLayoutInput,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub token_enabled: bool,
    pub allow_unsafe_bind: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonPrepareRunResult {
    pub layout: RuntimeLayout,
    pub store: DaemonStoreLayout,
    pub bind: DaemonBind,
    pub inspection: DaemonInspection,
    pub bind_warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonRecordProcessStartRequest {
    pub layout: RuntimeLayoutInput,
    pub pid: u32,
    pub bind: DaemonBind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonClearProcessRequest {
    pub layout: RuntimeLayoutInput,
    pub expected_pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStopRequest {
    pub layout: RuntimeLayoutInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStopResult {
    pub layout: RuntimeLayout,
    pub store: DaemonStoreLayout,
    pub inspection: DaemonInspection,
    pub stopped_pid: u32,
}

#[derive(Clone, PartialEq, Eq)]
pub struct DaemonReadinessToken(String);

impl DaemonReadinessToken {
    pub fn parse(value: impl AsRef<str>) -> Option<Self> {
        let trimmed = value.as_ref().trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(Self(trimmed.to_string()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for DaemonReadinessToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DaemonReadinessToken([redacted])")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonDetachedStartRequest {
    pub layout: RuntimeLayoutInput,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub token_enabled: bool,
    pub allow_unsafe_bind: bool,
    pub executable: PathBuf,
    pub status_probe_token: Option<DaemonReadinessToken>,
    pub startup_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonDetachedStartResult {
    pub layout: RuntimeLayout,
    pub store: DaemonStoreLayout,
    pub inspection: DaemonInspection,
    pub daemon_url: String,
    pub launched_pid: Option<u32>,
    pub stdout_log_path: PathBuf,
    pub stderr_log_path: PathBuf,
    pub status_warning: Option<String>,
    pub bind_warnings: Vec<String>,
    pub already_running: bool,
}

/// Use-case boundary for daemon status reads.
pub trait DaemonStatusUseCase {
    /// Resolves daemon state for a runtime home.
    fn daemon_status(&self, request: DaemonStatusRequest) -> KernelResult<DaemonStatusResult>;
}

/// Use-case boundary for foreground lifecycle preparation and process control.
pub trait DaemonLifecycleUseCase {
    /// Prepares a foreground daemon run by resolving layout, bind, state, and bind safety.
    fn prepare_run(&self, request: DaemonPrepareRunRequest)
        -> KernelResult<DaemonPrepareRunResult>;

    /// Records the pid and effective bind after the HTTP listener has started.
    fn record_process_start(
        &self,
        request: DaemonRecordProcessStartRequest,
    ) -> KernelResult<DaemonStatusResult>;

    /// Clears process metadata when it matches an optional expected pid.
    fn clear_process_if_matches(&self, request: DaemonClearProcessRequest) -> KernelResult<()>;

    /// Terminates a running daemon process and clears matching process metadata.
    fn stop_daemon(&self, request: DaemonStopRequest) -> KernelResult<DaemonStopResult>;
}

/// Use-case boundary for detached daemon startup and readiness checks.
pub trait DaemonDetachedStartUseCase {
    /// Starts or reuses a detached daemon process and waits for readiness.
    fn start_daemon_detached<'a>(
        &'_ self,
        request: DaemonDetachedStartRequest,
    ) -> DaemonUseCaseFuture<'_, DaemonDetachedStartResult>;
}
