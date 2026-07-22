//! Reqwest-backed cloud provider client.

use std::time::Duration;

use futures_util::StreamExt;
use reqwest::{Client, Method, Request, Response, StatusCode, Url};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::features::{
    auth::domain::Provider,
    cloud::domain::{
        provider_supports, CloudChatContentPart, CloudChatMessage, CloudChatRequest,
        CloudChatResponse, CloudEmbeddingRequest, CloudEmbeddingResponse, CloudEndpointCapability,
        CloudImageGenerationRequest, CloudImageGenerationResponse, CloudStreamEvent,
    },
};
use crate::foundation::error::{KernelError, KernelResult};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_ANTHROPIC_MAX_TOKENS: u32 = 1024;
const OPENAI_BASE_URL: &str = "https://api.openai.com";
const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Debug, Clone)]
pub struct CloudProviderEndpoints {
    pub openai_base_url: Url,
    pub anthropic_base_url: Url,
    pub gemini_base_url: Url,
}

impl CloudProviderEndpoints {
    pub fn new() -> KernelResult<Self> {
        Ok(Self {
            openai_base_url: parse_url("OpenAI", OPENAI_BASE_URL)?,
            anthropic_base_url: parse_url("Anthropic", ANTHROPIC_BASE_URL)?,
            gemini_base_url: parse_url("Gemini", GEMINI_BASE_URL)?,
        })
    }
}

impl Default for CloudProviderEndpoints {
    fn default() -> Self {
        Self::new().expect("static cloud provider URLs should parse")
    }
}

#[derive(Debug, Clone)]
pub struct ReqwestCloudModelClient {
    client: Client,
    endpoints: CloudProviderEndpoints,
}

impl ReqwestCloudModelClient {
    pub fn new() -> KernelResult<Self> {
        let client = Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .map_err(|err| {
                KernelError::RuntimeStateUnavailable(format!(
                    "failed to build cloud provider HTTP client: {err}"
                ))
            })?;
        Ok(Self {
            client,
            endpoints: CloudProviderEndpoints::new()?,
        })
    }

    pub fn with_client_and_endpoints(client: Client, endpoints: CloudProviderEndpoints) -> Self {
        Self { client, endpoints }
    }

    pub async fn complete_chat(
        &self,
        request: CloudChatRequest,
        secret: &str,
    ) -> KernelResult<CloudChatResponse> {
        ensure_supported(request.provider, CloudEndpointCapability::Chat)?;
        let http_request = self.chat_request(&request, secret, false)?;
        let response = self.execute(http_request, request.provider).await?;
        let value: Value = response.json().await.map_err(|err| {
            cloud_error(format!(
                "failed to decode {} chat response: {err}",
                request.provider.display_name()
            ))
        })?;
        decode_chat_response(request.provider, value)
    }

    pub async fn stream_chat(
        &self,
        mut request: CloudChatRequest,
        secret: &str,
        sink: &mut dyn FnMut(CloudStreamEvent),
    ) -> KernelResult<CloudChatResponse> {
        ensure_supported(request.provider, CloudEndpointCapability::Chat)?;
        request.stream = true;
        let http_request = self.chat_request(&request, secret, true)?;
        let response = self.execute(http_request, request.provider).await?;
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut collected = String::new();
        let mut finish_reason = "stop".to_string();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|err| {
                cloud_error(format!(
                    "failed to read {} chat stream: {err}",
                    request.provider.display_name()
                ))
            })?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some((event, data, consumed)) = next_sse_event(&buffer) {
                buffer.drain(..consumed);
                handle_stream_event(
                    request.provider,
                    &event,
                    &data,
                    sink,
                    &mut collected,
                    &mut finish_reason,
                )?;
            }
        }
        if !buffer.trim().is_empty() {
            if let Some((event, data, _)) = next_sse_event(&(buffer + "\n\n")) {
                handle_stream_event(
                    request.provider,
                    &event,
                    &data,
                    sink,
                    &mut collected,
                    &mut finish_reason,
                )?;
            }
        }
        sink(CloudStreamEvent::Done {
            finish_reason: finish_reason.clone(),
        });

        Ok(CloudChatResponse {
            text: collected,
            finish_reason,
            audio: None,
        })
    }

    pub async fn create_embedding(
        &self,
        request: CloudEmbeddingRequest,
        secret: &str,
    ) -> KernelResult<CloudEmbeddingResponse> {
        ensure_supported(request.provider, CloudEndpointCapability::Embedding)?;
        let http_request = self.embedding_request(&request, secret)?;
        let response = self.execute(http_request, request.provider).await?;
        let value: Value = response.json().await.map_err(|err| {
            cloud_error(format!(
                "failed to decode {} embedding response: {err}",
                request.provider.display_name()
            ))
        })?;
        decode_embedding_response(request.provider, value)
    }

    pub async fn generate_image(
        &self,
        request: CloudImageGenerationRequest,
        secret: &str,
    ) -> KernelResult<CloudImageGenerationResponse> {
        ensure_supported(request.provider, CloudEndpointCapability::ImageGeneration)?;
        let http_request = self.image_generation_request(&request, secret)?;
        let response = self.execute(http_request, request.provider).await?;
        let value: Value = response.json().await.map_err(|err| {
            cloud_error(format!(
                "failed to decode {} image generation response: {err}",
                request.provider.display_name()
            ))
        })?;
        decode_image_generation_response(request.provider, value)
    }

    #[cfg(test)]
    pub(crate) fn chat_request(
        &self,
        request: &CloudChatRequest,
        secret: &str,
        stream: bool,
    ) -> KernelResult<Request> {
        cloud_chat_request(&self.client, &self.endpoints, request, secret, stream)
    }

    #[cfg(not(test))]
    fn chat_request(
        &self,
        request: &CloudChatRequest,
        secret: &str,
        stream: bool,
    ) -> KernelResult<Request> {
        cloud_chat_request(&self.client, &self.endpoints, request, secret, stream)
    }

    fn embedding_request(
        &self,
        request: &CloudEmbeddingRequest,
        secret: &str,
    ) -> KernelResult<Request> {
        cloud_embedding_request(&self.client, &self.endpoints, request, secret)
    }

    fn image_generation_request(
        &self,
        request: &CloudImageGenerationRequest,
        secret: &str,
    ) -> KernelResult<Request> {
        cloud_image_generation_request(&self.client, &self.endpoints, request, secret)
    }

    async fn execute(&self, request: Request, provider: Provider) -> KernelResult<Response> {
        let response = self.client.execute(request).await.map_err(|err| {
            cloud_error(format!(
                "{} request failed before a response was received: {err}",
                provider.display_name()
            ))
        })?;
        if response.status().is_success() {
            return Ok(response);
        }
        Err(provider_http_error(provider, response).await)
    }
}

