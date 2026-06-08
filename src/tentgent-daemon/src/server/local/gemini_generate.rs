use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tentgent_kernel::features::server::domain::ServerCapability;

use crate::provider_compat::ProviderCompatRejection;

use super::{
    capability::{ensure_local_provider_capability, ensure_model_endpoint},
    error::LocalServerError,
    native::{NativeLocalChatMessage, NativeLocalChatRequest, NativeLocalChatResponse},
    proxy::response_from_upstream,
    sse::gemini_stream_from_local_sse,
    LocalServerState, RUNTIME_CHAT_PATH, RUNTIME_CHAT_STREAM_PATH,
};

pub(super) async fn gemini_generate_content(
    State(state): State<LocalServerState>,
    Path(operation): Path<String>,
    Json(request): Json<LocalGeminiGenerateContentRequest>,
) -> Result<Response, LocalServerError> {
    ensure_local_provider_capability(
        state.config.capability,
        ServerCapability::Chat,
        "Gemini-compatible local generateContent",
    )?;
    let endpoint = ensure_model_endpoint(&state).await?;
    gemini_generate_content_to_upstream(
        &state.client,
        request,
        &operation,
        &endpoint.base_url,
        &state.config.model_ref,
        state.config.capability,
    )
    .await
}

pub(super) async fn gemini_generate_content_to_upstream(
    client: &reqwest::Client,
    request: LocalGeminiGenerateContentRequest,
    operation: &str,
    upstream_base_url: &str,
    bound_model_ref: &str,
    capability: ServerCapability,
) -> Result<Response, LocalServerError> {
    if capability != ServerCapability::Chat {
        return Err(ProviderCompatRejection::unsupported_capability(format!(
            "Gemini-compatible local generateContent requires a chat server; this server is bound to {}",
            capability.as_str()
        ))
        .into());
    }
    let operation = GeminiLocalOperation::parse(operation)?;
    let stream = operation.stream;
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
        gemini_stream_response_from_upstream(upstream, bound_model_ref).await
    } else {
        gemini_response_from_upstream(upstream, bound_model_ref).await
    }
}

pub(super) async fn gemini_response_from_upstream(
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
    Ok(Json(gemini_response(
        bound_model_ref,
        Some(response.text),
        Some("STOP"),
    ))
    .into_response())
}

pub(super) async fn gemini_stream_response_from_upstream(
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
        .body(Body::from_stream(gemini_stream_from_local_sse(
            upstream.bytes_stream(),
            bound_model_ref.to_string(),
        )))
        .map_err(|err| LocalServerError::bad_gateway(format!("build chat stream failed: {err}")))
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalGeminiGenerateContentRequest {
    contents: Vec<LocalGeminiContent>,
    #[serde(alias = "systemInstruction")]
    system_instruction: Option<LocalGeminiContent>,
    #[serde(alias = "generationConfig")]
    generation_config: Option<LocalGeminiGenerationConfig>,
    tools: Option<Value>,
    #[serde(alias = "toolConfig")]
    tool_config: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalGeminiGenerationConfig {
    #[serde(alias = "maxOutputTokens")]
    max_output_tokens: Option<u32>,
    temperature: Option<f32>,
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalGeminiContent {
    role: Option<String>,
    parts: Vec<LocalGeminiPart>,
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalGeminiPart {
    text: Option<String>,
}

struct GeminiLocalOperation {
    stream: bool,
}

impl GeminiLocalOperation {
    fn parse(operation: &str) -> Result<Self, LocalServerError> {
        if operation.strip_suffix(":generateContent").is_some() {
            return Ok(Self { stream: false });
        }
        if operation.strip_suffix(":streamGenerateContent").is_some() {
            return Ok(Self { stream: true });
        }
        Err(ProviderCompatRejection::unsupported_operation(
            "unsupported Gemini generateContent operation",
        )
        .into())
    }
}

impl LocalGeminiGenerateContentRequest {
    pub(super) fn into_native_chat_request(
        self,
    ) -> Result<NativeLocalChatRequest, LocalServerError> {
        self.reject_unsupported()?;
        if self.contents.is_empty() {
            return Err(LocalServerError::bad_request(
                "bad_request",
                "Gemini-compatible generateContent requests must contain at least one content",
            ));
        }
        let mut messages = Vec::new();
        if let Some(system) = self.system_instruction {
            messages.push(NativeLocalChatMessage {
                role: "system".to_string(),
                content: gemini_content_text(system)?,
            });
        }
        messages.extend(
            self.contents
                .into_iter()
                .map(LocalGeminiContent::into_native)
                .collect::<Result<Vec<_>, _>>()?,
        );
        let generation_config = self.generation_config.unwrap_or_default();
        Ok(NativeLocalChatRequest {
            messages,
            max_tokens: generation_config.max_output_tokens,
            temperature: generation_config.temperature,
        })
    }

    fn reject_unsupported(&self) -> Result<(), LocalServerError> {
        if self.tools.is_some() || self.tool_config.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "Gemini-compatible tools require kernel tool-call support",
            )
            .into());
        }
        Ok(())
    }
}

impl Default for LocalGeminiGenerationConfig {
    fn default() -> Self {
        Self {
            max_output_tokens: None,
            temperature: None,
        }
    }
}

impl LocalGeminiContent {
    fn into_native(self) -> Result<NativeLocalChatMessage, LocalServerError> {
        Ok(NativeLocalChatMessage {
            role: gemini_role(self.role.as_deref())?,
            content: gemini_content_text(self)?,
        })
    }
}

fn gemini_role(role: Option<&str>) -> Result<String, LocalServerError> {
    match role.unwrap_or("user").trim().to_ascii_lowercase().as_str() {
        "user" => Ok("user".to_string()),
        "model" | "assistant" => Ok("assistant".to_string()),
        "system" => Ok("system".to_string()),
        "" => Err(LocalServerError::bad_request(
            "bad_request",
            "content role is empty",
        )),
        other => Err(LocalServerError::bad_request(
            "bad_request",
            format!("unsupported Gemini content role `{other}`"),
        )),
    }
}

fn gemini_content_text(content: LocalGeminiContent) -> Result<String, LocalServerError> {
    let mut text = String::new();
    for part in content.parts {
        match part.text {
            Some(value) => text.push_str(&value),
            None => {
                return Err(ProviderCompatRejection::unsupported_content(
                    "only Gemini text parts are supported by local model-bound servers",
                )
                .into());
            }
        }
    }
    Ok(text)
}

pub(super) fn gemini_response(
    bound_model_ref: &str,
    text: Option<String>,
    finish_reason: Option<&str>,
) -> Value {
    let parts = text
        .map(|text| vec![json!({ "text": text })])
        .unwrap_or_default();
    json!({
        "candidates": [{
            "index": 0,
            "content": {
                "role": "model",
                "parts": parts
            },
            "finishReason": finish_reason
        }],
        "usageMetadata": null,
        "modelVersion": bound_model_ref
    })
}
