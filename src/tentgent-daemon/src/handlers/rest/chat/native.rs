use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use tentgent_kernel::features::chat::domain::ChatFinishReason;

use crate::transport::rest::{error::RestError, state::RestState};

use super::{
    dto::{chat_response, ChatRequest, DeltaEvent, DoneEvent, ErrorEvent},
    execution::{
        chat_preparation_request, complete_chat, finish_reason_str, sse_json_event,
        stream_chat_response, ChatStreamMapper, ChatTransportMessage, ChatTransportRequest,
    },
};

pub async fn complete(
    State(state): State<RestState>,
    Json(request): Json<ChatRequest>,
) -> Result<Response, RestError> {
    let stream = request.stream.unwrap_or(false);
    let request = chat_preparation_request(&state, request.into_transport(), stream)?;
    if stream {
        return Ok(stream_chat_response(state, request, NativeStreamMapper));
    }

    let result = complete_chat(state, request).await?;
    Ok(Json(chat_response(result)).into_response())
}

struct NativeStreamMapper;

impl ChatStreamMapper for NativeStreamMapper {
    fn delta(&mut self, text: String) -> Vec<axum::response::sse::Event> {
        vec![sse_json_event(Some("delta"), &DeltaEvent { delta: &text })]
    }

    fn done(&mut self, finish_reason: ChatFinishReason) -> Vec<axum::response::sse::Event> {
        vec![sse_json_event(
            Some("done"),
            &DoneEvent {
                finish_reason: finish_reason_str(&finish_reason),
            },
        )]
    }

    fn error(&mut self, code: &str, message: String) -> Vec<axum::response::sse::Event> {
        vec![sse_json_event(
            Some("error"),
            &ErrorEvent {
                error: code,
                message,
            },
        )]
    }
}

impl ChatRequest {
    fn into_transport(self) -> ChatTransportRequest {
        ChatTransportRequest {
            model_ref: self.model_ref,
            adapter_ref: self.adapter_ref,
            cloud_provider: None,
            messages: self
                .messages
                .into_iter()
                .map(|message| ChatTransportMessage {
                    role: message.role,
                    content: message.content,
                })
                .collect(),
            max_tokens: self.max_tokens,
            temperature: self.temperature,
        }
    }
}
