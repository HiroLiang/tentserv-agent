//! Chat request, target, response, and streaming domain types.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::features::adapter::domain::{AdapterBackendSupport, AdapterRef};
use crate::features::auth::domain::Provider;
use crate::features::model::domain::{ModelCapability, ModelFormat, ModelRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

impl ChatRole {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ChatRoleParseError> {
        let normalized = value.as_ref().trim().to_ascii_lowercase();
        match normalized.as_str() {
            "" => Err(ChatRoleParseError::Empty),
            "system" => Ok(Self::System),
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            _ => Err(ChatRoleParseError::Unsupported { value: normalized }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

impl std::fmt::Display for ChatRole {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChatRoleParseError {
    #[error("chat role is empty")]
    Empty,
    #[error("unsupported chat role `{value}`")]
    Unsupported { value: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

impl ChatMessage {
    pub fn new(
        role: ChatRole,
        content: impl Into<String>,
    ) -> Result<Self, ChatMessageValidationError> {
        let content = content.into().trim().to_string();
        if content.is_empty() {
            return Err(ChatMessageValidationError::EmptyContent { role });
        }

        Ok(Self { role, content })
    }

    pub fn system(content: impl Into<String>) -> Result<Self, ChatMessageValidationError> {
        Self::new(ChatRole::System, content)
    }

    pub fn user(content: impl Into<String>) -> Result<Self, ChatMessageValidationError> {
        Self::new(ChatRole::User, content)
    }

    pub fn assistant(content: impl Into<String>) -> Result<Self, ChatMessageValidationError> {
        Self::new(ChatRole::Assistant, content)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChatMessageValidationError {
    #[error("{role} chat message content is empty")]
    EmptyContent { role: ChatRole },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatPrompt {
    pub messages: Vec<ChatMessage>,
}

impl ChatPrompt {
    pub fn new(messages: Vec<ChatMessage>) -> Result<Self, ChatPromptValidationError> {
        if messages.is_empty() {
            return Err(ChatPromptValidationError::Empty);
        }

        Ok(Self { messages })
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChatPromptValidationError {
    #[error("chat prompt must contain at least one message")]
    Empty,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatGenerationOptions {
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: bool,
}

impl Default for ChatGenerationOptions {
    fn default() -> Self {
        Self {
            max_tokens: None,
            temperature: None,
            stream: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChatBackend {
    TransformersPeft,
    Mlx,
    LlamaCpp,
}

impl ChatBackend {
    pub const fn from_model_format(format: ModelFormat) -> Option<Self> {
        match format {
            ModelFormat::Safetensors => Some(Self::TransformersPeft),
            ModelFormat::Gguf => Some(Self::LlamaCpp),
            ModelFormat::Mlx => Some(Self::Mlx),
            ModelFormat::Diffusers => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TransformersPeft => "transformers-peft",
            Self::Mlx => "mlx",
            Self::LlamaCpp => "llama-cpp",
        }
    }

    pub const fn adapter_backend_support(self) -> AdapterBackendSupport {
        match self {
            Self::TransformersPeft => AdapterBackendSupport::TransformersPeft,
            Self::Mlx => AdapterBackendSupport::Mlx,
            Self::LlamaCpp => AdapterBackendSupport::LlamaCpp,
        }
    }
}

impl std::fmt::Display for ChatBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatRuntimeTarget {
    LocalModel {
        model_ref: ModelRef,
        backend: ChatBackend,
        source_repo: Option<String>,
        source_revision: Option<String>,
        model_capabilities: Vec<ModelCapability>,
    },
    CloudProvider {
        provider: Provider,
        provider_model: String,
    },
}

impl ChatRuntimeTarget {
    pub fn supports_adapters(&self) -> bool {
        matches!(self, Self::LocalModel { .. })
    }

    pub fn model_label(&self) -> String {
        match self {
            Self::LocalModel { model_ref, .. } => model_ref.to_string(),
            Self::CloudProvider { provider_model, .. } => provider_model.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedChatAdapter {
    pub adapter_ref: AdapterRef,
    pub backend: AdapterBackendSupport,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedChatTarget {
    pub runtime: ChatRuntimeTarget,
    pub adapter: Option<ResolvedChatAdapter>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub target: ResolvedChatTarget,
    pub prompt: ChatPrompt,
    pub options: ChatGenerationOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatFinishReason {
    Stop,
    Length,
    Cancelled,
    Error,
    Other(String),
}

impl ChatFinishReason {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Stop => "stop",
            Self::Length => "length",
            Self::Cancelled => "cancelled",
            Self::Error => "error",
            Self::Other(value) => value.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatResponse {
    pub text: String,
    pub finish_reason: ChatFinishReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ChatStreamEvent {
    Delta { text: String },
    Done { finish_reason: ChatFinishReason },
    Error { code: String, message: String },
}
