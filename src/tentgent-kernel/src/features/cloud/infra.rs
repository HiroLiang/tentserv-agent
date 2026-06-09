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

fn cloud_chat_request(
    client: &Client,
    endpoints: &CloudProviderEndpoints,
    request: &CloudChatRequest,
    secret: &str,
    stream: bool,
) -> KernelResult<Request> {
    match request.provider {
        Provider::OpenAI => client
            .request(
                Method::POST,
                join_url(&endpoints.openai_base_url, "/v1/chat/completions")?,
            )
            .bearer_auth(secret)
            .json(&openai_chat_body(request, stream))
            .build()
            .map_err(build_error),
        Provider::Anthropic => client
            .request(
                Method::POST,
                join_url(&endpoints.anthropic_base_url, "/v1/messages")?,
            )
            .header("x-api-key", secret)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&anthropic_chat_body(request, stream))
            .build()
            .map_err(build_error),
        Provider::Gemini => {
            let operation = if stream {
                "streamGenerateContent"
            } else {
                "generateContent"
            };
            let mut url = gemini_model_url(
                &endpoints.gemini_base_url,
                &request.model,
                operation,
                secret,
            )?;
            if stream {
                url.query_pairs_mut().append_pair("alt", "sse");
            }
            client
                .request(Method::POST, url)
                .json(&gemini_chat_body(request))
                .build()
                .map_err(build_error)
        }
        Provider::HuggingFace => Err(unsupported_provider_error(
            request.provider,
            CloudEndpointCapability::Chat,
        )),
    }
}

fn cloud_embedding_request(
    client: &Client,
    endpoints: &CloudProviderEndpoints,
    request: &CloudEmbeddingRequest,
    secret: &str,
) -> KernelResult<Request> {
    match request.provider {
        Provider::OpenAI => client
            .request(
                Method::POST,
                join_url(&endpoints.openai_base_url, "/v1/embeddings")?,
            )
            .bearer_auth(secret)
            .json(&json!({
                "model": request.model,
                "input": request.input,
            }))
            .build()
            .map_err(build_error),
        Provider::Gemini => {
            let model = gemini_model_path(&request.model);
            let url = gemini_model_url(
                &endpoints.gemini_base_url,
                &request.model,
                "batchEmbedContents",
                secret,
            )?;
            let requests = request
                .input
                .iter()
                .map(|text| {
                    json!({
                        "model": model,
                        "content": {"parts": [{"text": text}]}
                    })
                })
                .collect::<Vec<_>>();
            client
                .request(Method::POST, url)
                .json(&json!({ "requests": requests }))
                .build()
                .map_err(build_error)
        }
        Provider::Anthropic | Provider::HuggingFace => Err(unsupported_provider_error(
            request.provider,
            CloudEndpointCapability::Embedding,
        )),
    }
}

fn cloud_image_generation_request(
    client: &Client,
    endpoints: &CloudProviderEndpoints,
    request: &CloudImageGenerationRequest,
    secret: &str,
) -> KernelResult<Request> {
    match request.provider {
        Provider::OpenAI => {
            let mut body = json!({
                "model": request.model,
                "prompt": request.prompt,
                "n": 1,
            });
            if !request.model.starts_with("gpt-image-") {
                body["response_format"] = Value::String("b64_json".to_string());
            }
            if let Some(size) = &request.size {
                body["size"] = Value::String(size.clone());
            }
            client
                .request(
                    Method::POST,
                    join_url(&endpoints.openai_base_url, "/v1/images/generations")?,
                )
                .bearer_auth(secret)
                .json(&body)
                .build()
                .map_err(build_error)
        }
        Provider::Gemini => {
            let url = gemini_model_url(
                &endpoints.gemini_base_url,
                &request.model,
                "predict",
                secret,
            )?;
            let mut parameters = json!({ "sampleCount": 1 });
            if let Some(size) = &request.size {
                parameters["sampleImageSize"] = Value::String(size.clone());
            }
            client
                .request(Method::POST, url)
                .json(&json!({
                    "instances": [{"prompt": request.prompt}],
                    "parameters": parameters,
                }))
                .build()
                .map_err(build_error)
        }
        Provider::Anthropic | Provider::HuggingFace => Err(unsupported_provider_error(
            request.provider,
            CloudEndpointCapability::ImageGeneration,
        )),
    }
}

