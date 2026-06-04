use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tentgent_kernel::features::{auth::domain::Provider, chat::domain::ChatFinishReason};

use crate::{
    provider_compat::{OpenAiChatCompatFields, OpenAiTextMessage},
    time::unix_timestamp_seconds,
    transport::rest::{error::RestError, state::RestState},
};

use super::execution::{
    chat_preparation_request, complete_chat, finish_reason_str, response_id, sse_data_event,
    sse_json_event, stream_chat_response, ChatStreamMapper, ChatTransportMessage,
    ChatTransportRequest,
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
    messages: Vec<OpenAiTextMessage>,
    adapter_ref: Option<String>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    stream: Option<bool>,
    #[serde(flatten)]
    compat: OpenAiChatCompatFields,
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
        self.compat.reject_unsupported()?;
        let max_tokens = self.max_tokens.or(self.compat.max_completion_tokens());
        Ok(ChatTransportRequest {
            model_ref: self.model,
            adapter_ref: self.adapter_ref,
            cloud_provider: Some(Provider::OpenAI),
            messages: self
                .messages
                .into_iter()
                .map(openai_message_into_transport)
                .collect::<Result<Vec<_>, _>>()?,
            max_tokens,
            temperature: self.temperature,
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

fn openai_message_into_transport(
    message: OpenAiTextMessage,
) -> Result<ChatTransportMessage, RestError> {
    let message = message.into_text_message()?;
    Ok(ChatTransportMessage {
        role: message.role,
        content: message.content,
    })
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
        assert!(format!("{error:?}").contains("unsupported_provider_field"));
    }

    #[test]
    fn request_rejects_response_format_before_kernel_mapping() {
        let request: OpenAiChatCompletionRequest = serde_json::from_value(json!({
            "model": "gemma-alias",
            "messages": [{"role": "user", "content": "hi"}],
            "response_format": {"type": "json_object"}
        }))
        .expect("request");

        let error = request
            .into_transport()
            .expect_err("response_format unsupported");
        assert!(format!("{error:?}").contains("unsupported_provider_field"));
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
        assert!(format!("{error:?}").contains("unsupported_provider_content"));
    }
}
