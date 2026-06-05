use axum::{
    extract::State,
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
    chat_preparation_request, complete_chat, response_id, sse_json_event, stream_chat_response,
    ChatStreamMapper, ChatTransportMessage, ChatTransportRequest,
};

pub(crate) async fn messages(
    State(state): State<RestState>,
    Json(request): Json<ClaudeMessagesRequest>,
) -> Result<Response, RestError> {
    let context = ClaudeContext::new(&request.model);
    let stream = request.stream.unwrap_or(false);
    let request = request.into_transport()?;
    let request = chat_preparation_request(&state, request, stream)?;
    if stream {
        return Ok(stream_chat_response(
            state,
            request,
            ClaudeStreamMapper::new(context),
        ));
    }

    let result = complete_chat(state, request).await?;
    Ok(Json(claude_response(
        context,
        result.response.text,
        result.response.finish_reason,
    ))
    .into_response())
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClaudeMessagesRequest {
    model: String,
    messages: Vec<ClaudeMessage>,
    system: Option<ClaudeContent>,
    adapter_ref: Option<String>,
    max_tokens: u32,
    temperature: Option<f32>,
    stream: Option<bool>,
    tools: Option<serde_json::Value>,
    tool_choice: Option<serde_json::Value>,
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
}

#[derive(Debug, Clone)]
struct ClaudeContext {
    id: String,
    model: String,
}

struct ClaudeStreamMapper {
    context: ClaudeContext,
}

impl ClaudeMessagesRequest {
    fn into_transport(self) -> Result<ChatTransportRequest, RestError> {
        self.reject_unsupported()?;
        let mut messages = Vec::new();
        if let Some(system) = self.system {
            messages.push(ChatTransportMessage {
                role: "system".to_string(),
                content: claude_content(system)?,
            });
        }
        for message in self.messages {
            messages.push(message.into_transport()?);
        }

        Ok(ChatTransportRequest {
            model_ref: self.model,
            adapter_ref: self.adapter_ref,
            cloud_provider: Some(Provider::Anthropic),
            messages,
            max_tokens: Some(self.max_tokens),
            temperature: self.temperature,
        })
    }

    fn reject_unsupported(&self) -> Result<(), RestError> {
        if self.tools.is_some() || self.tool_choice.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "Claude-compatible tools require kernel tool-call support",
            )
            .into());
        }
        Ok(())
    }
}

impl ClaudeMessage {
    fn into_transport(self) -> Result<ChatTransportMessage, RestError> {
        Ok(ChatTransportMessage {
            role: claude_role(&self.role)?,
            content: claude_content(self.content)?,
        })
    }
}

impl ClaudeContext {
    fn new(model: &str) -> Self {
        Self {
            id: response_id("msg"),
            model: model.to_string(),
        }
    }
}

impl ClaudeStreamMapper {
    fn new(context: ClaudeContext) -> Self {
        Self { context }
    }
}

impl ChatStreamMapper for ClaudeStreamMapper {
    fn start(&mut self) -> Vec<axum::response::sse::Event> {
        vec![
            sse_json_event(
                Some("message_start"),
                &json!({
                    "type": "message_start",
                    "message": {
                        "id": self.context.id,
                        "type": "message",
                        "role": "assistant",
                        "model": self.context.model,
                        "content": [],
                        "stop_reason": null,
                        "stop_sequence": null,
                        "usage": claude_usage()
                    }
                }),
            ),
            sse_json_event(
                Some("content_block_start"),
                &json!({
                    "type": "content_block_start",
                    "index": 0,
                    "content_block": {
                        "type": "text",
                        "text": ""
                    }
                }),
            ),
        ]
    }