fn openai_chat_body(request: &CloudChatRequest, stream: bool) -> Value {
    let messages = request
        .messages
        .iter()
        .map(openai_chat_message)
        .collect::<Vec<_>>();
    let mut body = json!({
        "model": request.model,
        "messages": messages,
        "stream": stream,
    });
    if let Some(max_tokens) = request.max_tokens {
        body["max_tokens"] = json!(max_tokens);
    }
    if let Some(temperature) = request.temperature {
        body["temperature"] = json!(temperature);
    }
    if let Some(modalities) = request.response_modalities.as_ref() {
        body["modalities"] = json!(modalities);
    }
    if let Some(audio) = request.audio.as_ref() {
        body["audio"] = audio.clone();
    }
    body
}

fn openai_chat_message(message: &CloudChatMessage) -> Value {
    if message.content.len() == 1 {
        if let CloudChatContentPart::Text(text) = &message.content[0] {
            return json!({
                "role": openai_role(&message.role),
                "content": text,
            });
        }
    }
    let content = message
        .content
        .iter()
        .map(|part| match part {
            CloudChatContentPart::Text(text) => json!({"type": "text", "text": text}),
            CloudChatContentPart::ImageUrl { url } => {
                json!({"type": "image_url", "image_url": {"url": url}})
            }
            CloudChatContentPart::ImageBase64 { media_type, data } => {
                json!({"type": "image_url", "image_url": {"url": format!("data:{media_type};base64,{data}")}})
            }
            CloudChatContentPart::InputAudio { data, format } => {
                json!({"type": "input_audio", "input_audio": {"data": data, "format": format}})
            }
        })
        .collect::<Vec<_>>();
    json!({
        "role": openai_role(&message.role),
        "content": content,
    })
}

fn anthropic_chat_body(request: &CloudChatRequest, stream: bool) -> Value {
    let mut system = Vec::new();
    let mut messages = Vec::new();
    for message in &request.messages {
        if message.role.eq_ignore_ascii_case("system") {
            system.push(message.text_content());
            continue;
        }
        messages.push(json!({
            "role": anthropic_role(&message.role),
            "content": anthropic_content(&message.content),
        }));
    }
    let mut body = json!({
        "model": request.model,
        "messages": messages,
        "max_tokens": request.max_tokens.unwrap_or(DEFAULT_ANTHROPIC_MAX_TOKENS),
        "stream": stream,
    });
    if !system.is_empty() {
        body["system"] = Value::String(system.join("\n\n"));
    }
    if let Some(temperature) = request.temperature {
        body["temperature"] = json!(temperature);
    }
    body
}

fn anthropic_content(parts: &[CloudChatContentPart]) -> Value {
    if parts.len() == 1 {
        if let CloudChatContentPart::Text(text) = &parts[0] {
            return Value::String(text.clone());
        }
    }
    Value::Array(
        parts
            .iter()
            .map(|part| match part {
                CloudChatContentPart::Text(text) => json!({"type": "text", "text": text}),
                CloudChatContentPart::ImageBase64 { media_type, data } => json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": media_type,
                        "data": data
                    }
                }),
                CloudChatContentPart::ImageUrl { url } => json!({
                    "type": "text",
                    "text": format!("[image_url: {url}]")
                }),
                CloudChatContentPart::InputAudio { format, .. } => json!({
                    "type": "text",
                    "text": format!("[input_audio: {format}]")
                }),
            })
            .collect(),
    )
}

fn gemini_chat_body(request: &CloudChatRequest) -> Value {
    let mut contents = Vec::new();
    let mut system_parts = Vec::new();
    for message in &request.messages {
        let parts = gemini_parts(&message.content);
        if message.role.eq_ignore_ascii_case("system") {
            system_parts.extend(parts);
            continue;
        }
        contents.push(json!({
            "role": gemini_role(&message.role),
            "parts": parts,
        }));
    }
    let mut body = json!({ "contents": contents });
    if !system_parts.is_empty() {
        body["systemInstruction"] = json!({ "parts": system_parts });
    }
    let mut generation_config = serde_json::Map::new();
    if let Some(max_tokens) = request.max_tokens {
        generation_config.insert("maxOutputTokens".to_string(), json!(max_tokens));
    }
    if let Some(temperature) = request.temperature {
        generation_config.insert("temperature".to_string(), json!(temperature));
    }
    if !generation_config.is_empty() {
        body["generationConfig"] = Value::Object(generation_config);
    }
    body
}

