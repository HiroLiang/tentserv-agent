use axum::{
    body::Body,
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tentgent_kernel::features::server::domain::ServerCapability;

use crate::{provider_compat::ProviderCompatRejection, time::unix_timestamp_seconds};

use super::{
    capability::{ensure_local_provider_capability, ensure_model_endpoint},
    error::LocalServerError,
    native::{NativeLocalChatMessage, NativeLocalChatRequest, NativeLocalChatResponse},
    proxy::response_from_upstream,
    sse::claude_stream_from_local_sse,
    LocalServerState, RUNTIME_CHAT_PATH, RUNTIME_CHAT_STREAM_PATH,
};

pub(super) async fn claude_messages(
    State(state): State<LocalServerState>,
    Json(request): Json<LocalClaudeMessagesRequest>,
) -> Result<Response, LocalServerError> {
    ensure_local_provider_capability(
        state.config.capability,
        ServerCapability::Chat,
        "Claude-compatible local messages",
    )?;
    let endpoint = ensure_model_endpoint(&state).await?;
    claude_messages_to_upstream(
        &state.client,
        request,
        &endpoint.base_url,
        &state.config.model_ref,
        state.config.capability,
    )
    .await
}

pub(super) async fn claude_messages_to_upstream(
    client: &reqwest::Client,
    request: LocalClaudeMessagesRequest,
    upstream_base_url: &str,
    bound_model_ref: &str,
    capability: ServerCapability,
) -> Result<Response, LocalServerError> {
    if capability != ServerCapability::Chat {
        return Err(ProviderCompatRejection::unsupported_capability(format!(
            "Claude-compatible local messages require a chat server; this server is bound to {}",
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
        claude_stream_response_from_upstream(upstream, bound_model_ref).await
    } else {
        claude_response_from_upstream(upstream, bound_model_ref).await
    }
}

pub(super) async fn claude_response_from_upstream(
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
        "id": format!("msg-{}", unix_timestamp_seconds()),
        "type": "message",
        "role": "assistant",
        "content": [{
            "type": "text",
            "text": response.text
        }],
        "model": bound_model_ref,
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": null
    }))
    .into_response())
}

pub(super) async fn claude_stream_response_from_upstream(
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
        .body(Body::from_stream(claude_stream_from_local_sse(
            upstream.bytes_stream(),
            bound_model_ref.to_string(),
        )))
        .map_err(|err| LocalServerError::bad_gateway(format!("build chat stream failed: {err}")))
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalClaudeMessagesRequest {
    model: String,
    messages: Vec<LocalClaudeMessage>,
    system: Option<LocalClaudeContent>,
    max_tokens: u32,
    temperature: Option<f32>,
    stream: Option<bool>,
    tools: Option<Value>,
    tool_choice: Option<Value>,
    audio: Option<Value>,
    modalities: Option<Value>,
    input_audio: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalClaudeMessage {
    role: String,
    content: LocalClaudeContent,
    audio: Option<Value>,
    input_audio: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum LocalClaudeContent {
    Text(String),
    Blocks(Vec<LocalClaudeContentBlock>),
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalClaudeContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

impl LocalClaudeMessagesRequest {
    pub(super) fn into_native_chat_request(
        self,
    ) -> Result<NativeLocalChatRequest, LocalServerError> {
        let _caller_model = self.model.as_str();
        self.reject_unsupported()?;
        if self.messages.is_empty() {
            return Err(LocalServerError::bad_request(
                "bad_request",
                "Claude-compatible messages requests must contain at least one message",
            ));
        }
        let mut messages = Vec::new();
        if let Some(system) = self.system {
            messages.push(NativeLocalChatMessage {
                role: "system".to_string(),
                content: claude_text_content(system)?,
            });
        }
        messages.extend(
            self.messages
                .into_iter()
                .map(LocalClaudeMessage::into_native)
                .collect::<Result<Vec<_>, _>>()?,
        );
        Ok(NativeLocalChatRequest {
            messages,
            max_tokens: Some(self.max_tokens),
            temperature: self.temperature,
        })
    }

    fn reject_unsupported(&self) -> Result<(), LocalServerError> {
        if self.tools.is_some() || self.tool_choice.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "Claude-compatible tools require kernel tool-call support",
            )
            .into());
        }
        if self.audio.is_some() || self.modalities.is_some() || self.input_audio.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "Claude-compatible audio input and output are not supported by Tentgent local compatibility yet",
            )
            .into());
        }
        Ok(())
    }
}

impl LocalClaudeMessage {
    fn into_native(self) -> Result<NativeLocalChatMessage, LocalServerError> {
        if self.audio.is_some() || self.input_audio.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "Claude-compatible message audio fields are not supported by Tentgent local compatibility yet",
            )
            .into());
        }
        Ok(NativeLocalChatMessage {
            role: claude_text_role(&self.role)?,
            content: claude_text_content(self.content)?,
        })
    }
}

fn claude_text_role(role: &str) -> Result<String, LocalServerError> {
    match role.trim().to_ascii_lowercase().as_str() {
        "system" => Ok("system".to_string()),
        "user" => Ok("user".to_string()),
        "assistant" => Ok("assistant".to_string()),
        "" => Err(LocalServerError::bad_request(
            "bad_request",
            "message role is empty",
        )),
        other => Err(LocalServerError::bad_request(
            "bad_request",
            format!("unsupported Claude message role `{other}`"),
        )),
    }
}

fn claude_text_content(content: LocalClaudeContent) -> Result<String, LocalServerError> {
    match content {
        LocalClaudeContent::Text(text) => Ok(text),
        LocalClaudeContent::Blocks(blocks) => {
            let mut text = String::new();
            for block in blocks {
                if block.kind != "text" {
                    return Err(ProviderCompatRejection::unsupported_content(format!(
                        "unsupported Claude content block `{}`",
                        block.kind
                    ))
                    .into());
                }
                text.push_str(block.text.as_deref().unwrap_or_default());
            }
            Ok(text)
        }
    }
}