    fn delta(&mut self, text: String) -> Vec<axum::response::sse::Event> {
        vec![sse_json_event(
            Some("content_block_delta"),
            &json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {
                    "type": "text_delta",
                    "text": text
                }
            }),
        )]
    }

    fn done(&mut self, finish_reason: ChatFinishReason) -> Vec<axum::response::sse::Event> {
        vec![
            sse_json_event(
                Some("content_block_stop"),
                &json!({
                    "type": "content_block_stop",
                    "index": 0
                }),
            ),
            sse_json_event(
                Some("message_delta"),
                &json!({
                    "type": "message_delta",
                    "delta": {
                        "stop_reason": claude_stop_reason(&finish_reason),
                        "stop_sequence": null
                    },
                    "usage": claude_usage()
                }),
            ),
            sse_json_event(
                Some("message_stop"),
                &json!({
                    "type": "message_stop"
                }),
            ),
        ]
    }

    fn error(&mut self, code: &str, message: String) -> Vec<axum::response::sse::Event> {
        vec![sse_json_event(
            Some("error"),
            &json!({
                "type": "error",
                "error": {
                    "type": code,
                    "message": message
                }
            }),
        )]
    }
}

fn claude_response(
    context: ClaudeContext,
    text: String,
    finish_reason: ChatFinishReason,
) -> serde_json::Value {
    json!({
        "id": context.id,
        "type": "message",
        "role": "assistant",
        "content": [{
            "type": "text",
            "text": text
        }],
        "model": context.model,
        "stop_reason": claude_stop_reason(&finish_reason),
        "stop_sequence": null,
        "usage": claude_usage()
    })
}

fn claude_usage() -> serde_json::Value {
    serde_json::Value::Null
}

fn claude_role(role: &str) -> Result<String, RestError> {
    match role.trim().to_ascii_lowercase().as_str() {
        "system" => Ok("system".to_string()),
        "user" => Ok("user".to_string()),
        "assistant" => Ok("assistant".to_string()),
        "" => Err(RestError::bad_request(
            "bad_request",
            "message role is empty",
        )),
        other => Err(RestError::bad_request(
            "bad_request",
            format!("unsupported Claude message role `{other}`"),
        )),
    }
}

fn claude_content(content: ClaudeContent) -> Result<String, RestError> {
    match content {
        ClaudeContent::Text(text) => Ok(text),
        ClaudeContent::Blocks(blocks) => {
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

fn claude_stop_reason(reason: &ChatFinishReason) -> &str {
    match reason {
        ChatFinishReason::Stop => "end_turn",
        ChatFinishReason::Length => "max_tokens",
        ChatFinishReason::Cancelled => "stop_sequence",
        ChatFinishReason::Error => "stop_sequence",
        ChatFinishReason::Other(value) => value.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_stream_response_uses_claude_shape_and_unknown_usage() {
        let body = claude_response(
            ClaudeContext::new("gemma-alias"),
            "hello".to_string(),
            ChatFinishReason::Stop,
        );

        assert_eq!(body["type"], "message");
        assert_eq!(body["role"], "assistant");
        assert_eq!(body["model"], "gemma-alias");
        assert_eq!(body["content"][0]["type"], "text");
        assert_eq!(body["content"][0]["text"], "hello");
        assert_eq!(body["stop_reason"], "end_turn");
        assert!(body["usage"].is_null());
    }

    #[test]
    fn request_rejects_tools_before_kernel_mapping() {
        let request: ClaudeMessagesRequest = serde_json::from_value(json!({
            "model": "gemma-alias",
            "max_tokens": 16,
            "messages": [{"role": "user", "content": "hi"}],
            "tools": [{"name": "lookup", "input_schema": {"type": "object"}}]
        }))
        .expect("request");

        let error = request.into_transport().expect_err("tools unsupported");
        assert!(format!("{error:?}").contains("unsupported_provider_field"));
    }

    #[test]
    fn request_rejects_non_text_content_blocks() {
        let request: ClaudeMessagesRequest = serde_json::from_value(json!({
            "model": "gemma-alias",
            "max_tokens": 16,
            "messages": [{
                "role": "user",
                "content": [{"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AA=="}}]
            }]
        }))
        .expect("request");

        let error = request.into_transport().expect_err("image unsupported");
        assert!(format!("{error:?}").contains("unsupported_provider_content"));
    }
}
