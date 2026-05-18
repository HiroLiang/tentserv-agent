//! Server feature package ports.

use crate::foundation::error::KernelResult;

use super::domain::{
    LaunchMode, ServerInspection, ServerRef, ServerRefSelector, ServerRemoveOutcome,
    ServerRuntimeTarget, ServerSpec, ServerStoreLayout, ServerSummary,
};

/// Ensures the server-store directory exists for mutating server operations.
pub trait ServerStoreLayoutInitializer {
    /// Creates the server-store root directory.
    fn ensure_server_store_layout(&self, layout: &ServerStoreLayout) -> KernelResult<()>;
}

/// Generates stable server refs from normalized server identity data.
pub trait ServerIdentityGenerator {
    /// Derives a full server ref for one normalized runtime target and bind shape.
    fn server_ref_for_target(
        &self,
        target: &ServerRuntimeTarget,
        host: &str,
        port: u16,
        lazy_load: bool,
        idle_seconds: Option<u64>,
    ) -> KernelResult<ServerRef>;
}

/// Supplies timestamps for durable server records.
pub trait ServerClock {
    /// Returns the current UTC timestamp formatted as RFC3339.
    fn now_rfc3339(&self) -> KernelResult<String>;
}

/// Probes local process liveness from a persisted process id.
pub trait ServerProcessProbe {
    /// Returns true when the operating system reports the process is still running.
    fn is_process_running(&self, pid: u32) -> KernelResult<bool>;
}

/// Controls local server processes.
pub trait ServerProcessController {
    /// Sends a graceful termination signal and waits briefly for exit.
    fn terminate_process(&self, pid: u32) -> KernelResult<()>;
}

/// Reads and writes stored server specs and process metadata.
pub trait ServerCatalogStore {
    /// Lists stored server summaries sorted for stable display.
    fn list_servers(&self, layout: &ServerStoreLayout) -> KernelResult<Vec<ServerSummary>>;

    /// Lists only currently running stored servers.
    fn list_running_servers(&self, layout: &ServerStoreLayout) -> KernelResult<Vec<ServerSummary>>;

    /// Resolves a full server ref or unique prefix and returns inspection paths.
    fn inspect_server(
        &self,
        layout: &ServerStoreLayout,
        selector: &ServerRefSelector,
    ) -> KernelResult<ServerInspection>;

    /// Loads a spec after the caller has an exact server ref.
    fn load_server_spec(
        &self,
        layout: &ServerStoreLayout,
        server_ref: &ServerRef,
    ) -> KernelResult<ServerSpec>;

    /// Writes a server spec.
    fn save_server_spec(&self, layout: &ServerStoreLayout, spec: &ServerSpec) -> KernelResult<()>;

    /// Removes a stopped server directory.
    fn remove_server(
        &self,
        layout: &ServerStoreLayout,
        server_ref: &ServerRef,
    ) -> KernelResult<ServerRemoveOutcome>;

    /// Records a newly started server process.
    fn record_process_start(
        &self,
        layout: &ServerStoreLayout,
        server_ref: &ServerRef,
        pid: u32,
        launch_mode: LaunchMode,
        started_at: String,
    ) -> KernelResult<ServerInspection>;

    /// Clears process metadata when it is absent or matches the expected pid.
    fn clear_process_if_matches(
        &self,
        layout: &ServerStoreLayout,
        server_ref: &ServerRef,
        expected_pid: Option<u32>,
    ) -> KernelResult<()>;
}
