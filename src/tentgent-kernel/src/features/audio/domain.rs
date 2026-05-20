//! Audio transcription request, target, and response domain types.

use std::{fmt, path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::features::model::domain::{ModelCapability, ModelFormat, ModelRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AudioTranscriptionOutputFormat {
    Text,
    Json,
    Vtt,
    Srt,
}

impl AudioTranscriptionOutputFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::Vtt => "vtt",
            Self::Srt => "srt",
        }
    }

    pub const fn extension(self) -> &'static str {
        match self {
            Self::Text => "txt",
            Self::Json => "json",
            Self::Vtt => "vtt",
            Self::Srt => "srt",
        }
    }

    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Text => "text/plain",
            Self::Json => "application/json",
            Self::Vtt => "text/vtt",
            Self::Srt => "application/x-subrip",
        }
    }

    pub fn default_filename(self) -> String {
        format!("transcript.{}", self.extension())
    }
}

impl fmt::Display for AudioTranscriptionOutputFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for AudioTranscriptionOutputFormat {
    type Err = AudioTranscriptionOutputFormatParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" => Err(AudioTranscriptionOutputFormatParseError::Empty),
            "text" | "txt" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            "vtt" => Ok(Self::Vtt),
            "srt" => Ok(Self::Srt),
            _ => Err(AudioTranscriptionOutputFormatParseError::Unsupported {
                value: value.trim().to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AudioTranscriptionOutputFormatParseError {
    #[error("audio transcription output format must not be blank; expected one of: text, json, vtt, srt")]
    Empty,
    #[error("unsupported audio transcription output format `{value}`; expected one of: text, json, vtt, srt")]
    Unsupported { value: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AudioTranscriptionBackend {
    TransformersAutomaticSpeechRecognition,
}

impl AudioTranscriptionBackend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TransformersAutomaticSpeechRecognition => "transformers-asr",
        }
    }

    pub const fn from_model_format(format: ModelFormat) -> Option<Self> {
        match format {
            ModelFormat::Safetensors => Some(Self::TransformersAutomaticSpeechRecognition),
            ModelFormat::Diffusers | ModelFormat::Gguf | ModelFormat::Mlx => None,
        }
    }
}

impl fmt::Display for AudioTranscriptionBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioTranscriptionRuntimeTarget {
    LocalModel {
        model_ref: ModelRef,
        backend: AudioTranscriptionBackend,
        source_repo: Option<String>,
        source_revision: Option<String>,
        model_capabilities: Vec<ModelCapability>,
    },
}

impl AudioTranscriptionRuntimeTarget {
    pub fn model_label(&self) -> String {
        match self {
            Self::LocalModel { model_ref, .. } => model_ref.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedAudioTranscriptionTarget {
    pub runtime: AudioTranscriptionRuntimeTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioTranscriptionRequest {
    pub target: ResolvedAudioTranscriptionTarget,
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub output_format: AudioTranscriptionOutputFormat,
    pub language: Option<String>,
    pub timestamps: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioTranscriptionResponse {
    pub output_format: AudioTranscriptionOutputFormat,
    pub media_type: String,
    pub output_path: PathBuf,
    pub total_bytes: u64,
    pub text: Option<String>,
}
