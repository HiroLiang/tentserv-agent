//! Job workspace metadata use case.

use crate::foundation::error::KernelResult;

use super::{
    super::ports::JobCatalogPort,
    port::{JobMutationResult, JobWorkspaceUpdateRequest, JobWorkspaceUseCase},
};

pub struct StdJobWorkspaceUseCase<'a> {
    catalog: &'a dyn JobCatalogPort,
}

impl<'a> StdJobWorkspaceUseCase<'a> {
    pub fn new(catalog: &'a dyn JobCatalogPort) -> Self {
        Self { catalog }
    }
}

impl JobWorkspaceUseCase for StdJobWorkspaceUseCase<'_> {
    fn update_job_workspace(
        &self,
        request: JobWorkspaceUpdateRequest,
    ) -> KernelResult<JobMutationResult> {
        Ok(JobMutationResult {
            job: self
                .catalog
                .update_job_workspace(&request.job_id, request.workspace)?,
        })
    }
}
