use axum::{routing::get, Router};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::handlers::rest::{adapter, dataset, health, model, status};

use super::state::RestState;

pub fn build_router(state: RestState) -> Router {
    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/v1/status", get(status::status))
        .route("/v1/models", get(model::list))
        .route("/v1/models/{reference}", get(model::inspect))
        .route("/v1/adapters", get(adapter::list))
        .route("/v1/adapters/{reference}", get(adapter::inspect))
        .route("/v1/datasets", get(dataset::list))
        .route("/v1/datasets/{reference}", get(dataset::inspect))
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
}
