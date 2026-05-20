//! Vision chat request, target, and response domain types.

use std::{fmt, path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::features::model::domain::{ModelCapability, ModelFormat, ModelRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VisionChatOutputFormat {
    Text,
    Json,
    Md,
}

impl VisionChatOutputFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::Md => "md",
        }
    }

    pub const fn extension(self) -> &'static str {
        match self {
            Self::Text => "txt",
            Self::Json => "json",
            Self::Md => "md",
        }
    }

    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Text => "text/plain",
            Self::Json => "application/json",
            Self::Md => "text/markdown",
        }
    }
}

impl Default for VisionChatOutputFormat {
    fn default() -> Self {
        Self::Text
    }
}

impl fmt::Display for VisionChatOutputFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for VisionChatOutputFormat {
    type Err = VisionChatOutputFormatParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" => Err(VisionChatOutputFormatParseError::Empty),
            "text" | "txt" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            "md" | "markdown" => Ok(Self::Md),
            _ => Err(VisionChatOutputFormatParseError::Unsupported {
                value: value.trim().to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VisionChatOutputFormatParseError {
    #[error("vision chat output format must not be blank; expected one of: text, json, md")]
    Empty,
    #[error("unsupported vision chat output format `{value}`; expected one of: text, json, md")]
    Unsupported { value: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisionChatPrompt {
    pub prompt: String,
    pub system_prompt: Option<String>,
}

impl VisionChatPrompt {
    pub fn new(
        prompt: impl Into<String>,
        system_prompt: Option<String>,
    ) -> Result<Self, VisionChatPromptValidationError> {
        let prompt = prompt.into().trim().to_string();
        if prompt.is_empty() {
            return Err(VisionChatPromptValidationError::EmptyPrompt);
        }
        let system_prompt = system_prompt.and_then(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        });

        Ok(Self {
            prompt,
            system_prompt,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VisionChatPromptValidationError {
    #[error("vision chat prompt must not be empty")]
    EmptyPrompt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct VisionChatGenerationOptions {
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VisionChatBackend {
    TransformersImageTextToText,
}

impl VisionChatBackend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TransformersImageTextToText => "transformers-image-text-to-text",
        }
    }

    pub const fn from_model_format(format: ModelFormat) -> Option<Self> {
        match format {
            ModelFormat::Safetensors => Some(Self::TransformersImageTextToText),
            ModelFormat::Diffusers | ModelFormat::Gguf | ModelFormat::Mlx => None,
        }
    }
}

impl fmt::Display for VisionChatBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisionChatRuntimeTarget {
    LocalModel {
        model_ref: ModelRef,
        backend: VisionChatBackend,
        source_repo: Option<String>,
        source_revision: Option<String>,
        model_capabilities: Vec<ModelCapability>,
    },
}

impl VisionChatRuntimeTarget {
    pub fn model_label(&self) -> String {
        match self {
            Self::LocalModel { model_ref, .. } => model_ref.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedVisionChatTarget {
    pub runtime: VisionChatRuntimeTarget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisionChatRequest {
    pub target: ResolvedVisionChatTarget,
    pub image_path: PathBuf,
    pub image_media_type: Option<String>,
    pub prompt: VisionChatPrompt,
    pub output_format: VisionChatOutputFormat,
    pub options: VisionChatGenerationOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisionChatResponse {
    pub output_format: VisionChatOutputFormat,
    pub media_type: String,
    pub text: String,
    pub finish_reason: String,
}