mod protocol;
use protocol::*;

fn ensure_supported(provider: Provider, capability: CloudEndpointCapability) -> KernelResult<()> {
    if provider_supports(provider, capability) {
        Ok(())
    } else {
        Err(unsupported_provider_error(provider, capability))
    }
}

fn unsupported_provider_error(
    provider: Provider,
    capability: CloudEndpointCapability,
) -> KernelError {
    KernelError::UnsupportedTarget(format!(
        "{} does not support cloud {} through Tentgent yet",
        provider.display_name(),
        capability.as_str()
    ))
}

fn openai_role(role: &str) -> &str {
    if role.eq_ignore_ascii_case("assistant") {
        "assistant"
    } else if role.eq_ignore_ascii_case("system") || role.eq_ignore_ascii_case("developer") {
        "system"
    } else {
        "user"
    }
}

fn anthropic_role(role: &str) -> &str {
    if role.eq_ignore_ascii_case("assistant") {
        "assistant"
    } else {
        "user"
    }
}

fn gemini_role(role: &str) -> &str {
    if role.eq_ignore_ascii_case("assistant") || role.eq_ignore_ascii_case("model") {
        "model"
    } else {
        "user"
    }
}

fn gemini_model_path(model: &str) -> String {
    let model = model.trim().trim_start_matches('/');
    if model.starts_with("models/") {
        model.to_string()
    } else {
        format!("models/{model}")
    }
}

fn gemini_model_url(base: &Url, model: &str, operation: &str, secret: &str) -> KernelResult<Url> {
    gemini_model_url_version(base, "v1beta", model, operation, secret)
}

fn gemini_model_url_version(
    base: &Url,
    version: &str,
    model: &str,
    operation: &str,
    secret: &str,
) -> KernelResult<Url> {
    let model_path = gemini_model_path(model);
    let mut url = join_url(base, &format!("/{version}/{model_path}:{operation}"))?;
    url.query_pairs_mut().append_pair("key", secret);
    Ok(url)
}

fn gemini_image_model_uses_generate_content(model: &str) -> bool {
    let model = model.trim().trim_start_matches("models/");
    model.starts_with("gemini-") && model.contains("-image")
}

fn gemini_imagen_model_uses_predict(model: &str) -> bool {
    let model = model.trim().trim_start_matches("models/");
    model.starts_with("imagen-")
}

fn join_url(base: &Url, path: &str) -> KernelResult<Url> {
    base.join(path)
        .map_err(|err| cloud_error(format!("failed to build cloud provider URL: {err}")))
}

fn parse_url(label: &str, value: &str) -> KernelResult<Url> {
    Url::parse(value).map_err(|err| {
        KernelError::RuntimeStateUnavailable(format!("failed to parse {label} base URL: {err}"))
    })
}

fn build_error(err: reqwest::Error) -> KernelError {
    cloud_error(format!("failed to build cloud provider request: {err}"))
}

async fn provider_http_error(provider: Provider, response: Response) -> KernelError {
    let status = response.status();
    let detail = response
        .text()
        .await
        .unwrap_or_else(|err| format!("failed to read error body: {err}"));
    let category = match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => "auth failed",
        StatusCode::TOO_MANY_REQUESTS => "rate limited",
        _ => "request failed",
    };
    cloud_error(format!(
        "{} {} with HTTP {}: {}",
        provider.display_name(),
        category,
        status.as_u16(),
        truncate_detail(&detail)
    ))
}

fn truncate_detail(detail: &str) -> String {
    const MAX: usize = 800;
    let detail = detail.trim();
    if detail.len() <= MAX {
        detail.to_string()
    } else {
        format!("{}...", &detail[..MAX])
    }
}

fn cloud_error(message: impl Into<String>) -> KernelError {
    KernelError::RuntimeStateUnavailable(message.into())
}

#[cfg(test)]
mod tests;
