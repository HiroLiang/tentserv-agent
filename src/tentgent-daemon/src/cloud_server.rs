use std::net::SocketAddr;

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tentgent_kernel::{
    features::{
        auth::domain::Provider,
        cloud::{
            domain::{
                CloudChatContentPart, CloudChatMessage, CloudChatRequest, CloudEmbeddingRequest,
                CloudEndpointCapability, CloudImageGenerationRequest,
            },
            infra::ReqwestCloudModelClient,
        },
    },
    foundation::error::KernelError,
};

use crate::{
    provider_compat::{ensure_provider_capability, ProviderCompatRejection},
    time::unix_timestamp_seconds,
};

#[derive(Debug, Clone)]
pub struct CloudServerRuntimeConfig {
    pub server_ref: String,
    pub provider: Provider,
    pub provider_model: String,
    pub host: String,
    pub port: u16,
    pub runtime_home: Option<String>,
}

#[derive(Clone)]
struct CloudServerState {
    config: CloudServerRuntimeConfig,
    secret: String,
}

pub async fn run_cloud_server_runtime(config: CloudServerRuntimeConfig) -> miette::Result<()> {
    let secret = std::env::var(config.provider.env_var()).map_err(|_| {
        miette::miette!(
            "{} is required to launch cloud server {}",
            config.provider.env_var(),
            config.server_ref
        )
    })?;
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .map_err(|err| miette::miette!("invalid cloud server bind address: {err}"))?;
    let state = CloudServerState { config, secret };
    let router = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/chat", post(chat))
        .route("/v1/chat/completions", post(openai_chat))
        .route("/v1/messages", post(claude_messages))
        .route("/v1beta/models/{*operation}", post(gemini_generate_content))
        .route("/v1/embeddings", post(embeddings))
        .route("/v1/images/generations", post(images))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|err| miette::miette!("cloud server bind failed: {err}"))?;
    axum::serve(listener, router)
        .await
        .map_err(|err| miette::miette!("cloud server failed: {err}"))
}

async fn gemini_generate_content(
    State(state): State<CloudServerState>,
    Path(operation): Path<String>,
    Json(request): Json<GeminiGenerateContentRequest>,
) -> Result<Response, CloudServerError> {
    request.reject_unsupported()?;
    let stream = gemini_operation_stream(&operation)?;
    let mut messages = Vec::new();
    if let Some(system) = request.system_instruction {
        messages.push(CloudChatMessage {
            role: "system".to_string(),
            content: gemini_parts_into_cloud(system.parts)?,
        });
    }
    for content in request.contents {
        messages.push(CloudChatMessage {
            role: content.role.unwrap_or_else(|| "user".to_string()),
            content: gemini_parts_into_cloud(content.parts)?,
        });
    }
    let cloud_request = CloudChatRequest {
        provider: state.config.provider,
        model: state.config.provider_model.clone(),
        messages,
        max_tokens: request
            .generation_config
            .as_ref()
            .and_then(|config| config.max_output_tokens),
        temperature: request
            .generation_config
            .as_ref()
            .and_then(|config| config.temperature),
        stream,
    };
    if stream {
        return stream_response(state, cloud_request).await;
    }
    let client = ReqwestCloudModelClient::new()?;
    let response = client.complete_chat(cloud_request, &state.secret).await?;
    Ok(Json(json!({
        "candidates": [{
            "index": 0,
            "content": {
                "role": "model",
                "parts": [{"text": response.text}]
            },
            "finishReason": response.finish_reason
        }],
        "usageMetadata": null,
        "modelVersion": state.config.provider_model,
    }))
    .into_response())
}

async fn healthz(State(state): State<CloudServerState>) -> Json<serde_json::Value> {
    Json(json!({
        "ok": true,
        "runtime_kind": "cloud",
        "server_ref": state.config.server_ref,
        "runtime_home": state.config.runtime_home,
        "provider": state.config.provider.cli_name(),
        "model": state.config.provider_model,
    }))
}

