//! Job use case ports.

use crate::foundation::error::KernelResult;

use super::super::{
    domain::{JobArtifact, JobId, JobItem, JobProgressUpdate, JobWorkspaceSummary},
    ports::JobCreateRecord,
};

/// Request for creating a durable job record.
#[derive(Debug, Clone, PartialEq)]
pub struct JobCreateRequest {
    pub record: JobCreateRecord,
}

/// Result of creating a durable job record.
#[derive(Debug, Clone, PartialEq)]
pub struct JobCreateResult {
    pub job: JobItem,
}

/// Request for listing durable job records.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JobListRequest;

/// Result of listing durable job records.
#[derive(Debug, Clone, PartialEq)]
pub struct JobListResult {
    pub jobs: Vec<JobItem>,
}

/// Request for inspecting one durable job record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobInspectRequest {
    pub job_id: JobId,
}

/// Result of inspecting one durable job record.
#[derive(Debug, Clone, PartialEq)]
pub struct JobInspectResult {
    pub job: Option<JobItem>,
}

/// Request for marking a job as running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobStartRequest {
    pub job_id: JobId,
    pub stage: String,
}

/// Request for appending progress, stage, warning, or output updates.
#[derive(Debug, Clone, PartialEq)]
pub struct JobProgressUpdateRequest {
    pub job_id: JobId,
    pub update: JobProgressUpdate,
}

/// Request for attaching the current workspace summary to a job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobWorkspaceUpdateRequest {
    pub job_id: JobId,
    pub workspace: JobWorkspaceSummary,
}

/// Request for marking a job as succeeded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobCompleteRequest {
    pub job_id: JobId,
    pub artifact: Option<JobArtifact>,
    pub result_summary: String,
}

/// Request for marking a job as failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobFailRequest {
    pub job_id: JobId,
    pub error_summary: String,
}

/// Request for canceling one active job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobCancelRequest {
    pub job_id: JobId,
    pub reason: String,
}

/// Request for deleting one terminal job record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobDeleteTerminalRequest {
    pub job_id: JobId,
}

/// Request for interrupting all active jobs after daemon lifecycle changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobInterruptActiveRequest {
    pub reason: String,
}

/// Result of mutating one durable job record.
#[derive(Debug, Clone, PartialEq)]
pub struct JobMutationResult {
    pub job: Option<JobItem>,
}

/// Use-case boundary for read-only job catalog operations.
pub trait JobCatalogReadUseCase {
    /// Lists all durable job records.
    fn list_jobs(&self, request: JobListRequest) -> KernelResult<JobListResult>;

    /// Inspects one durable job record by job id.
    fn inspect_job(&self, request: JobInspectRequest) -> KernelResult<JobInspectResult>;
}

/// Use-case boundary for job lifecycle state transitions.
pub trait JobLifecycleUseCase {
    /// Creates a durable queued job record.
    fn create_job(&self, request: JobCreateRequest) -> KernelResult<JobCreateResult>;

    /// Marks a queued job as running.
    fn start_job(&self, request: JobStartRequest) -> KernelResult<JobMutationResult>;

    /// Updates progress, stage, warnings, or output tail for an active job.
    fn update_job_progress(
        &self,
        request: JobProgressUpdateRequest,
    ) -> KernelResult<JobMutationResult>;

    /// Marks a job as succeeded and attaches an optional produced artifact.
    fn complete_job(&self, request: JobCompleteRequest) -> KernelResult<JobMutationResult>;

    /// Marks a job as failed with a concise error summary.
    fn fail_job(&self, request: JobFailRequest) -> KernelResult<JobMutationResult>;

    /// Marks a job as canceled with a concise reason.
    fn cancel_job(&self, request: JobCancelRequest) -> KernelResult<JobMutationResult>;

    /// Deletes a terminal job record.
    fn delete_terminal_job(
        &self,
        request: JobDeleteTerminalRequest,
    ) -> KernelResult<JobMutationResult>;

    /// Marks all queued or running jobs as interrupted.
    fn interrupt_active_jobs(
        &self,
        request: JobInterruptActiveRequest,
    ) -> KernelResult<JobListResult>;
}

/// Use-case boundary for attaching workspace state to durable job records.
pub trait JobWorkspaceUseCase {
    /// Updates the workspace summary for a job.
    fn update_job_workspace(
        &self,
        request: JobWorkspaceUpdateRequest,
    ) -> KernelResult<JobMutationResult>;
}
