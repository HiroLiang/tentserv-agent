use axum::{
    body::Body,
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tentgent_kernel::features::server::domain::ServerCapability;

use crate::{
    provider_compat::{OpenAiChatCompatFields, OpenAiTextMessage, ProviderCompatRejection},
    time::unix_timestamp_seconds,
};

use super::{
    capability::{ensure_local_provider_capability, ensure_model_endpoint},
    error::LocalServerError,
    evidence::record_runtime_execution_result,
    native::{NativeLocalChatMessage, NativeLocalChatRequest, NativeLocalChatResponse},
    proxy::response_from_upstream,
    sse::openai_stream_from_local_sse,
    LocalServerState, RUNTIME_CHAT_PATH, RUNTIME_CHAT_STREAM_PATH,
};

pub(super) async fn openai_chat_completions(
    State(state): State<LocalServerState>,
    Json(request): Json<LocalOpenAiChatCompletionRequest>,
) -> Result<Response, LocalServerError> {
    ensure_local_provider_capability(
        state.config.capability,
        ServerCapability::Chat,
        "OpenAI-compatible local chat completions",
    )?;
    let endpoint = ensure_model_endpoint(&state).await?;
    let result = openai_chat_completions_to_upstream(
        &state.client,
        request,
        &endpoint.base_url,
        &state.config.model_ref,
        state.config.capability,
    )
    .await;
    record_runtime_execution_result(&state, &result);
    result
}

pub(super) async fn openai_chat_completions_to_upstream(
    client: &reqwest::Client,
    request: LocalOpenAiChatCompletionRequest,
    upstream_base_url: &str,
    bound_model_ref: &str,
    capability: ServerCapability,
) -> Result<Response, LocalServerError> {
    if capability != ServerCapability::Chat {
        return Err(ProviderCompatRejection::unsupported_capability(format!(
            "OpenAI-compatible local chat requires a chat server; this server is bound to {}",
            capability.as_str()
        ))
        .into());
    }
    let stream = request.stream.unwrap_or(false);
    let native_request = request.into_native_chat_request()?;
    let route = if stream {
        RUNTIME_CHAT_STREAM_PATH
    } else {
        RUNTIME_CHAT_PATH
    };
    let upstream = client
        .post(format!(
            "{}{}",
            upstream_base_url.trim_end_matches('/'),
            route
        ))
        .json(&native_request)
        .send()
        .await
        .map_err(|err| {
            LocalServerError::bad_gateway(format!("model runtime proxy failed: {err}"))
        })?;
    if stream {
        openai_stream_response_from_upstream(upstream, bound_model_ref).await
    } else {
        openai_response_from_upstream(upstream, bound_model_ref).await
    }
}

pub(super) async fn openai_response_from_upstream(
    upstream: reqwest::Response,
    bound_model_ref: &str,
) -> Result<Response, LocalServerError> {
    if !upstream.status().is_success() {
        return response_from_upstream(upstream);
    }
    let response = upstream
        .json::<NativeLocalChatResponse>()
        .await
        .map_err(|err| {
            LocalServerError::bad_gateway(format!("decode chat response failed: {err}"))
        })?;
    Ok(Json(json!({
        "id": format!("chatcmpl-{}", unix_timestamp_seconds()),
        "object": "chat.completion",
        "created": unix_timestamp_seconds(),
        "model": bound_model_ref,
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": response.text},
            "finish_reason": "stop",
            "logprobs": null
        }],
        "usage": null
    }))
    .into_response())
}

pub(super) async fn openai_stream_response_from_upstream(
    upstream: reqwest::Response,
    bound_model_ref: &str,
) -> Result<Response, LocalServerError> {
    if !upstream.status().is_success() {
        return response_from_upstream(upstream);
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from_stream(openai_stream_from_local_sse(
            upstream.bytes_stream(),
            bound_model_ref.to_string(),
        )))
        .map_err(|err| LocalServerError::bad_gateway(format!("build chat stream failed: {err}")))
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalOpenAiChatCompletionRequest {
    model: Option<String>,
    messages: Vec<OpenAiTextMessage>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    stream: Option<bool>,
    adapter_ref: Option<String>,
    #[serde(flatten)]
    compat: OpenAiChatCompatFields,
}

impl LocalOpenAiChatCompletionRequest {
    pub(super) fn into_native_chat_request(
        self,
    ) -> Result<NativeLocalChatRequest, LocalServerError> {
        let _caller_model = self.model.as_deref();
        self.compat.reject_unsupported()?;
        if self.adapter_ref.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "local OpenAI-compatible chat does not support adapter_ref yet",
            )
            .into());
        }
        if self.messages.is_empty() {
            return Err(LocalServerError::bad_request(
                "bad_request",
                "OpenAI-compatible chat requests must contain at least one message",
            ));
        }
        let max_tokens = self.max_tokens.or(self.compat.max_completion_tokens());
        let messages = self
            .messages
            .into_iter()
            .map(OpenAiTextMessage::into_text_message)
            .map(|message| message.map(NativeLocalChatMessage::from))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(NativeLocalChatRequest {
            messages,
            max_tokens,
            temperature: self.temperature,
        })
    }
}
