use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tentgent_kernel::features::{auth::domain::Provider, chat::domain::ChatFinishReason};

use crate::{
    provider_compat::ProviderCompatRejection,
    transport::rest::{error::RestError, state::RestState},
};

use super::execution::{
    chat_preparation_request, complete_chat, sse_json_event, stream_chat_response,
    ChatStreamMapper, ChatTransportMessage, ChatTransportRequest,
};

pub(crate) async fn generate_content(
    State(state): State<RestState>,
    Path(operation): Path<String>,
    Json(request): Json<GeminiGenerateContentRequest>,
) -> Result<Response, RestError> {
    let operation = GeminiOperation::parse(operation)?;
    let stream = operation.stream;
    let model = operation.model;
    let context = GeminiContext {
        model: model.clone(),
    };
    let request = request.into_transport(model)?;
    let request = chat_preparation_request(&state, request, stream)?;
    if stream {
        return Ok(stream_chat_response(
            state,
            request,
            GeminiStreamMapper::new(context),
        ));
    }

    let result = complete_chat(state, request).await?;
    Ok(Json(gemini_response(
        context,
        Some(result.response.text),
        Some(result.response.finish_reason),
    ))
    .into_response())
}

#[derive(Debug)]
struct GeminiOperation {
    model: String,
    stream: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GeminiGenerateContentRequest {
    contents: Vec<GeminiContent>,
    adapter_ref: Option<String>,
    #[serde(alias = "generationConfig")]
    generation_config: Option<GeminiGenerationConfig>,
    #[serde(alias = "systemInstruction")]
    system_instruction: Option<GeminiContent>,
    tools: Option<serde_json::Value>,
    #[serde(alias = "toolConfig")]
    tool_config: Option<serde_json::Value>,
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
}

#[derive(Debug, Clone)]
struct GeminiContext {
    model: String,
}

struct GeminiStreamMapper {
    context: GeminiContext,
}

impl GeminiOperation {
    fn parse(operation: String) -> Result<Self, RestError> {
        if let Some(model) = operation.strip_suffix(":generateContent") {
            return Self::new(model, false);
        }
        if let Some(model) = operation.strip_suffix(":streamGenerateContent") {
            return Self::new(model, true);
        }
        Err(ProviderCompatRejection::unsupported_operation(
            "unsupported Gemini generateContent operation",
        )
        .into())
    }

    fn new(model: &str, stream: bool) -> Result<Self, RestError> {
        let model = model.trim();
        if model.is_empty() {
            return Err(RestError::bad_request("bad_request", "model is empty"));
        }
        Ok(Self {
            model: model.to_string(),
            stream,
        })
    }
}

impl GeminiGenerateContentRequest {
    fn into_transport(self, model_ref: String) -> Result<ChatTransportRequest, RestError> {
        self.reject_unsupported()?;
        let mut messages = Vec::new();
        if let Some(system) = self.system_instruction {
            messages.push(ChatTransportMessage {
                role: "system".to_string(),
                content: gemini_content_text(system)?,
            });
        }
        for content in self.contents {
            messages.push(content.into_transport()?);
        }
        let generation_config = self.generation_config.unwrap_or_default();

        Ok(ChatTransportRequest {
            model_ref,
            adapter_ref: self.adapter_ref,
            cloud_provider: Some(Provider::Gemini),
            messages,
            max_tokens: generation_config.max_output_tokens,
            temperature: generation_config.temperature,
        })
    }

    fn reject_unsupported(&self) -> Result<(), RestError> {
        if self.tools.is_some() || self.tool_config.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "Gemini-compatible tools require kernel tool-call support",
            )
            .into());
        }
        Ok(())
    }
}

impl Default for GeminiGenerationConfig {
    fn default() -> Self {
        Self {
            max_output_tokens: None,
            temperature: None,
        }
    }
}

impl GeminiContent {
    fn into_transport(self) -> Result<ChatTransportMessage, RestError> {
        Ok(ChatTransportMessage {
            role: gemini_role(self.role.as_deref())?,
            content: gemini_content_text(self)?,
        })
    }
}

impl GeminiStreamMapper {
    fn new(context: GeminiContext) -> Self {
        Self { context }
    }
}

impl ChatStreamMapper for GeminiStreamMapper {
    fn delta(&mut self, text: String) -> Vec<axum::response::sse::Event> {
        vec![sse_json_event(
            None,
            &gemini_response(self.context.clone(), Some(text), None),
        )]
    }

    fn done(&mut self, finish_reason: ChatFinishReason) -> Vec<axum::response::sse::Event> {
        vec![sse_json_event(
            None,
            &gemini_response(self.context.clone(), None, Some(finish_reason)),
        )]
    }

    fn error(&mut self, code: &str, message: String) -> Vec<axum::response::sse::Event> {
        vec![sse_json_event(
            None,
            &json!({
                "error": {
                    "code": code,
                    "message": message
                }
            }),
        )]
    }
}

fn gemini_response(
    context: GeminiContext,
    text: Option<String>,
    finish_reason: Option<ChatFinishReason>,
) -> serde_json::Value {
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
            "finishReason": finish_reason.map(|reason| gemini_finish_reason(&reason).to_string())
        }],
        "usageMetadata": null,
        "modelVersion": context.model
    })
}