async fn chat(
    State(state): State<CloudServerState>,
    Json(request): Json<NativeChatRequest>,
) -> Result<Response, CloudServerError> {
    let stream = request.stream.unwrap_or(false);
    let cloud_request = CloudChatRequest {
        provider: state.config.provider,
        model: state.config.provider_model.clone(),
        messages: request
            .messages
            .into_iter()
            .map(|message| CloudChatMessage::text(message.role, message.content))
            .collect(),
        max_tokens: request.max_tokens,
        temperature: request.temperature,
        stream,
    };
    if stream {
        return stream_response(state, cloud_request).await;
    }
    let client = ReqwestCloudModelClient::new()?;
    let response = client.complete_chat(cloud_request, &state.secret).await?;
    Ok(Json(json!({
        "text": response.text,
        "finish_reason": response.finish_reason,
        "model_ref": state.config.provider_model,
        "adapter_ref": null
    }))
    .into_response())
}

async fn openai_chat(
    State(state): State<CloudServerState>,
    Json(request): Json<OpenAiChatRequest>,
) -> Result<Response, CloudServerError> {
    request.reject_unsupported()?;
    let stream = request.stream.unwrap_or(false);
    let cloud_request = CloudChatRequest {
        provider: state.config.provider,
        model: state.config.provider_model.clone(),
        messages: request
            .messages
            .into_iter()
            .map(OpenAiMessage::into_cloud)
            .collect::<Result<Vec<_>, _>>()?,
        max_tokens: request.max_tokens.or(request.max_completion_tokens),
        temperature: request.temperature,
        stream,
    };
    if stream {
        return stream_response(state, cloud_request).await;
    }
    let client = ReqwestCloudModelClient::new()?;
    let response = client.complete_chat(cloud_request, &state.secret).await?;
    Ok(Json(json!({
        "id": format!("chatcmpl-{}", unix_timestamp_seconds()),
        "object": "chat.completion",
        "created": unix_timestamp_seconds(),
        "model": state.config.provider_model,
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": response.text},
            "finish_reason": response.finish_reason
        }],
        "usage": null
    }))
    .into_response())
}

async fn claude_messages(
    State(state): State<CloudServerState>,
    Json(request): Json<ClaudeMessagesRequest>,
) -> Result<Response, CloudServerError> {
    request.reject_unsupported()?;
    let mut messages = Vec::new();
    if let Some(system) = request.system {
        messages.push(CloudChatMessage::text("system", system));
    }
    messages.extend(
        request
            .messages
            .into_iter()
            .map(ClaudeMessage::into_cloud)
            .collect::<Result<Vec<_>, _>>()?,
    );
    let cloud_request = CloudChatRequest {
        provider: state.config.provider,
        model: state.config.provider_model.clone(),
        messages,
        max_tokens: request.max_tokens,
        temperature: request.temperature,
        stream: false,
    };
    let client = ReqwestCloudModelClient::new()?;
    let response = client.complete_chat(cloud_request, &state.secret).await?;
    Ok(Json(json!({
        "id": format!("msg-{}", unix_timestamp_seconds()),
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": response.text}],
        "model": state.config.provider_model,
        "stop_reason": response.finish_reason,
        "usage": null
    }))
    .into_response())
}

async fn embeddings(
    State(state): State<CloudServerState>,
    Json(request): Json<EmbeddingRequest>,
) -> Result<Json<EmbeddingResponse>, CloudServerError> {
    request.reject_unsupported()?;
    ensure_provider_capability(state.config.provider, CloudEndpointCapability::Embedding)?;
    let client = ReqwestCloudModelClient::new()?;
    let response = client
        .create_embedding(
            CloudEmbeddingRequest {
                provider: state.config.provider,
                model: state.config.provider_model.clone(),
                input: request.input.into_items(),
            },
            &state.secret,
        )
        .await?;
    Ok(Json(EmbeddingResponse {
        model_ref: state.config.provider_model,
        data: response
            .vectors
            .into_iter()
            .enumerate()
            .map(|(index, embedding)| EmbeddingItem { index, embedding })
            .collect(),
    }))
}

async fn images(
    State(state): State<CloudServerState>,
    Json(request): Json<ImageRequest>,
) -> Result<Json<ImageResponse>, CloudServerError> {
    request.reject_unsupported()?;
    ensure_provider_capability(
        state.config.provider,
        CloudEndpointCapability::ImageGeneration,
    )?;
    let client = ReqwestCloudModelClient::new()?;
    let response = client
        .generate_image(
            CloudImageGenerationRequest {
                provider: state.config.provider,
                model: state.config.provider_model.clone(),
                prompt: request.prompt,
                size: request.size,
            },
            &state.secret,
        )
        .await?;
    Ok(Json(ImageResponse {
        created: unix_timestamp_seconds(),
        data: vec![ImageData {
            b64_json: response.b64_json,
        }],
    }))
}

