mod dto;

use axum::{
    extract::{Path, State},
    Json,
};
use tentgent_kernel::features::job::{infra::FileJobWorkspaceStore, ports::JobWorkspacePort};

use crate::{
    runtime::{JobId, JobWorkspaceSummary},
    transport::rest::{error::RestError, state::RestState},
};

pub use self::dto::{job_item, JobResponse, JobsResponse};

pub async fn list(State(state): State<RestState>) -> Json<JobsResponse> {
    Json(JobsResponse {
        jobs: state
            .app()
            .jobs()
            .list()
            .into_iter()
            .map(job_item)
            .collect(),
    })
}

pub async fn inspect(
    State(state): State<RestState>,
    Path(job_id): Path<String>,
) -> Result<Json<JobResponse>, RestError> {
    let job_id = JobId::new(job_id);
    let Some(job) = state.app().jobs().get(&job_id) else {
        return Err(RestError::not_found(
            "not_found",
            format!("job `{job_id}` was not found"),
        ));
    };

    Ok(Json(JobResponse { job: job_item(job) }))
}

pub async fn cancel(
    State(state): State<RestState>,
    Path(job_id): Path<String>,
) -> Result<Json<JobResponse>, RestError> {
    let job_id = JobId::new(job_id);
    let Some(job) = state.app().jobs().get(&job_id) else {
        return Err(RestError::not_found(
            "not_found",
            format!("job `{job_id}` was not found"),
        ));
    };
    if job.status.is_terminal() {
        return Ok(Json(JobResponse { job: job_item(job) }));
    }

    state.app().job_runner().abort(&job_id);
    let Some(job) = state
        .app()
        .jobs()
        .cancel(&job_id, "job cancellation requested")
    else {
        return Err(RestError::not_found(
            "not_found",
            format!("job `{job_id}` was not found"),
        ));
    };

    Ok(Json(JobResponse { job: job_item(job) }))
}

pub async fn delete(
    State(state): State<RestState>,
    Path(job_id): Path<String>,
) -> Result<Json<JobResponse>, RestError> {
    let job_id = JobId::new(job_id);
    let Some(job) = state.app().jobs().get(&job_id) else {
        return Err(RestError::not_found(
            "not_found",
            format!("job `{job_id}` was not found"),
        ));
    };
    if !job.status.is_terminal() {
        return Err(RestError::conflict(
            "job_active",
            format!("job `{job_id}` is active and cannot be deleted"),
        ));
    }

    FileJobWorkspaceStore::from_runtime_dir(state.app().layout().runtime_dir.clone())
        .remove_workspace(&job)
        .map_err(|err| {
            RestError::internal(
                "job_workspace_cleanup_failed",
                format!("failed to remove workspace for job `{job_id}`: {err}"),
            )
        })?;

    let Some(mut job) = state.app().jobs().delete_terminal(&job_id) else {
        return Err(RestError::not_found(
            "not_found",
            format!("job `{job_id}` was not found"),
        ));
    };
    job.workspace = Some(JobWorkspaceSummary::removed());

    Ok(Json(JobResponse { job: job_item(job) }))
}