fn gemini_role(role: Option<&str>) -> Result<String, RestError> {
    match role.unwrap_or("user").trim().to_ascii_lowercase().as_str() {
        "user" => Ok("user".to_string()),
        "model" | "assistant" => Ok("assistant".to_string()),
        "system" => Ok("system".to_string()),
        "" => Err(RestError::bad_request(
            "bad_request",
            "content role is empty",
        )),
        other => Err(RestError::bad_request(
            "bad_request",
            format!("unsupported Gemini content role `{other}`"),
        )),
    }
}

fn gemini_content_text(content: GeminiContent) -> Result<String, RestError> {
    let mut text = String::new();
    for part in content.parts {
        match part.text {
            Some(value) => text.push_str(&value),
            None => {
                return Err(ProviderCompatRejection::unsupported_content(
                    "only Gemini text parts are supported",
                )
                .into());
            }
        }
    }
    Ok(text)
}

fn gemini_finish_reason(reason: &ChatFinishReason) -> &str {
    match reason {
        ChatFinishReason::Stop => "STOP",
        ChatFinishReason::Length => "MAX_TOKENS",
        ChatFinishReason::Cancelled => "OTHER",
        ChatFinishReason::Error => "OTHER",
        ChatFinishReason::Other(value) => value.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_stream_response_uses_gemini_shape_and_unknown_usage() {
        let body = gemini_response(
            GeminiContext {
                model: "gemma-alias".to_string(),
            },
            Some("hello".to_string()),
            Some(ChatFinishReason::Stop),
        );

        assert_eq!(body["modelVersion"], "gemma-alias");
        assert_eq!(body["candidates"][0]["content"]["role"], "model");
        assert_eq!(
            body["candidates"][0]["content"]["parts"][0]["text"],
            "hello"
        );
        assert_eq!(body["candidates"][0]["finishReason"], "STOP");
        assert!(body["usageMetadata"].is_null());
    }

    #[test]
    fn request_rejects_tools_before_kernel_mapping() {
        let request: GeminiGenerateContentRequest = serde_json::from_value(json!({
            "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
            "tools": [{"functionDeclarations": [{"name": "lookup"}]}]
        }))
        .expect("request");

        let error = request
            .into_transport("gemma-alias".to_string())
            .expect_err("tools unsupported");
        assert!(format!("{error:?}").contains("unsupported_provider_field"));
    }

    #[test]
    fn request_accepts_system_instruction_contents_and_generation_config() {
        let request: GeminiGenerateContentRequest = serde_json::from_value(json!({
            "systemInstruction": {
                "parts": [{"text": "Answer briefly."}]
            },
            "contents": [
                {"role": "user", "parts": [{"text": "hi"}]},
                {"role": "model", "parts": [{"text": "hello"}]},
                {"parts": [{"text": "again"}]}
            ],
            "generationConfig": {
                "maxOutputTokens": 12,
                "temperature": 0.2
            },
            "adapter_ref": "adapter-a"
        }))
        .expect("request");

        let request = request
            .into_transport("gemma-alias".to_string())
            .expect("transport request");

        assert_eq!(request.model_ref, "gemma-alias");
        assert_eq!(request.adapter_ref.as_deref(), Some("adapter-a"));
        assert_eq!(request.cloud_provider, Some(Provider::Gemini));
        assert_eq!(request.max_tokens, Some(12));
        assert_eq!(request.temperature, Some(0.2));
        assert_eq!(
            request.messages,
            vec![
                ChatTransportMessage {
                    role: "system".to_string(),
                    content: "Answer briefly.".to_string()
                },
                ChatTransportMessage {
                    role: "user".to_string(),
                    content: "hi".to_string()
                },
                ChatTransportMessage {
                    role: "assistant".to_string(),
                    content: "hello".to_string()
                },
                ChatTransportMessage {
                    role: "user".to_string(),
                    content: "again".to_string()
                }
            ]
        );
    }

    #[test]
    fn request_rejects_tool_config_before_kernel_mapping() {
        let request: GeminiGenerateContentRequest = serde_json::from_value(json!({
            "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
            "toolConfig": {"functionCallingConfig": {"mode": "AUTO"}}
        }))
        .expect("request");

        let error = request
            .into_transport("gemma-alias".to_string())
            .expect_err("toolConfig unsupported");
        assert!(format!("{error:?}").contains("unsupported_provider_field"));
    }

    #[test]
    fn operation_rejects_unsupported_suffix() {
        let error = GeminiOperation::parse("gemma-alias:countTokens".to_string())
            .expect_err("unsupported operation");

        assert!(format!("{error:?}").contains("unsupported_provider_operation"));
    }

    #[test]
    fn operation_accepts_stream_generate_content_suffix() {
        let operation = GeminiOperation::parse("gemma-alias:streamGenerateContent".to_string())
            .expect("operation");

        assert_eq!(operation.model, "gemma-alias");
        assert!(operation.stream);
    }

    #[test]
    fn request_rejects_non_text_parts() {
        let request: GeminiGenerateContentRequest = serde_json::from_value(json!({
            "contents": [{
                "role": "user",
                "parts": [{"inlineData": {"mimeType": "image/png", "data": "AA=="}}]
            }]
        }))
        .expect("request");

        let error = request
            .into_transport("gemma-alias".to_string())
            .expect_err("inline data unsupported");
        assert!(format!("{error:?}").contains("unsupported_provider_content"));
    }
}
