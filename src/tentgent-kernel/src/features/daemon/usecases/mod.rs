//! Daemon use case boundaries.

mod common;
mod lifecycle;
mod port;

#[cfg(test)]
mod tests;

pub use lifecycle::StdDaemonUseCase;
pub use port::{
    DaemonClearProcessRequest, DaemonDetachedStartRequest, DaemonDetachedStartResult,
    DaemonDetachedStartUseCase, DaemonInspectionMode, DaemonLifecycleUseCase,
    DaemonPrepareRunRequest, DaemonPrepareRunResult, DaemonReadinessToken,
    DaemonRecordProcessStartRequest, DaemonStatusRequest, DaemonStatusResult, DaemonStatusUseCase,
    DaemonStopRequest, DaemonStopResult, DaemonUseCaseFuture,
};