async fn stream_response(
    state: CloudServerState,
    mut request: CloudChatRequest,
) -> Result<Response, CloudServerError> {
    use axum::response::sse::{Event, Sse};
    use futures_util::stream;
    use std::convert::Infallible;

    request.stream = false;
    let client = ReqwestCloudModelClient::new()?;
    let response = client.complete_chat(request, &state.secret).await?;
    let mut events = Vec::new();
    if !response.text.is_empty() {
        events.push(Ok(Event::default()
            .event("delta")
            .data(json!({"delta": response.text}).to_string())));
    }
    events.push(Ok(Event::default()
        .event("done")
        .data(json!({"finish_reason": response.finish_reason}).to_string())));
    let stream = stream::iter(
        events
            .into_iter()
            .collect::<Vec<Result<Event, Infallible>>>(),
    );
    Ok(Sse::new(stream).into_response())
}

#[derive(Debug, Deserialize)]
struct NativeChatRequest {
    messages: Vec<NativeMessage>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    stream: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct NativeMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatRequest {
    messages: Vec<OpenAiMessage>,
    max_tokens: Option<u32>,
    max_completion_tokens: Option<u32>,
    temperature: Option<f32>,
    stream: Option<bool>,
    tools: Option<Value>,
    tool_choice: Option<Value>,
    functions: Option<Value>,
    function_call: Option<Value>,
    response_format: Option<Value>,
    modalities: Option<Vec<String>>,
    audio: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    role: String,
    content: OpenAiContent,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OpenAiContent {
    Text(String),
    Parts(Vec<OpenAiPart>),
}

#[derive(Debug, Deserialize)]
struct OpenAiPart {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
    image_url: Option<OpenAiImageUrl>,
}

#[derive(Debug, Deserialize)]
struct OpenAiImageUrl {
    url: String,
}

impl OpenAiMessage {
    fn into_cloud(self) -> Result<CloudChatMessage, CloudServerError> {
        let content = match self.content {
            OpenAiContent::Text(text) => vec![CloudChatContentPart::Text(text)],
            OpenAiContent::Parts(parts) => parts
                .into_iter()
                .map(|part| match part.kind.as_str() {
                    "text" => Ok::<CloudChatContentPart, CloudServerError>(
                        CloudChatContentPart::Text(part.text.unwrap_or_default()),
                    ),
                    "image_url" => Ok(CloudChatContentPart::ImageUrl {
                        url: part
                            .image_url
                            .map(|image| image.url)
                            .ok_or_else(|| CloudServerError::bad_request("image_url is missing"))?,
                    }),
                    other => Err(CloudServerError::from(
                        ProviderCompatRejection::unsupported_content(format!(
                            "unsupported OpenAI content part `{other}`"
                        )),
                    )),
                })
                .collect::<Result<Vec<_>, CloudServerError>>()?,
        };
        Ok(CloudChatMessage {
            role: self.role,
            content,
        })
    }
}

impl OpenAiChatRequest {
    fn reject_unsupported(&self) -> Result<(), ProviderCompatRejection> {
        if self.tools.is_some()
            || self.tool_choice.is_some()
            || self.functions.is_some()
            || self.function_call.is_some()
        {
            return Err(ProviderCompatRejection::unsupported_field(
                "OpenAI-compatible tools and function calling require kernel tool-call support",
            ));
        }
        if self.response_format.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "OpenAI-compatible response_format is not supported by Tentgent chat compatibility yet",
            ));
        }
        if self.audio.is_some()
            || self
                .modalities
                .as_ref()
                .is_some_and(|modalities| modalities.iter().any(|value| value != "text"))
        {
            return Err(ProviderCompatRejection::unsupported_field(
                "OpenAI-compatible audio output requires kernel multimodal support",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct ClaudeMessagesRequest {
    messages: Vec<ClaudeMessage>,
    system: Option<String>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    stream: Option<bool>,
    tools: Option<Value>,
    tool_choice: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ClaudeMessage {
    role: String,
    content: ClaudeContent,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ClaudeContent {
    Text(String),
    Blocks(Vec<ClaudeContentBlock>),
}

#[derive(Debug, Deserialize)]
struct ClaudeContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
    source: Option<ClaudeImageSource>,
}

#[derive(Debug, Deserialize)]
struct ClaudeImageSource {
    #[serde(rename = "type")]
    kind: String,
    media_type: String,
    data: String,
}

impl ClaudeMessage {
    fn into_cloud(self) -> Result<CloudChatMessage, CloudServerError> {
        let content = match self.content {
            ClaudeContent::Text(text) => vec![CloudChatContentPart::Text(text)],
            ClaudeContent::Blocks(blocks) => blocks
                .into_iter()
                .map(|block| match block.kind.as_str() {
                    "text" => Ok::<CloudChatContentPart, CloudServerError>(
                        CloudChatContentPart::Text(block.text.unwrap_or_default()),
                    ),
                    "image" => {
                        let source = block.source.ok_or_else(|| {
                            CloudServerError::bad_request("Claude image source is missing")
                        })?;
                        if source.kind != "base64" {
                            return Err(CloudServerError::from(
                                ProviderCompatRejection::unsupported_content(format!(
                                    "unsupported Claude image source `{}`",
                                    source.kind
                                )),
                            ));
                        }
                        Ok(CloudChatContentPart::ImageBase64 {
                            media_type: source.media_type,
                            data: source.data,
                        })
                    }
                    other => Err(CloudServerError::from(
                        ProviderCompatRejection::unsupported_content(format!(
                            "unsupported Claude content block `{other}`"
                        )),
                    )),
                })
                .collect::<Result<Vec<_>, CloudServerError>>()?,
        };
        Ok(CloudChatMessage {
            role: self.role,
            content,
        })
    }
}

impl ClaudeMessagesRequest {
    fn reject_unsupported(&self) -> Result<(), ProviderCompatRejection> {
        if self.tools.is_some() || self.tool_choice.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "Claude-compatible tools require kernel tool-call support",
            ));
        }
        if self.stream.unwrap_or(false) {
            return Err(ProviderCompatRejection::unsupported_field(
                "direct cloud Claude messages do not support stream=true yet",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct GeminiGenerateContentRequest {
    contents: Vec<GeminiContent>,
    #[serde(alias = "systemInstruction")]
    system_instruction: Option<GeminiContent>,
    #[serde(alias = "generationConfig")]
    generation_config: Option<GeminiGenerationConfig>,
    tools: Option<Value>,
    #[serde(alias = "toolConfig")]
    tool_config: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct GeminiGenerationConfig {
    #[serde(alias = "maxOutputTokens")]
    max_output_tokens: Option<u32>,
    temperature: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct GeminiContent {
    role: Option<String>,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Deserialize)]
struct GeminiPart {
    text: Option<String>,
    #[serde(alias = "inlineData")]
    inline_data: Option<GeminiInlineData>,
}

#[derive(Debug, Deserialize)]
struct GeminiInlineData {
    #[serde(alias = "mimeType")]
    mime_type: String,
    data: String,
}

fn gemini_parts_into_cloud(
    parts: Vec<GeminiPart>,
) -> Result<Vec<CloudChatContentPart>, CloudServerError> {
    parts
        .into_iter()
        .map(|part| {
            if let Some(text) = part.text {
                return Ok(CloudChatContentPart::Text(text));
            }
            if let Some(data) = part.inline_data {
                return Ok(CloudChatContentPart::ImageBase64 {
                    media_type: data.mime_type,
                    data: data.data,
                });
            }
            Err(ProviderCompatRejection::unsupported_content("unsupported Gemini part").into())
        })
        .collect()
}

impl GeminiGenerateContentRequest {
    fn reject_unsupported(&self) -> Result<(), ProviderCompatRejection> {
        if self.tools.is_some() || self.tool_config.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "Gemini-compatible tools require kernel tool-call support",
            ));
        }
        Ok(())
    }
}

fn gemini_operation_stream(operation: &str) -> Result<bool, ProviderCompatRejection> {
    if operation.strip_suffix(":generateContent").is_some() {
        return Ok(false);
    }
    if operation.strip_suffix(":streamGenerateContent").is_some() {
        return Ok(true);
    }
    Err(ProviderCompatRejection::unsupported_operation(
        "unsupported Gemini generateContent operation",
    ))
}

#[derive(Debug, Deserialize)]
struct EmbeddingRequest {
    input: EmbeddingInput,
    dimensions: Option<Value>,
}

impl EmbeddingRequest {
    fn reject_unsupported(&self) -> Result<(), ProviderCompatRejection> {
        if self.dimensions.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "provider-compatible embeddings do not support dimensions overrides yet",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum EmbeddingInput {
    One(String),
    Many(Vec<String>),
}

impl EmbeddingInput {
    fn into_items(self) -> Vec<String> {
        match self {
            Self::One(value) => vec![value],
            Self::Many(values) => values,
        }
    }
}

#[derive(Debug, Serialize)]
struct EmbeddingResponse {
    model_ref: String,
    data: Vec<EmbeddingItem>,
}

#[derive(Debug, Serialize)]
struct EmbeddingItem {
    index: usize,
    embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct ImageRequest {
    prompt: String,
    size: Option<String>,
    response_format: Option<Value>,
    n: Option<Value>,
}

impl ImageRequest {
    fn reject_unsupported(&self) -> Result<(), ProviderCompatRejection> {
        if self.response_format.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "provider-compatible image generation response_format is not supported; Tentgent returns b64_json",
            ));
        }
        if self.n.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "provider-compatible image generation only supports one image per request today",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
struct ImageResponse {
    created: u64,
    data: Vec<ImageData>,
}

#[derive(Debug, Serialize)]
struct ImageData {
    b64_json: String,
}

#[derive(Debug)]
struct CloudServerError {
    status: axum::http::StatusCode,
    code: &'static str,
    message: String,
}

impl CloudServerError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: axum::http::StatusCode::BAD_REQUEST,
            code: "bad_request",
            message: message.into(),
        }
    }
}

impl From<ProviderCompatRejection> for CloudServerError {
    fn from(rejection: ProviderCompatRejection) -> Self {
        let (code, message) = rejection.into_parts();
        Self {
            status: axum::http::StatusCode::BAD_REQUEST,
            code,
            message,
        }
    }
}

impl From<KernelError> for CloudServerError {
    fn from(error: KernelError) -> Self {
        match error {
            KernelError::UnsupportedTarget(message) => {
                ProviderCompatRejection::unsupported_capability(message).into()
            }
            other => Self {
                status: axum::http::StatusCode::BAD_GATEWAY,
                code: "cloud_runtime_failed",
                message: other.to_string(),
            },
        }
    }
}

impl IntoResponse for CloudServerError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({
                "error": self.code,
                "message": self.message,
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_request_rejects_tools_with_provider_field_code() {
        let request: OpenAiChatRequest = serde_json::from_value(json!({
            "messages": [{"role": "user", "content": "hi"}],
            "tools": [{"type": "function", "function": {"name": "lookup"}}]
        }))
        .expect("request");

        let error = request.reject_unsupported().expect_err("tools unsupported");

        let (code, _) = error.into_parts();
        assert_eq!(code, "unsupported_provider_field");
    }

    #[test]
    fn claude_request_rejects_stream_true_with_provider_field_code() {
        let request: ClaudeMessagesRequest = serde_json::from_value(json!({
            "messages": [{"role": "user", "content": "hi"}],
            "stream": true
        }))
        .expect("request");

        let error = request
            .reject_unsupported()
            .expect_err("stream unsupported");

        let (code, _) = error.into_parts();
        assert_eq!(code, "unsupported_provider_field");
    }

    #[test]
    fn gemini_operation_rejects_unsupported_suffix() {
        let error = gemini_operation_stream("gemini-2.0-flash:countTokens")
            .expect_err("unsupported operation");

        let (code, _) = error.into_parts();
        assert_eq!(code, "unsupported_provider_operation");
    }

    #[test]
    fn embedding_request_rejects_dimensions_override() {
        let request: EmbeddingRequest = serde_json::from_value(json!({
            "input": "hello",
            "dimensions": 384
        }))
        .expect("request");

        let error = request
            .reject_unsupported()
            .expect_err("dimensions unsupported");

        let (code, _) = error.into_parts();
        assert_eq!(code, "unsupported_provider_field");
    }

    #[test]
    fn unsupported_kernel_target_maps_to_provider_capability_code() {
        let error = CloudServerError::from(KernelError::UnsupportedTarget(
            "Anthropic does not support cloud embedding through Tentgent yet".to_string(),
        ));

        assert_eq!(error.status, axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "unsupported_provider_capability");
    }
}
