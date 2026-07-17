//! Server use cases.

mod common;
mod lifecycle;
mod port;
mod support_gate;

#[cfg(test)]
mod tests;

pub(crate) use common::server_runtime_backend_for_format;
pub use lifecycle::StdServerUseCase;
pub use port::{
    ServerClearProcessRequest, ServerInspectRequest, ServerInspectResult, ServerLifecycleUseCase,
    ServerListRequest, ServerListResult, ServerPrepareRequest, ServerPrepareResult,
    ServerRecordProcessStartRequest, ServerRemoveRequest, ServerRemoveResult,
    ServerResolveForStartRequest, ServerSpecUseCase, ServerStartAuthorization, ServerStopRequest,
    ServerStopResult,
};
