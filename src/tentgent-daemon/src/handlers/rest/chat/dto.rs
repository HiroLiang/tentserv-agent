use serde::{Deserialize, Serialize};
use tentgent_kernel::features::chat::{domain::ChatFinishReason, usecases::ChatCompletionResult};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatRequest {
    pub model_ref: String,
    pub adapter_ref: Option<String>,
    pub messages: Vec<ChatMessageRequest>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatMessageRequest {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub text: String,
    pub finish_reason: String,
    pub model_ref: String,
    pub adapter_ref: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct DeltaEvent<'a> {
    pub delta: &'a str,
}

#[derive(Debug, Serialize)]
pub(super) struct DoneEvent<'a> {
    pub finish_reason: &'a str,
}

#[derive(Debug, Serialize)]
pub(super) struct ErrorEvent<'a> {
    pub error: &'a str,
    pub message: String,
}

pub fn chat_response(result: ChatCompletionResult) -> ChatResponse {
    ChatResponse {
        text: result.response.text,
        finish_reason: finish_reason_str(&result.response.finish_reason).to_string(),
        model_ref: result
            .prepared
            .model
            .map(|model| model.metadata.model_ref.into_string())
            .unwrap_or_default(),
        adapter_ref: result
            .prepared
            .adapter
            .map(|adapter| adapter.metadata.adapter_ref.into_string()),
    }
}

pub(super) fn finish_reason_str(reason: &ChatFinishReason) -> &str {
    reason.as_str()
}