fn gemini_parts(parts: &[CloudChatContentPart]) -> Vec<Value> {
    parts
        .iter()
        .map(|part| match part {
            CloudChatContentPart::Text(text) => json!({"text": text}),
            CloudChatContentPart::ImageBase64 { media_type, data } => {
                json!({"inlineData": {"mimeType": media_type, "data": data}})
            }
            CloudChatContentPart::ImageUrl { url } => {
                json!({"text": format!("[image_url: {url}]")})
            }
            CloudChatContentPart::InputAudio { format, .. } => {
                json!({"text": format!("[input_audio: {format}]")})
            }
        })
        .collect()
}

fn decode_chat_response(provider: Provider, value: Value) -> KernelResult<CloudChatResponse> {
    let (text, finish_reason, audio) = match provider {
        Provider::OpenAI => {
            let choice = value
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|choices| choices.first())
                .ok_or_else(|| cloud_error("OpenAI chat response did not contain choices"))?;
            let text = choice
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let finish = choice
                .get("finish_reason")
                .and_then(Value::as_str)
                .unwrap_or("stop")
                .to_string();
            let audio = choice
                .get("message")
                .and_then(|message| message.get("audio"))
                .cloned();
            (text, finish, audio)
        }
        Provider::Anthropic => {
            let text = value
                .get("content")
                .and_then(Value::as_array)
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(|part| part.get("text").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default();
            let finish = value
                .get("stop_reason")
                .and_then(Value::as_str)
                .unwrap_or("end_turn")
                .to_string();
            (text, finish, None)
        }
        Provider::Gemini => {
            let (text, finish) = decode_gemini_text_response(&value);
            (text, finish, None)
        }
        Provider::HuggingFace => {
            return Err(unsupported_provider_error(
                provider,
                CloudEndpointCapability::Chat,
            ))
        }
    };
    Ok(CloudChatResponse {
        text,
        finish_reason,
        audio,
    })
}

fn decode_embedding_response(
    provider: Provider,
    value: Value,
) -> KernelResult<CloudEmbeddingResponse> {
    let vectors = match provider {
        Provider::OpenAI => value
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| cloud_error("OpenAI embedding response did not contain data"))?
            .iter()
            .map(|item| json_array_to_f32_vec(item.get("embedding")))
            .collect::<KernelResult<Vec<_>>>()?,
        Provider::Gemini => value
            .get("embeddings")
            .and_then(Value::as_array)
            .ok_or_else(|| cloud_error("Gemini embedding response did not contain embeddings"))?
            .iter()
            .map(|item| json_array_to_f32_vec(item.get("values")))
            .collect::<KernelResult<Vec<_>>>()?,
        Provider::Anthropic | Provider::HuggingFace => {
            return Err(unsupported_provider_error(
                provider,
                CloudEndpointCapability::Embedding,
            ))
        }
    };
    Ok(CloudEmbeddingResponse { vectors })
}

fn decode_image_generation_response(
    provider: Provider,
    value: Value,
) -> KernelResult<CloudImageGenerationResponse> {
    let b64_json = match provider {
        Provider::OpenAI => value
            .get("data")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("b64_json").or_else(|| item.get("b64Json")))
            .and_then(Value::as_str)
            .ok_or_else(|| cloud_error("OpenAI image response did not contain b64_json"))?
            .to_string(),
        Provider::Gemini => gemini_image_base64(&value)?,
        Provider::Anthropic | Provider::HuggingFace => {
            return Err(unsupported_provider_error(
                provider,
                CloudEndpointCapability::ImageGeneration,
            ))
        }
    };
    Ok(CloudImageGenerationResponse {
        b64_json,
        media_type: "image/png".to_string(),
    })
}

