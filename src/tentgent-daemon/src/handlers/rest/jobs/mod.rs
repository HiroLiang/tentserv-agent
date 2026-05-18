mod dto;

use axum::{
    extract::{Path, State},
    Json,
};

use crate::{
    runtime::JobId,
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
