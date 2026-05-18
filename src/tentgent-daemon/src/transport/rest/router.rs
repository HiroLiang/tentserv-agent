use axum::{
    routing::{get, post},
    Router,
};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::handlers::rest::{
    adapter, auth, chat, daemon, dataset, doctor, health, jobs, model, server, session, status,
    train,
};

use super::state::RestState;

pub fn build_router(state: RestState) -> Router {
    app_routes().with_state(state).layer(
        TraceLayer::new_for_http()
            .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
            .on_request(DefaultOnRequest::new().level(Level::INFO))
            .on_response(DefaultOnResponse::new().level(Level::INFO)),
    )
}

fn app_routes() -> Router<RestState> {
    Router::new()
        .merge(system_routes())
        .merge(diagnostic_routes())
        .merge(chat_routes())
        .merge(job_routes())
        .merge(store_routes())
        .merge(train_routes())
        .merge(server_routes())
        .merge(session_routes())
}

fn system_routes() -> Router<RestState> {
    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/v1/status", get(status::status))
}

fn diagnostic_routes() -> Router<RestState> {
    Router::new()
        .route("/v1/auth", get(auth::list))
        .route("/v1/auth/{provider}", get(auth::inspect))
        .route("/v1/doctor", get(doctor::report))
        .route("/v1/daemon/logs", get(daemon::logs))
        .route("/v1/daemon/logs/stdout", get(daemon::stdout_log))
        .route("/v1/daemon/logs/stderr", get(daemon::stderr_log))
        .route("/v1/daemon/shutdown", post(daemon::shutdown))
}

fn chat_routes() -> Router<RestState> {
    Router::new()
        .route("/v1/chat", post(chat::complete))
        .route("/v1/chat/completions", post(chat::chat_completions))
        .route("/v1/messages", post(chat::messages))
        .route("/v1beta/models/{*operation}", post(chat::generate_content))
}

fn job_routes() -> Router<RestState> {
    Router::new()
        .route("/v1/jobs", get(jobs::list))
        .route("/v1/jobs/{job_id}", get(jobs::inspect))
}

fn store_routes() -> Router<RestState> {
    Router::new()
        .route("/v1/models", get(model::list))
        .route("/v1/models/import/jobs", post(model::import_job))
        .route("/v1/models/pull/jobs", post(model::pull_job))
        .route(
            "/v1/models/{reference}",
            get(model::inspect).delete(model::remove),
        )
        .route("/v1/adapters", get(adapter::list))
        .route("/v1/adapters/import/jobs", post(adapter::import_job))
        .route("/v1/adapters/pull/jobs", post(adapter::pull_job))
        .route(
            "/v1/adapters/{reference}",
            get(adapter::inspect).delete(adapter::remove),
        )
        .route("/v1/datasets", get(dataset::list))
        .route("/v1/datasets/import/jobs", post(dataset::import_job))
        .route("/v1/datasets/synth/jobs", post(dataset::synth_job))
        .route("/v1/datasets/eval/jobs", post(dataset::eval_job))
        .route(
            "/v1/datasets/{reference}",
            get(dataset::inspect).delete(dataset::remove),
        )
}

fn train_routes() -> Router<RestState> {
    Router::new()
        .route(
            "/v1/train/lora/plans",
            get(train::list_plans).post(train::create_plan),
        )
        .route("/v1/train/lora/plans/preview", post(train::preview_plan))
        .route(
            "/v1/train/lora/plans/{reference}/runs",
            get(train::list_plan_runs).post(train::start_lora_run_job),
        )
        .route(
            "/v1/train/lora/plans/{reference}",
            get(train::inspect_plan).delete(train::remove_plan),
        )
        .route("/v1/train/lora/runs", get(train::list_runs))
        .route("/v1/train/lora/runs/{reference}", get(train::inspect_run))
        .route(
            "/v1/train/lora/runs/{reference}/metrics",
            get(train::metrics),
        )
        .route("/v1/train/lora/runs/{reference}/logs", get(train::logs))
        .route(
            "/v1/train/lora/runs/{reference}/logs/raw",
            get(train::raw_log),
        )
}

fn server_routes() -> Router<RestState> {
    Router::new()
        .route("/v1/servers", get(server::list).post(server::create))
        .route(
            "/v1/servers/{reference}",
            get(server::inspect).delete(server::remove),
        )
        .route("/v1/servers/{reference}/start", post(server::start))
        .route("/v1/servers/{reference}/stop", post(server::stop))
        .route("/v1/servers/{reference}/health", get(server::health))
        .route("/v1/servers/{reference}/logs", get(server::logs))
        .route(
            "/v1/servers/{reference}/logs/stdout",
            get(server::stdout_log),
        )
        .route(
            "/v1/servers/{reference}/logs/stderr",
            get(server::stderr_log),
        )
}

fn session_routes() -> Router<RestState> {
    Router::new()
        .route("/v1/sessions", get(session::list))
        .route("/v1/sessions/{reference}", get(session::inspect))
        .route("/v1/sessions/{reference}/messages", get(session::messages))
}
