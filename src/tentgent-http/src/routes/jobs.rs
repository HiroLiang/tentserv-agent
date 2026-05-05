use crate::{
    app::DaemonHttpState,
    dto::ErrorResponse,
    http::HttpResponse,
    jobs::{JobResponse, JobsResponse},
    response::{json_response, not_found_response},
};

pub(crate) fn list_jobs_response(state: &DaemonHttpState) -> HttpResponse {
    json_response(
        200,
        JobsResponse {
            jobs: state.jobs().list(),
        },
    )
}

pub(crate) fn inspect_job_response(state: &DaemonHttpState, job_id: &str) -> HttpResponse {
    match state.jobs().get(job_id) {
        Some(job) => json_response(200, JobResponse { job }),
        None if job_id.is_empty() => not_found_response("/v1/jobs/"),
        None => json_response(
            404,
            ErrorResponse {
                error: "not_found",
                message: format!("job `{job_id}` was not found"),
            },
        ),
    }
}
