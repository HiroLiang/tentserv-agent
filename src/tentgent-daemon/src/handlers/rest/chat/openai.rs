use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tentgent_kernel::features::chat::domain::ChatFinishReason;

use crate::transport::rest::{error::RestError, state::RestState};

use super::execution::{
    chat_preparation_request, complete_chat, finish_reason_str, response_id, sse_data_event,
    sse_json_event, stream_chat_response, unix_timestamp_seconds, ChatStreamMapper,
    ChatTransportMessage, ChatTransportRequest,
};

pub(crate) async fn chat_completions(
    State(state): State<RestState>,
    Json(request): Json<OpenAiChatCompletionRequest>,
) -> Result<Response, RestError> {
    let context = OpenAiContext::new(&request.model);
    let stream = request.stream.unwrap_or(false);
    let request = request.into_transport()?;
    let request = chat_preparation_request(&state, request, stream)?;
    if stream {
        return Ok(stream_chat_response(
            state,
            request,
            OpenAiStreamMapper::new(context),
        ));
    }

    let result = complete_chat(state, request).await?;
    Ok(Json(openai_response(
        context,
        result.response.text,
        result.response.finish_reason,
    ))
    .into_response())
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenAiChatCompletionRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    adapter_ref: Option<String>,
    max_tokens: Option<u32>,
    max_completion_tokens: Option<u32>,
    temperature: Option<f32>,
    stream: Option<bool>,
    tools: Option<serde_json::Value>,
    tool_choice: Option<serde_json::Value>,
    functions: Option<serde_json::Value>,
    function_call: Option<serde_json::Value>,
    modalities: Option<Vec<String>>,
    audio: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    role: String,
    content: OpenAiContent,
    tool_calls: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OpenAiContent {
    Text(String),
    Parts(Vec<OpenAiContentPart>),
}

#[derive(Debug, Deserialize)]
struct OpenAiContentPart {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Debug, Clone)]
struct OpenAiContext {
    id: String,
    model: String,
    created: u64,
}

#[derive(Debug, Serialize)]
struct OpenAiDelta<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<&'a str>,
}

impl OpenAiChatCompletionRequest {
    fn into_transport(self) -> Result<ChatTransportRequest, RestError> {
        self.reject_unsupported()?;
        Ok(ChatTransportRequest {
            model_ref: self.model,
            adapter_ref: self.adapter_ref,
            messages: self
                .messages
                .into_iter()
                .map(OpenAiMessage::into_transport)
                .collect::<Result<Vec<_>, _>>()?,
            max_tokens: self.max_tokens.or(self.max_completion_tokens),
            temperature: self.temperature,
        })
    }

    fn reject_unsupported(&self) -> Result<(), RestError> {
        if self.tools.is_some()
            || self.tool_choice.is_some()
            || self.functions.is_some()
            || self.function_call.is_some()
        {
            return Err(RestError::bad_request(
                "unsupported_chat_feature",
                "OpenAI-compatible tools and function calling require kernel tool-call support",
            ));
        }
        if self.audio.is_some()
            || self
                .modalities
                .as_ref()
                .is_some_and(|modalities| modalities.iter().any(|value| value != "text"))
        {
            return Err(RestError::bad_request(
                "unsupported_chat_feature",
                "OpenAI-compatible audio output requires kernel multimodal support",
            ));
        }
        Ok(())
    }
}

impl OpenAiMessage {
    fn into_transport(self) -> Result<ChatTransportMessage, RestError> {
        if self.tool_calls.is_some() {
            return Err(RestError::bad_request(
                "unsupported_chat_feature",
                "OpenAI-compatible tool call messages require kernel tool-call support",
            ));
        }
        Ok(ChatTransportMessage {
            role: openai_role(&self.role)?,
            content: openai_content(self.content)?,
        })
    }
}

impl OpenAiContext {
    fn new(model: &str) -> Self {
        Self {
            id: response_id("chatcmpl"),
            model: model.to_string(),
            created: unix_timestamp_seconds(),
        }
    }
}

impl ChatStreamMapper for OpenAiStreamMapper {
    fn start(&mut self) -> Vec<axum::response::sse::Event> {
        vec![openai_chunk(
            &self.context,
            Some(OpenAiDelta {
                role: Some("assistant"),
                content: None,
            }),
            None,
        )]
    }

    fn delta(&mut self, text: String) -> Vec<axum::response::sse::Event> {
        vec![openai_chunk(
            &self.context,
            Some(OpenAiDelta {
                role: None,
                content: Some(&text),
            }),
            None,
        )]
    }

