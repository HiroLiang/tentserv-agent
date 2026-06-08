use serde::{Deserialize, Serialize};

use crate::provider_compat::ProviderChatTextMessage;

#[derive(Debug, Serialize)]
pub(super) struct NativeLocalEmbeddingRequest {
    pub(super) input: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct NativeLocalEmbeddingResponse {
    pub(super) model_ref: String,
    pub(super) data: Vec<NativeLocalEmbeddingItem>,
}

#[derive(Debug, Serialize)]
pub(super) struct NativeLocalImageGenerationRequest {
    pub(super) prompt: String,
    pub(super) output_path: String,
    pub(super) output_format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) height: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub(super) struct NativeLocalImageGenerationResponse {
    pub(super) model_ref: String,
    pub(super) output_path: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct NativeLocalEmbeddingItem {
    pub(super) index: usize,
    pub(super) embedding: Vec<f32>,
}

#[derive(Debug, Serialize)]
pub(super) struct NativeLocalChatRequest {
    pub(super) messages: Vec<NativeLocalChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
pub(super) struct NativeLocalChatMessage {
    pub(super) role: String,
    pub(super) content: String,
}

impl From<ProviderChatTextMessage> for NativeLocalChatMessage {
    fn from(message: ProviderChatTextMessage) -> Self {
        Self {
            role: message.role,
            content: message.content,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct NativeLocalChatResponse {
    pub(super) text: String,
}
