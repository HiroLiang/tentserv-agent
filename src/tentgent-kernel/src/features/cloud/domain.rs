//! Cloud provider request and response domain types.

use crate::features::auth::domain::Provider;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudEndpointCapability {
    Chat,
    VisionChat,
    Embedding,
    ImageGeneration,
}

impl CloudEndpointCapability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::VisionChat => "vision-chat",
            Self::Embedding => "embedding",
            Self::ImageGeneration => "image-generation",
        }
    }
}

pub fn provider_supports(provider: Provider, capability: CloudEndpointCapability) -> bool {
    provider_capabilities(provider).contains(&capability)
}

pub fn provider_capabilities(provider: Provider) -> &'static [CloudEndpointCapability] {
    match provider {
        Provider::OpenAI => &[
            CloudEndpointCapability::Chat,
            CloudEndpointCapability::VisionChat,
            CloudEndpointCapability::Embedding,
            CloudEndpointCapability::ImageGeneration,
        ],
        Provider::Anthropic => &[
            CloudEndpointCapability::Chat,
            CloudEndpointCapability::VisionChat,
        ],
        Provider::Gemini => &[
            CloudEndpointCapability::Chat,
            CloudEndpointCapability::VisionChat,
            CloudEndpointCapability::Embedding,
            CloudEndpointCapability::ImageGeneration,
        ],
        Provider::HuggingFace => &[],
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CloudChatRequest {
    pub provider: Provider,
    pub model: String,
    pub messages: Vec<CloudChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: bool,
    pub response_modalities: Option<Vec<String>>,
    pub audio: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CloudChatMessage {
    pub role: String,
    pub content: Vec<CloudChatContentPart>,
}

impl CloudChatMessage {
    pub fn text(role: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: vec![CloudChatContentPart::Text(text.into())],
        }
    }

    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|part| match part {
                CloudChatContentPart::Text(text) => Some(text.as_str()),
                CloudChatContentPart::ImageUrl { .. }
                | CloudChatContentPart::ImageBase64 { .. }
                | CloudChatContentPart::AudioBase64 { .. }
                | CloudChatContentPart::InputAudio { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    pub fn has_image(&self) -> bool {
        self.content.iter().any(|part| {
            matches!(
                part,
                CloudChatContentPart::ImageUrl { .. } | CloudChatContentPart::ImageBase64 { .. }
            )
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CloudChatContentPart {
    Text(String),
    ImageUrl { url: String },
    ImageBase64 { media_type: String, data: String },
    AudioBase64 { media_type: String, data: String },
    InputAudio { data: String, format: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudChatResponse {
    pub text: String,
    pub finish_reason: String,
    pub audio: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CloudStreamEvent {
    Delta { text: String },
    Done { finish_reason: String },
    Error { code: String, message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudEmbeddingRequest {
    pub provider: Provider,
    pub model: String,
    pub input: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CloudEmbeddingResponse {
    pub vectors: Vec<Vec<f32>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CloudImageGenerationRequest {
    pub provider: Provider,
    pub model: String,
    pub prompt: String,
    pub size: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudImageGenerationResponse {
    pub b64_json: String,
    pub media_type: String,
}