fn handle_stream_event(
    provider: Provider,
    event: &str,
    data: &str,
    sink: &mut dyn FnMut(CloudStreamEvent),
    collected: &mut String,
    finish_reason: &mut String,
) -> KernelResult<()> {
    if data.trim() == "[DONE]" || data.trim().is_empty() {
        return Ok(());
    }
    match provider {
        Provider::OpenAI => {
            let chunk: OpenAiStreamChunk = serde_json::from_str(data).map_err(|err| {
                cloud_error(format!("failed to decode OpenAI stream chunk: {err}"))
            })?;
            for choice in chunk.choices {
                if let Some(delta) = choice.delta.and_then(|delta| delta.content) {
                    collected.push_str(&delta);
                    sink(CloudStreamEvent::Delta { text: delta });
                }
                if let Some(reason) = choice.finish_reason {
                    *finish_reason = reason;
                }
            }
            Ok(())
        }
        Provider::Anthropic => {
            if event == "content_block_delta" {
                let chunk: AnthropicDeltaEvent = serde_json::from_str(data).map_err(|err| {
                    cloud_error(format!("failed to decode Anthropic stream delta: {err}"))
                })?;
                if let Some(text) = chunk.delta.text {
                    collected.push_str(&text);
                    sink(CloudStreamEvent::Delta { text });
                }
            } else if event == "message_delta" {
                let chunk: AnthropicMessageDeltaEvent =
                    serde_json::from_str(data).map_err(|err| {
                        cloud_error(format!(
                            "failed to decode Anthropic stream stop reason: {err}"
                        ))
                    })?;
                if let Some(reason) = chunk.delta.stop_reason {
                    *finish_reason = reason;
                }
            } else if event == "error" {
                let message = data.to_string();
                sink(CloudStreamEvent::Error {
                    code: "cloud_provider_error".to_string(),
                    message: message.clone(),
                });
                return Err(cloud_error(message));
            }
            Ok(())
        }
        Provider::Gemini => {
            let value: Value = serde_json::from_str(data).map_err(|err| {
                cloud_error(format!("failed to decode Gemini stream chunk: {err}"))
            })?;
            let (text, reason) = decode_gemini_text_response(&value);
            if !text.is_empty() {
                collected.push_str(&text);
                sink(CloudStreamEvent::Delta { text });
            }
            if !reason.is_empty() {
                *finish_reason = reason;
            }
            Ok(())
        }
        Provider::HuggingFace => Err(unsupported_provider_error(
            provider,
            CloudEndpointCapability::Chat,
        )),
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChunk {
    choices: Vec<OpenAiStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChoice {
    delta: Option<OpenAiDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiDelta {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicDeltaEvent {
    delta: AnthropicTextDelta,
}

#[derive(Debug, Deserialize)]
struct AnthropicTextDelta {
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessageDeltaEvent {
    delta: AnthropicStopDelta,
}

#[derive(Debug, Deserialize)]
struct AnthropicStopDelta {
    stop_reason: Option<String>,
}

fn decode_gemini_text_response(value: &Value) -> (String, String) {
    let Some(candidate) = value
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|candidates| candidates.first())
    else {
        return (String::new(), String::new());
    };
    let text = candidate
        .get("content")
        .and_then(|content| content.get("parts"))
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();
    let finish = candidate
        .get("finishReason")
        .and_then(Value::as_str)
        .unwrap_or("STOP")
        .to_string();
    (text, finish)
}

fn gemini_image_base64(value: &Value) -> KernelResult<String> {
    let candidates = [
        value.pointer("/predictions/0/bytesBase64Encoded"),
        value.pointer("/predictions/0/image/bytesBase64Encoded"),
        value.pointer("/predictions/0/image/imageBytes"),
        value.pointer("/generatedImages/0/image/imageBytes"),
        value.pointer("/generatedImages/0/image/bytesBase64Encoded"),
    ];
    candidates
        .into_iter()
        .flatten()
        .find_map(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| cloud_error("Gemini image response did not contain image bytes"))
}

fn json_array_to_f32_vec(value: Option<&Value>) -> KernelResult<Vec<f32>> {
    value
        .and_then(Value::as_array)
        .ok_or_else(|| cloud_error("embedding vector was not an array"))?
        .iter()
        .map(|value| {
            value
                .as_f64()
                .map(|number| number as f32)
                .ok_or_else(|| cloud_error("embedding vector contained a non-number value"))
        })
        .collect()
}

fn next_sse_event(buffer: &str) -> Option<(String, String, usize)> {
    let index = buffer.find("\n\n")?;
    let block = &buffer[..index];
    let mut event = None;
    let mut data = Vec::new();
    for line in block.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.trim().to_string());
        }
    }
    Some((event.unwrap_or_default(), data.join("\n"), index + 2))
}

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
    let model_path = gemini_model_path(model);
    let mut url = join_url(base, &format!("/v1beta/{model_path}:{operation}"))?;
    url.query_pairs_mut().append_pair("key", secret);
    Ok(url)
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
mod tests {
    use super::*;

    fn client() -> ReqwestCloudModelClient {
        ReqwestCloudModelClient::with_client_and_endpoints(
            Client::new(),
            CloudProviderEndpoints {
                openai_base_url: Url::parse("https://openai.test").unwrap(),
                anthropic_base_url: Url::parse("https://anthropic.test").unwrap(),
                gemini_base_url: Url::parse("https://gemini.test").unwrap(),
            },
        )
    }

    #[test]
    fn openai_chat_template_keeps_image_url_parts() {
        let request = CloudChatRequest {
            provider: Provider::OpenAI,
            model: "gpt-test".to_string(),
            messages: vec![CloudChatMessage {
                role: "user".to_string(),
                content: vec![
                    CloudChatContentPart::Text("what is here?".to_string()),
                    CloudChatContentPart::ImageUrl {
                        url: "data:image/png;base64,AA==".to_string(),
                    },
                ],
            }],
            max_tokens: Some(12),
            temperature: Some(0.0),
            stream: false,
            response_modalities: None,
            audio: None,
        };
        let http = client().chat_request(&request, "sk-test", false).unwrap();
        let body = http.body().and_then(|body| body.as_bytes()).unwrap();
        let value: Value = serde_json::from_slice(body).unwrap();

        assert_eq!(value["model"], "gpt-test");
        assert_eq!(value["messages"][0]["content"][0]["type"], "text");
        assert_eq!(value["messages"][0]["content"][1]["type"], "image_url");
    }

    #[test]
    fn openai_chat_template_keeps_audio_input_and_output_options() {
        let request = CloudChatRequest {
            provider: Provider::OpenAI,
            model: "gpt-audio".to_string(),
            messages: vec![CloudChatMessage {
                role: "user".to_string(),
                content: vec![
                    CloudChatContentPart::Text("what is in this recording?".to_string()),
                    CloudChatContentPart::InputAudio {
                        data: "AA==".to_string(),
                        format: "wav".to_string(),
                    },
                ],
            }],
            max_tokens: Some(12),
            temperature: Some(0.0),
            stream: false,
            response_modalities: Some(vec!["text".to_string(), "audio".to_string()]),
            audio: Some(json!({"voice": "alloy", "format": "wav"})),
        };
        let http = client().chat_request(&request, "sk-test", false).unwrap();
        let body = http.body().and_then(|body| body.as_bytes()).unwrap();
        let value: Value = serde_json::from_slice(body).unwrap();

        assert_eq!(value["model"], "gpt-audio");
        assert_eq!(value["modalities"], json!(["text", "audio"]));
        assert_eq!(value["audio"], json!({"voice": "alloy", "format": "wav"}));
        assert_eq!(value["messages"][0]["content"][0]["type"], "text");
        assert_eq!(value["messages"][0]["content"][1]["type"], "input_audio");
        assert_eq!(
            value["messages"][0]["content"][1]["input_audio"],
            json!({"data": "AA==", "format": "wav"})
        );
    }

    #[test]
    fn openai_chat_response_preserves_audio_output() {
        let response = decode_chat_response(
            Provider::OpenAI,
            json!({
                "choices": [{
                    "message": {
                        "content": "hello",
                        "audio": {
                            "id": "audio_123",
                            "data": "AA==",
                            "transcript": "hello"
                        }
                    },
                    "finish_reason": "stop"
                }]
            }),
        )
        .expect("response");

        assert_eq!(response.text, "hello");
        assert_eq!(response.finish_reason, "stop");
        assert_eq!(
            response.audio,
            Some(json!({
                "id": "audio_123",
                "data": "AA==",
                "transcript": "hello"
            }))
        );
    }

    #[test]
    fn anthropic_chat_template_uses_messages_url_and_bound_model() {
        let request = CloudChatRequest {
            provider: Provider::Anthropic,
            model: "claude-test".to_string(),
            messages: vec![
                CloudChatMessage {
                    role: "system".to_string(),
                    content: vec![CloudChatContentPart::Text("Answer briefly.".to_string())],
                },
                CloudChatMessage {
                    role: "user".to_string(),
                    content: vec![
                        CloudChatContentPart::ImageBase64 {
                            media_type: "image/png".to_string(),
                            data: "AA==".to_string(),
                        },
                        CloudChatContentPart::Text("Describe this image.".to_string()),
                    ],
                },
            ],
            max_tokens: Some(12),
            temperature: Some(0.2),
            stream: false,
            response_modalities: None,
            audio: None,
        };
        let http = client().chat_request(&request, "sk-ant", false).unwrap();
        let body = http.body().and_then(|body| body.as_bytes()).unwrap();
        let value: Value = serde_json::from_slice(body).unwrap();

        assert_eq!(http.url().as_str(), "https://anthropic.test/v1/messages");
        assert_eq!(value["model"], "claude-test");
        assert_eq!(value["system"], "Answer briefly.");
        assert_eq!(value["max_tokens"], 12);
        let temperature = value["temperature"].as_f64().expect("temperature");
        assert!((temperature - 0.2).abs() < 0.00001);
        assert_eq!(value["stream"], false);
        assert_eq!(value["messages"][0]["role"], "user");
        assert_eq!(value["messages"][0]["content"][0]["type"], "image");
        assert_eq!(
            value["messages"][0]["content"][0]["source"]["media_type"],
            "image/png"
        );
        assert_eq!(value["messages"][0]["content"][1]["type"], "text");
    }

    #[test]
    fn gemini_chat_template_uses_generate_content_url_and_inline_data() {
        let request = CloudChatRequest {
            provider: Provider::Gemini,
            model: "gemini-test".to_string(),
            messages: vec![CloudChatMessage {
                role: "user".to_string(),
                content: vec![CloudChatContentPart::ImageBase64 {
                    media_type: "image/png".to_string(),
                    data: "AA==".to_string(),
                }],
            }],
            max_tokens: Some(12),
            temperature: None,
            stream: false,
            response_modalities: None,
            audio: None,
        };
        let http = client()
            .chat_request(&request, "gemini-key", false)
            .unwrap();
        assert_eq!(
            http.url().as_str(),
            "https://gemini.test/v1beta/models/gemini-test:generateContent?key=gemini-key"
        );
        let body = http.body().and_then(|body| body.as_bytes()).unwrap();
        let value: Value = serde_json::from_slice(body).unwrap();

        assert_eq!(
            value["contents"][0]["parts"][0]["inlineData"]["mimeType"],
            "image/png"
        );
        assert_eq!(value["generationConfig"]["maxOutputTokens"], 12);
    }

    #[test]
    fn openai_gpt_image_template_omits_response_format() {
        let http = client()
            .image_generation_request(
                &CloudImageGenerationRequest {
                    provider: Provider::OpenAI,
                    model: "gpt-image-1".to_string(),
                    prompt: "red square".to_string(),
                    size: Some("1024x1024".to_string()),
                },
                "sk-test",
            )
            .unwrap();
        let body = http.body().and_then(|body| body.as_bytes()).unwrap();
        let value: Value = serde_json::from_slice(body).unwrap();

        assert_eq!(value["model"], "gpt-image-1");
        assert_eq!(value["size"], "1024x1024");
        assert!(value.get("response_format").is_none());
    }

    #[test]
    fn openai_legacy_image_template_requests_b64_json() {
        let http = client()
            .image_generation_request(
                &CloudImageGenerationRequest {
                    provider: Provider::OpenAI,
                    model: "dall-e-3".to_string(),
                    prompt: "red square".to_string(),
                    size: None,
                },
                "sk-test",
            )
            .unwrap();
        let body = http.body().and_then(|body| body.as_bytes()).unwrap();
        let value: Value = serde_json::from_slice(body).unwrap();

        assert_eq!(value["model"], "dall-e-3");
        assert_eq!(value["response_format"], "b64_json");
    }

    #[test]
    fn anthropic_embedding_is_unsupported() {
        let err = cloud_embedding_request(
            &Client::new(),
            &CloudProviderEndpoints::default(),
            &CloudEmbeddingRequest {
                provider: Provider::Anthropic,
                model: "claude".to_string(),
                input: vec!["hello".to_string()],
            },
            "sk-ant",
        )
        .expect_err("unsupported");

        assert!(matches!(err, KernelError::UnsupportedTarget(_)));
    }
}
