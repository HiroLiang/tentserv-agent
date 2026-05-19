use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::handlers::rest::{
    adapter, audio, auth, chat, daemon, dataset, doctor, embedding, health, jobs, model, rerank,
    server, session, status, train,
};

use super::{security::authorize_daemon_token, state::RestState};

pub fn build_router(state: RestState) -> Router {
    app_routes(state.clone()).with_state(state).layer(
        TraceLayer::new_for_http()
            .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
            .on_request(DefaultOnRequest::new().level(Level::INFO))
            .on_response(DefaultOnResponse::new().level(Level::INFO)),
    )
}

fn app_routes(state: RestState) -> Router<RestState> {
    Router::new()
        .route("/healthz", get(health::healthz))
        .merge(v1_routes().route_layer(middleware::from_fn_with_state(
            state,
            authorize_daemon_token,
        )))
}

fn v1_routes() -> Router<RestState> {
    Router::new()
        .route("/v1/status", get(status::status))
        .merge(diagnostic_routes())
        .merge(chat_routes())
        .merge(embedding_routes())
        .merge(rerank_routes())
        .merge(audio_routes())
        .merge(job_routes())
        .merge(store_routes())
        .merge(train_routes())
        .merge(server_routes())
        .merge(session_routes())
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

fn embedding_routes() -> Router<RestState> {
    Router::new().route("/v1/embeddings", post(embedding::create))
}

fn rerank_routes() -> Router<RestState> {
    Router::new().route("/v1/rerank", post(rerank::create))
}

fn audio_routes() -> Router<RestState> {
    Router::new()
        .route(
            "/v1/audio/transcriptions/job",
            post(audio::create_transcription_job_from_upload),
        )
        .route(
            "/v1/audio/transcriptions/job/{job_id}/result",
            get(audio::transcription_job_result),
        )
        .route(
            "/v1/audio/transcriptions/jobs",
            post(audio::create_transcription_job),
        )
        .route(
            "/v1/audio/transcriptions/jobs/{job_id}/result",
            get(audio::transcription_job_result),
        )
}

fn job_routes() -> Router<RestState> {
    Router::new()
        .route("/v1/jobs", get(jobs::list))
        .route("/v1/jobs/{job_id}", get(jobs::inspect).delete(jobs::delete))
        .route("/v1/jobs/{job_id}/cancel", post(jobs::cancel))
}

fn store_routes() -> Router<RestState> {
    Router::new()
        .route("/v1/models", get(model::list))
        .route("/v1/models/import", post(model::import))
        .route("/v1/models/pull", post(model::pull))
        .route("/v1/models/import/jobs", post(model::import_job))
        .route("/v1/models/pull/jobs", post(model::pull_job))
        .route(
            "/v1/models/{reference}",
            get(model::inspect)
                .patch(model::update_capability)
                .delete(model::remove),
        )
        .route("/v1/adapters", get(adapter::list))
        .route("/v1/adapters/import", post(adapter::import))
        .route("/v1/adapters/pull", post(adapter::pull))
        .route("/v1/adapters/import/jobs", post(adapter::import_job))
        .route("/v1/adapters/pull/jobs", post(adapter::pull_job))
        .route("/v1/adapters/{reference}/bind", post(adapter::bind))
        .route(
            "/v1/adapters/{reference}",
            get(adapter::inspect).delete(adapter::remove),
        )
        .route("/v1/datasets", get(dataset::list))
        .route("/v1/datasets/validate", post(dataset::validate))
        .route("/v1/datasets/template", post(dataset::template))
        .route("/v1/datasets/import", post(dataset::import))
        .route("/v1/datasets/import/jobs", post(dataset::import_job))
        .route("/v1/datasets/synth/jobs", post(dataset::synth_job))
        .route("/v1/datasets/eval/jobs", post(dataset::eval_job))
        .route("/v1/datasets/{reference}/export", post(dataset::export))
        .route("/v1/datasets/{reference}/diff", post(dataset::diff))
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
        .route("/v1/sessions", get(session::list).post(session::create))
        .route(
            "/v1/sessions/{reference}",
            get(session::inspect)
                .patch(session::update)
                .delete(session::remove),
        )
        .route(
            "/v1/sessions/{reference}/messages",
            get(session::messages).post(session::append_messages),
        )
        .route("/v1/sessions/{reference}/compact", post(session::compact))
}
