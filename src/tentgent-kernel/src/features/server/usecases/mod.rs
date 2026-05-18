//! Server use cases.

mod common;
mod lifecycle;
mod port;

#[cfg(test)]
mod tests;

pub use lifecycle::StdServerUseCase;
pub use port::{
    ServerClearProcessRequest, ServerInspectRequest, ServerInspectResult, ServerLifecycleUseCase,
    ServerListRequest, ServerListResult, ServerPrepareRequest, ServerPrepareResult,
    ServerRecordProcessStartRequest, ServerRemoveRequest, ServerRemoveResult,
    ServerResolveForStartRequest, ServerSpecUseCase, ServerStopRequest, ServerStopResult,
};
