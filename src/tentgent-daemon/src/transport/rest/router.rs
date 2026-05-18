use axum::{
    routing::{get, post},
    Router,
};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::handlers::rest::{
    adapter, dataset, health, jobs, model, server, session, status, train,
};

use super::state::RestState;

pub fn build_router(state: RestState) -> Router {
    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/v1/status", get(status::status))
        .route("/v1/jobs", get(jobs::list))
        .route("/v1/jobs/{job_id}", get(jobs::inspect))
        .route("/v1/models", get(model::list))
        .route("/v1/models/import/jobs", post(model::import_job))
        .route("/v1/models/pull/jobs", post(model::pull_job))
        .route("/v1/models/{reference}", get(model::inspect))
        .route("/v1/adapters", get(adapter::list))
        .route("/v1/adapters/import/jobs", post(adapter::import_job))
        .route("/v1/adapters/pull/jobs", post(adapter::pull_job))
        .route("/v1/adapters/{reference}", get(adapter::inspect))
        .route("/v1/datasets", get(dataset::list))
        .route("/v1/datasets/import/jobs", post(dataset::import_job))
        .route("/v1/datasets/synth/jobs", post(dataset::synth_job))
        .route("/v1/datasets/eval/jobs", post(dataset::eval_job))
        .route("/v1/datasets/{reference}", get(dataset::inspect))
        .route(
            "/v1/train/lora/plans/{reference}/runs",
            post(train::start_lora_run_job),
        )
        .route("/v1/servers", get(server::list))
        .route("/v1/servers/{reference}", get(server::inspect))
        .route("/v1/sessions", get(session::list))
        .route("/v1/sessions/{reference}", get(session::inspect))
        .route("/v1/sessions/{reference}/messages", get(session::messages))
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
}
