//! Job catalog read use case.

use crate::foundation::error::KernelResult;

use super::{
    super::ports::JobCatalogPort,
    port::{
        JobCatalogReadUseCase, JobInspectRequest, JobInspectResult, JobListRequest, JobListResult,
    },
};

pub struct StdJobCatalogReadUseCase<'a> {
    catalog: &'a dyn JobCatalogPort,
}

impl<'a> StdJobCatalogReadUseCase<'a> {
    pub fn new(catalog: &'a dyn JobCatalogPort) -> Self {
        Self { catalog }
    }
}

impl JobCatalogReadUseCase for StdJobCatalogReadUseCase<'_> {
    fn list_jobs(&self, _request: JobListRequest) -> KernelResult<JobListResult> {
        Ok(JobListResult {
            jobs: self.catalog.list_jobs()?,
        })
    }

    fn inspect_job(&self, request: JobInspectRequest) -> KernelResult<JobInspectResult> {
        Ok(JobInspectResult {
            job: self.catalog.inspect_job(&request.job_id)?,
        })
    }
}
