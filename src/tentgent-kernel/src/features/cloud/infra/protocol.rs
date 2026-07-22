use super::*;

pub(super) fn cloud_chat_request(
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

pub(super) fn cloud_embedding_request(
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

pub(super) fn cloud_image_generation_request(
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
            if gemini_image_model_uses_generate_content(&request.model) {
                let url = gemini_model_url_version(
                    &endpoints.gemini_base_url,
                    "v1",
                    &request.model,
                    "generateContent",
                    secret,
                )?;
                return client
                    .request(Method::POST, url)
                    .json(&gemini_image_generation_body(request))
                    .build()
                    .map_err(build_error);
            }
            if !gemini_imagen_model_uses_predict(&request.model) {
                return Err(KernelError::UnsupportedTarget(format!(
                    "Gemini image generation requires a Gemini image model or Imagen model; `{}` is not recognized as image-generation capable",
                    request.model
                )));
            }

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

pub(super) fn openai_chat_body(request: &CloudChatRequest, stream: bool) -> Value {
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

pub(super) fn openai_chat_message(message: &CloudChatMessage) -> Value {
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
            CloudChatContentPart::AudioBase64 { media_type, .. } => {
                json!({"type": "text", "text": format!("[audio: {media_type}]")})
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

pub(super) fn anthropic_chat_body(request: &CloudChatRequest, stream: bool) -> Value {
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

pub(super) fn anthropic_content(parts: &[CloudChatContentPart]) -> Value {
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
                CloudChatContentPart::AudioBase64 { media_type, .. } => json!({
                    "type": "text",
                    "text": format!("[audio: {media_type}]")
                }),
                CloudChatContentPart::InputAudio { format, .. } => json!({
                    "type": "text",
                    "text": format!("[input_audio: {format}]")
                }),
            })
            .collect(),
    )
}

pub(super) fn gemini_chat_body(request: &CloudChatRequest) -> Value {
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

pub(super) fn gemini_image_generation_body(request: &CloudImageGenerationRequest) -> Value {
    json!({
        "contents": [{
            "parts": [{"text": request.prompt}]
        }]
    })
}

pub(super) fn gemini_parts(parts: &[CloudChatContentPart]) -> Vec<Value> {
    parts
        .iter()
        .map(|part| match part {
            CloudChatContentPart::Text(text) => json!({"text": text}),
            CloudChatContentPart::ImageBase64 { media_type, data } => {
                json!({"inlineData": {"mimeType": media_type, "data": data}})
            }
            CloudChatContentPart::AudioBase64 { media_type, data } => {
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

pub(super) fn decode_chat_response(
    provider: Provider,
    value: Value,
) -> KernelResult<CloudChatResponse> {
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

pub(super) fn decode_embedding_response(
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

pub(super) fn decode_image_generation_response(
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

pub(super) fn handle_stream_event(
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

pub(super) fn decode_gemini_text_response(value: &Value) -> (String, String) {
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

pub(super) fn gemini_image_base64(value: &Value) -> KernelResult<String> {
    let candidates = [
        gemini_generate_content_image_base64(value),
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

pub(super) fn gemini_generate_content_image_base64(value: &Value) -> Option<&Value> {
    value
        .get("candidates")
        .and_then(Value::as_array)?
        .iter()
        .flat_map(|candidate| {
            candidate
                .get("content")
                .and_then(|content| content.get("parts"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .find_map(|part| {
            part.get("inlineData")
                .or_else(|| part.get("inline_data"))
                .and_then(|inline_data| inline_data.get("data"))
        })
}

pub(super) fn json_array_to_f32_vec(value: Option<&Value>) -> KernelResult<Vec<f32>> {
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

pub(super) fn next_sse_event(buffer: &str) -> Option<(String, String, usize)> {
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
