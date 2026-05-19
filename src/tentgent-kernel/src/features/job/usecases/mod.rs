//! Job use case boundaries.

mod catalog;
mod lifecycle;
pub mod port;
mod workspace;

pub use catalog::StdJobCatalogReadUseCase;
pub use lifecycle::StdJobLifecycleUseCase;
pub use port::{
    JobCancelRequest, JobCatalogReadUseCase, JobCompleteRequest, JobCreateRequest, JobCreateResult,
    JobDeleteTerminalRequest, JobFailRequest, JobInspectRequest, JobInspectResult,
    JobInterruptActiveRequest, JobLifecycleUseCase, JobListRequest, JobListResult,
    JobMutationResult, JobProgressUpdateRequest, JobStartRequest, JobWorkspaceUpdateRequest,
    JobWorkspaceUseCase,
};
pub use workspace::StdJobWorkspaceUseCase;
