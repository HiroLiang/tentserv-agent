//! Embedding request, target, response, and backend domain types.

use serde::{Deserialize, Serialize};

use crate::features::model::domain::{ModelCapability, ModelFormat, ModelRef};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingInput {
    pub items: Vec<String>,
}

impl EmbeddingInput {
    pub fn new(items: Vec<String>) -> Result<Self, EmbeddingInputValidationError> {
        if items.is_empty() {
            return Err(EmbeddingInputValidationError::Empty);
        }
        if items.iter().any(|item| item.trim().is_empty()) {
            return Err(EmbeddingInputValidationError::EmptyItem);
        }

        Ok(Self { items })
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EmbeddingInputValidationError {
    #[error("embedding input must contain at least one string")]
    Empty,
    #[error("embedding input strings must not be empty")]
    EmptyItem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EmbeddingBackend {
    TransformersPeft,
}

impl EmbeddingBackend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TransformersPeft => "transformers-peft",
        }
    }

    pub const fn from_model_format(format: ModelFormat) -> Option<Self> {
        match format {
            ModelFormat::Safetensors => Some(Self::TransformersPeft),
            ModelFormat::Gguf | ModelFormat::Mlx => None,
        }
    }
}

impl std::fmt::Display for EmbeddingBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmbeddingRuntimeTarget {
    LocalModel {
        model_ref: ModelRef,
        backend: EmbeddingBackend,
        source_repo: Option<String>,
        source_revision: Option<String>,
        model_capabilities: Vec<ModelCapability>,
    },
}

impl EmbeddingRuntimeTarget {
    pub fn model_label(&self) -> String {
        match self {
            Self::LocalModel { model_ref, .. } => model_ref.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedEmbeddingTarget {
    pub runtime: EmbeddingRuntimeTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub target: ResolvedEmbeddingTarget,
    pub input: EmbeddingInput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingVector {
    pub index: usize,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub data: Vec<EmbeddingVector>,
}