    fn done(&mut self, finish_reason: ChatFinishReason) -> Vec<axum::response::sse::Event> {
        vec![
            openai_chunk(
                &self.context,
                Some(OpenAiDelta {
                    role: None,
                    content: None,
                }),
                Some(openai_finish_reason(&finish_reason)),
            ),
            sse_data_event("[DONE]"),
        ]
    }

    fn error(&mut self, code: &str, message: String) -> Vec<axum::response::sse::Event> {
        vec![
            sse_json_event(
                None,
                &json!({
                    "error": {
                        "message": message,
                        "type": code,
                        "code": code
                    }
                }),
            ),
            sse_data_event("[DONE]"),
        ]
    }
}

struct OpenAiStreamMapper {
    context: OpenAiContext,
}

impl OpenAiStreamMapper {
    fn new(context: OpenAiContext) -> Self {
        Self { context }
    }
}

fn openai_response(
    context: OpenAiContext,
    text: String,
    finish_reason: ChatFinishReason,
) -> serde_json::Value {
    json!({
        "id": context.id,
        "object": "chat.completion",
        "created": context.created,
        "model": context.model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": text,
            },
            "finish_reason": openai_finish_reason(&finish_reason),
            "logprobs": null
        }],
        "usage": null
    })
}

fn openai_chunk(
    context: &OpenAiContext,
    delta: Option<OpenAiDelta<'_>>,
    finish_reason: Option<&str>,
) -> axum::response::sse::Event {
    sse_json_event(
        None,
        &json!({
            "id": context.id,
            "object": "chat.completion.chunk",
            "created": context.created,
            "model": context.model,
            "choices": [{
                "index": 0,
                "delta": delta,
                "finish_reason": finish_reason,
                "logprobs": null
            }],
            "usage": null
        }),
    )
}

fn openai_role(role: &str) -> Result<String, RestError> {
    match role.trim().to_ascii_lowercase().as_str() {
        "developer" | "system" => Ok("system".to_string()),
        "user" => Ok("user".to_string()),
        "assistant" => Ok("assistant".to_string()),
        "" => Err(RestError::bad_request(
            "bad_request",
            "message role is empty",
        )),
        other => Err(RestError::bad_request(
            "bad_request",
            format!("unsupported OpenAI message role `{other}`"),
        )),
    }
}

fn openai_content(content: OpenAiContent) -> Result<String, RestError> {
    match content {
        OpenAiContent::Text(text) => Ok(text),
        OpenAiContent::Parts(parts) => {
            let mut text = String::new();
            for part in parts {
                if part.kind != "text" {
                    return Err(RestError::bad_request(
                        "bad_request",
                        format!("unsupported OpenAI content part `{}`", part.kind),
                    ));
                }
                text.push_str(part.text.as_deref().unwrap_or_default());
            }
            Ok(text)
        }
    }
}

fn openai_finish_reason(reason: &ChatFinishReason) -> &str {
    match reason {
        ChatFinishReason::Stop => "stop",
        ChatFinishReason::Length => "length",
        ChatFinishReason::Cancelled => "stop",
        ChatFinishReason::Error => "error",
        ChatFinishReason::Other(_) => finish_reason_str(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_stream_response_uses_openai_shape_and_unknown_usage() {
        let body = openai_response(
            OpenAiContext::new("gemma-alias"),
            "hello".to_string(),
            ChatFinishReason::Stop,
        );

        assert_eq!(body["object"], "chat.completion");
        assert_eq!(body["model"], "gemma-alias");
        assert_eq!(body["choices"][0]["message"]["role"], "assistant");
        assert_eq!(body["choices"][0]["message"]["content"], "hello");
        assert_eq!(body["choices"][0]["finish_reason"], "stop");
        assert!(body["usage"].is_null());
    }

    #[test]
    fn request_rejects_tools_before_kernel_mapping() {
        let request: OpenAiChatCompletionRequest = serde_json::from_value(json!({
            "model": "gemma-alias",
            "messages": [{"role": "user", "content": "hi"}],
            "tools": [{"type": "function", "function": {"name": "lookup"}}]
        }))
        .expect("request");

        let error = request.into_transport().expect_err("tools unsupported");
        assert!(format!("{error:?}").contains("unsupported_chat_feature"));
    }

    #[test]
    fn request_rejects_non_text_content_parts() {
        let request: OpenAiChatCompletionRequest = serde_json::from_value(json!({
            "model": "gemma-alias",
            "messages": [{
                "role": "user",
                "content": [{"type": "image_url", "image_url": {"url": "data:image/png;base64,AA=="}}]
            }]
        }))
        .expect("request");

        let error = request.into_transport().expect_err("image unsupported");
        assert!(format!("{error:?}").contains("bad_request"));
    }
}
