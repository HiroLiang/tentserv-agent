use std::net::SocketAddr;

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tentgent_kernel::features::{
    auth::domain::Provider,
    cloud::{
        domain::{
            CloudChatContentPart, CloudChatMessage, CloudChatRequest, CloudEmbeddingRequest,
            CloudImageGenerationRequest,
        },
        infra::ReqwestCloudModelClient,
    },
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
    let stream = operation.ends_with(":streamGenerateContent");
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
                    "text" => Ok(CloudChatContentPart::Text(part.text.unwrap_or_default())),
                    "image_url" => Ok(CloudChatContentPart::ImageUrl {
                        url: part
                            .image_url
                            .map(|image| image.url)
                            .ok_or_else(|| CloudServerError::bad_request("image_url is missing"))?,
                    }),
                    other => Err(CloudServerError::bad_request(format!(
                        "unsupported OpenAI content part `{other}`"
                    ))),
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        Ok(CloudChatMessage {
            role: self.role,
            content,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ClaudeMessagesRequest {
    messages: Vec<ClaudeMessage>,
    system: Option<String>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
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
                    "text" => Ok(CloudChatContentPart::Text(block.text.unwrap_or_default())),
                    "image" => {
                        let source = block.source.ok_or_else(|| {
                            CloudServerError::bad_request("Claude image source is missing")
                        })?;
                        if source.kind != "base64" {
                            return Err(CloudServerError::bad_request(format!(
                                "unsupported Claude image source `{}`",
                                source.kind
                            )));
                        }
                        Ok(CloudChatContentPart::ImageBase64 {
                            media_type: source.media_type,
                            data: source.data,
                        })
                    }
                    other => Err(CloudServerError::bad_request(format!(
                        "unsupported Claude content block `{other}`"
                    ))),
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        Ok(CloudChatMessage {
            role: self.role,
            content,
        })
    }
}

#[derive(Debug, Deserialize)]
struct GeminiGenerateContentRequest {
    contents: Vec<GeminiContent>,
    #[serde(alias = "systemInstruction")]
    system_instruction: Option<GeminiContent>,
    #[serde(alias = "generationConfig")]
    generation_config: Option<GeminiGenerationConfig>,
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
            Err(CloudServerError::bad_request("unsupported Gemini part"))
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct EmbeddingRequest {
    input: EmbeddingInput,
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
    message: String,
}

impl CloudServerError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: axum::http::StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }
}

impl From<tentgent_kernel::foundation::error::KernelError> for CloudServerError {
    fn from(error: tentgent_kernel::foundation::error::KernelError) -> Self {
        Self {
            status: axum::http::StatusCode::BAD_GATEWAY,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for CloudServerError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({
                "error": "cloud_runtime_failed",
                "message": self.message,
            })),
        )
            .into_response()
    }
}

fn unix_timestamp_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
