use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;

use super::{
    handlers::{
        audio_transcription, claude_messages, embeddings, gemini_generate_content, native_chat,
        native_chat_stream, openai_chat_completions, rerank, unsupported_path, vision_chat,
    },
    state::ClusterServerState,
};

pub(super) fn cluster_router(state: ClusterServerState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/chat/completions", post(openai_chat_completions))
        .route("/v1/messages", post(claude_messages))
        .route("/v1beta/models/{*operation}", post(gemini_generate_content))
        .route("/v1/chat", post(native_chat))
        .route("/v1/chat/stream", post(native_chat_stream))
        .route("/v1/embeddings", post(embeddings))
        .route("/v1/rerank", post(rerank))
        .route("/v1/audio/transcriptions", post(audio_transcription))
        .route("/v1/vision/chat", post(vision_chat))
        .fallback(unsupported_path)
        .with_state(state)
}

async fn healthz(State(state): State<ClusterServerState>) -> Response {
    match state.definitions.current() {
        Ok(snapshot) => {
            let chat_target = snapshot
                .definition
                .routes
                .get(&tentgent_kernel::features::cluster::domain::ClusterRouteKey::Chat);
            let chat_local = matches!(
                chat_target,
                Some(
                    tentgent_kernel::features::cluster::domain::ClusterRouteTarget::LocalModel { .. }
                )
            );
            let status = if chat_local {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            };
            (
                status,
                Json(json!({
                    "ok": chat_local,
                    "runtime_kind": "cluster-proxy",
                    "server_ref": state.config.server_ref,
                    "process_token": tentgent_kernel::features::server::infra::server_process_token_from_env(),
                    "runtime_home": state.layout.home_dir.display().to_string(),
                    "cluster_ref": state.config.cluster_ref,
                    "definition_hash": snapshot.hash,
                    "routes": snapshot.definition.routes.keys().map(|route| route.as_str()).collect::<Vec<_>>(),
                    "route_update_policy": snapshot.definition.route_update_policy,
                    "ownership": {
                        "route_claim_count": state.routes.active_claim_count(),
                        "active_request_count": state.routes.active_request_count()
                    },
                    "backend": "model-runtime-daemon"
                })),
            )
                .into_response()
        }
        Err(error) => error.into_response(),
    }
}
