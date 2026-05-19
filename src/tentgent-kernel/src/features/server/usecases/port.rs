//! Server use case ports.

use crate::features::server::domain::{
    LaunchMode, ServerCapability, ServerInspection, ServerPrepareOutcome, ServerRef,
    ServerRefSelector, ServerRemoveOutcome, ServerStopOutcome, ServerStoreLayout, ServerSummary,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

/// Request for creating or reusing a stored server spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerPrepareRequest {
    pub layout: RuntimeLayoutInput,
    pub runtime_ref: String,
    pub capability: ServerCapability,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub lazy_load: bool,
    pub idle_seconds: Option<u64>,
}

/// Result of preparing a server spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerPrepareResult {
    pub layout: RuntimeLayout,
    pub store: ServerStoreLayout,
    pub outcome: ServerPrepareOutcome,
}

/// Request for listing stored server specs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerListRequest {
    pub layout: RuntimeLayoutInput,
    pub running_only: bool,
}

/// Result of listing stored server specs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerListResult {
    pub layout: RuntimeLayout,
    pub store: ServerStoreLayout,
    pub servers: Vec<ServerSummary>,
}

/// Request for inspecting one stored server spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInspectRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: ServerRefSelector,
}

/// Result of inspecting one stored server spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInspectResult {
    pub layout: RuntimeLayout,
    pub store: ServerStoreLayout,
    pub inspection: ServerInspection,
}

/// Request for resolving a stopped server before launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerResolveForStartRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: ServerRefSelector,
}

/// Request for recording a spawned process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRecordProcessStartRequest {
    pub layout: RuntimeLayoutInput,
    pub server_ref: ServerRef,
    pub pid: u32,
    pub launch_mode: LaunchMode,
}

/// Request for clearing process metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerClearProcessRequest {
    pub layout: RuntimeLayoutInput,
    pub server_ref: ServerRef,
    pub expected_pid: Option<u32>,
}

/// Request for stopping one running server process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerStopRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: ServerRefSelector,
}

/// Result of stopping one running server process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerStopResult {
    pub layout: RuntimeLayout,
    pub store: ServerStoreLayout,
    pub outcome: ServerStopOutcome,
}

/// Request for removing one stopped server spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRemoveRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: ServerRefSelector,
}

/// Result of removing one stopped server spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRemoveResult {
    pub layout: RuntimeLayout,
    pub store: ServerStoreLayout,
    pub outcome: ServerRemoveOutcome,
}

/// Use-case boundary for stored server specs and catalog reads.
pub trait ServerSpecUseCase {
    /// Creates or reuses a normalized server spec without launching it.
    fn prepare_server(&self, request: ServerPrepareRequest) -> KernelResult<ServerPrepareResult>;

    /// Lists stored servers and their process state.
    fn list_servers(&self, request: ServerListRequest) -> KernelResult<ServerListResult>;

    /// Inspects one stored server.
    fn inspect_server(&self, request: ServerInspectRequest) -> KernelResult<ServerInspectResult>;

    /// Removes one stopped stored server.
    fn remove_server(&self, request: ServerRemoveRequest) -> KernelResult<ServerRemoveResult>;
}

/// Use-case boundary for process lifecycle metadata and process control.
pub trait ServerLifecycleUseCase {
    /// Resolves a stopped server and validates that its target can still be launched.
    fn resolve_for_start(
        &self,
        request: ServerResolveForStartRequest,
    ) -> KernelResult<ServerInspectResult>;

    /// Records a spawned server process.
    fn record_process_start(
        &self,
        request: ServerRecordProcessStartRequest,
    ) -> KernelResult<ServerInspectResult>;

    /// Clears process metadata when it matches an expected pid.
    fn clear_process_if_matches(&self, request: ServerClearProcessRequest) -> KernelResult<()>;

    /// Terminates a running server process and clears matching process metadata.
    fn stop_server(&self, request: ServerStopRequest) -> KernelResult<ServerStopResult>;
}
