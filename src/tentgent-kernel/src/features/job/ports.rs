//! Job feature ports.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::foundation::error::KernelResult;

use super::domain::{
    JobArtifact, JobId, JobItem, JobKind, JobProgressUpdate, JobResultFile, JobResultFileList,
    JobTarget, JobWorkspaceStreamSummary, JobWorkspaceSummary,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobCreateRecord {
    pub kind: JobKind,
    pub label: String,
    pub target: Option<JobTarget>,
    pub refresh_targets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobWorkspaceRef {
    pub job_id: JobId,
    pub workspace_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStreamKind {
    Input,
    Result,
}

impl JobStreamKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Result => "result",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobChunkCursor {
    pub next_index: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobChunkWrite {
    pub stream: JobStreamKind,
    pub index: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobChunkRead {
    pub bytes: Vec<u8>,
    pub next_cursor: JobChunkCursor,
    pub done: bool,
    pub chunks_read: usize,
}

pub trait JobCatalogPort {
    fn create_job(&self, record: JobCreateRecord) -> KernelResult<JobItem>;

    fn insert_job(&self, job: JobItem) -> KernelResult<()>;

    fn list_jobs(&self) -> KernelResult<Vec<JobItem>>;

    fn inspect_job(&self, job_id: &JobId) -> KernelResult<Option<JobItem>>;

    fn start_job(&self, job_id: &JobId, stage: String) -> KernelResult<Option<JobItem>>;

    fn update_job_progress(
        &self,
        job_id: &JobId,
        update: JobProgressUpdate,
    ) -> KernelResult<Option<JobItem>>;

    fn update_job_workspace(
        &self,
        job_id: &JobId,
        workspace: JobWorkspaceSummary,
    ) -> KernelResult<Option<JobItem>>;

    fn succeed_job(
        &self,
        job_id: &JobId,
        artifact: Option<JobArtifact>,
        result_summary: String,
    ) -> KernelResult<Option<JobItem>>;

    fn fail_job(&self, job_id: &JobId, error_summary: String) -> KernelResult<Option<JobItem>>;

    fn interrupt_active_jobs(&self, error_summary: String) -> KernelResult<Vec<JobItem>>;

    fn cancel_job(&self, job_id: &JobId, reason: String) -> KernelResult<Option<JobItem>>;

    fn delete_terminal_job(&self, job_id: &JobId) -> KernelResult<Option<JobItem>>;
}

pub trait JobWorkspacePort {
    fn open_workspace(&self, job_id: &JobId) -> KernelResult<JobWorkspaceRef>;

    fn summarize_workspace(&self, job_id: &JobId) -> KernelResult<JobWorkspaceSummary>;

    fn remove_workspace(&self, job: &JobItem) -> KernelResult<bool>;

    fn sweep_workspaces(&self, jobs: &[JobItem]) -> KernelResult<usize>;
}

pub trait JobChunkPort {
    fn write_chunk(&self, job_id: &JobId, chunk: JobChunkWrite) -> KernelResult<()>;

    fn commit_chunk(&self, job_id: &JobId, stream: JobStreamKind, index: u64) -> KernelResult<()>;

    fn finalize_stream(
        &self,
        job_id: &JobId,
        stream: JobStreamKind,
        summary: JobWorkspaceStreamSummary,
    ) -> KernelResult<JobWorkspaceSummary>;

    fn read_chunks(
        &self,
        job_id: &JobId,
        stream: JobStreamKind,
        cursor: JobChunkCursor,
        max_chunks: usize,
    ) -> KernelResult<JobChunkRead>;
}

pub trait JobResultPort {
    fn declare_result_file(&self, job_id: &JobId, file: JobResultFile) -> KernelResult<()>;

    fn list_result_files(&self, job_id: &JobId) -> KernelResult<JobResultFileList>;

    fn read_result_file(&self, job_id: &JobId, file_id: &str) -> KernelResult<Vec<u8>>;
}
