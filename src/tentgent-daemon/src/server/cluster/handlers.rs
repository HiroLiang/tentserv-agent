use axum::{
    extract::{Path, Request as AxumRequest, State},
    response::Response,
    Json,
};
use serde_json::Value;
use tentgent_kernel::features::cluster::domain::ClusterRouteKey;

use crate::server::local::{
    claude_messages as local_claude_messages,
    gemini_generate_content as local_gemini_generate_content,
    openai_chat_completions as local_openai_chat_completions,
    openai_embeddings as local_openai_embeddings, proxy_request, LocalClaudeMessagesRequest,
    LocalGeminiGenerateContentRequest, LocalOpenAiChatCompletionRequest,
};

use super::{error::ClusterServerError, state::ClusterServerState};

pub(super) async fn openai_chat_completions(
    State(state): State<ClusterServerState>,
    Json(request): Json<LocalOpenAiChatCompletionRequest>,
) -> Result<Response, ClusterServerError> {
    let local = state.resolve_local_state(ClusterRouteKey::Chat)?;
    local_openai_chat_completions(State(local), Json(request))
        .await
        .map_err(Into::into)
}

pub(super) async fn claude_messages(
    State(state): State<ClusterServerState>,
    Json(request): Json<LocalClaudeMessagesRequest>,
) -> Result<Response, ClusterServerError> {
    let local = state.resolve_local_state(ClusterRouteKey::Chat)?;
    local_claude_messages(State(local), Json(request))
        .await
        .map_err(Into::into)
}

pub(super) async fn gemini_generate_content(
    State(state): State<ClusterServerState>,
    Path(operation): Path<String>,
    Json(request): Json<LocalGeminiGenerateContentRequest>,
) -> Result<Response, ClusterServerError> {
    let local = state.resolve_local_state(ClusterRouteKey::Chat)?;
    local_gemini_generate_content(State(local), Path(operation), Json(request))
        .await
        .map_err(Into::into)
}

pub(super) async fn embeddings(
    State(state): State<ClusterServerState>,
    Json(request): Json<Value>,
) -> Result<Response, ClusterServerError> {
    let local = state.resolve_local_state(ClusterRouteKey::Embedding)?;
    local_openai_embeddings(State(local), Json(request))
        .await
        .map_err(Into::into)
}

pub(super) async fn native_chat(
    State(state): State<ClusterServerState>,
    request: AxumRequest,
) -> Result<Response, ClusterServerError> {
    proxy_route(state, ClusterRouteKey::Chat, request).await
}

pub(super) async fn rerank(
    State(state): State<ClusterServerState>,
    request: AxumRequest,
) -> Result<Response, ClusterServerError> {
    proxy_route(state, ClusterRouteKey::Rerank, request).await
}

pub(super) async fn audio_transcription(
    State(state): State<ClusterServerState>,
    request: AxumRequest,
) -> Result<Response, ClusterServerError> {
    proxy_route(state, ClusterRouteKey::AudioTranscription, request).await
}

pub(super) async fn vision_chat(
    State(state): State<ClusterServerState>,
    request: AxumRequest,
) -> Result<Response, ClusterServerError> {
    proxy_route(state, ClusterRouteKey::VisionChat, request).await
}

async fn proxy_route(
    state: ClusterServerState,
    route: ClusterRouteKey,
    request: AxumRequest,
) -> Result<Response, ClusterServerError> {
    let local = state.resolve_local_state(route)?;
    proxy_request(State(local), request)
        .await
        .map_err(Into::into)
}

pub(super) async fn unsupported_path(request: AxumRequest) -> ClusterServerError {
    ClusterServerError::unsupported_path(request.uri().path())
}
