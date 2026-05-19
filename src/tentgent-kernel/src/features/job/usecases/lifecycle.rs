//! Job lifecycle mutation use case.

use crate::foundation::error::KernelResult;

use super::{
    super::ports::JobCatalogPort,
    port::{
        JobCancelRequest, JobCompleteRequest, JobCreateRequest, JobCreateResult,
        JobDeleteTerminalRequest, JobFailRequest, JobInterruptActiveRequest, JobLifecycleUseCase,
        JobListResult, JobMutationResult, JobProgressUpdateRequest, JobStartRequest,
    },
};

pub struct StdJobLifecycleUseCase<'a> {
    catalog: &'a dyn JobCatalogPort,
}

impl<'a> StdJobLifecycleUseCase<'a> {
    pub fn new(catalog: &'a dyn JobCatalogPort) -> Self {
        Self { catalog }
    }
}

impl JobLifecycleUseCase for StdJobLifecycleUseCase<'_> {
    fn create_job(&self, request: JobCreateRequest) -> KernelResult<JobCreateResult> {
        Ok(JobCreateResult {
            job: self.catalog.create_job(request.record)?,
        })
    }

    fn start_job(&self, request: JobStartRequest) -> KernelResult<JobMutationResult> {
        Ok(JobMutationResult {
            job: self.catalog.start_job(&request.job_id, request.stage)?,
        })
    }

    fn update_job_progress(
        &self,
        request: JobProgressUpdateRequest,
    ) -> KernelResult<JobMutationResult> {
        Ok(JobMutationResult {
            job: self
                .catalog
                .update_job_progress(&request.job_id, request.update)?,
        })
    }

    fn complete_job(&self, request: JobCompleteRequest) -> KernelResult<JobMutationResult> {
        Ok(JobMutationResult {
            job: self.catalog.succeed_job(
                &request.job_id,
                request.artifact,
                request.result_summary,
            )?,
        })
    }

    fn fail_job(&self, request: JobFailRequest) -> KernelResult<JobMutationResult> {
        Ok(JobMutationResult {
            job: self
                .catalog
                .fail_job(&request.job_id, request.error_summary)?,
        })
    }

    fn cancel_job(&self, request: JobCancelRequest) -> KernelResult<JobMutationResult> {
        Ok(JobMutationResult {
            job: self.catalog.cancel_job(&request.job_id, request.reason)?,
        })
    }

    fn delete_terminal_job(
        &self,
        request: JobDeleteTerminalRequest,
    ) -> KernelResult<JobMutationResult> {
        Ok(JobMutationResult {
            job: self.catalog.delete_terminal_job(&request.job_id)?,
        })
    }

    fn interrupt_active_jobs(
        &self,
        request: JobInterruptActiveRequest,
    ) -> KernelResult<JobListResult> {
        Ok(JobListResult {
            jobs: self.catalog.interrupt_active_jobs(request.reason)?,
        })
    }
}
