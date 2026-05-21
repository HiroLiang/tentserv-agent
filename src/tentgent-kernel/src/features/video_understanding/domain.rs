//! Video-understanding request, target, and response domain types.

use std::{fmt, path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::features::model::domain::{MlxRuntimeFamily, ModelCapability, ModelFormat, ModelRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VideoUnderstandingOutputFormat {
    Text,
    Json,
    Md,
}

impl VideoUnderstandingOutputFormat {
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

    pub fn default_filename(self) -> String {
        format!("video-understanding.{}", self.extension())
    }
}

impl Default for VideoUnderstandingOutputFormat {
    fn default() -> Self {
        Self::Text
    }
}

impl fmt::Display for VideoUnderstandingOutputFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for VideoUnderstandingOutputFormat {
    type Err = VideoUnderstandingOutputFormatParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" => Err(VideoUnderstandingOutputFormatParseError::Empty),
            "text" | "txt" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            "md" | "markdown" => Ok(Self::Md),
            _ => Err(VideoUnderstandingOutputFormatParseError::Unsupported {
                value: value.trim().to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VideoUnderstandingOutputFormatParseError {
    #[error(
        "video understanding output format must not be blank; expected one of: text, json, md"
    )]
    Empty,
    #[error(
        "unsupported video understanding output format `{value}`; expected one of: text, json, md"
    )]
    Unsupported { value: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoUnderstandingPrompt {
    pub prompt: String,
    pub system_prompt: Option<String>,
}

impl VideoUnderstandingPrompt {
    pub fn new(
        prompt: impl Into<String>,
        system_prompt: Option<String>,
    ) -> Result<Self, VideoUnderstandingPromptValidationError> {
        let prompt = prompt.into().trim().to_string();
        if prompt.is_empty() {
            return Err(VideoUnderstandingPromptValidationError::EmptyPrompt);
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
pub enum VideoUnderstandingPromptValidationError {
    #[error("video understanding prompt must not be empty")]
    EmptyPrompt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct VideoUnderstandingGenerationOptions {
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VideoSamplingOptions {
    pub sample_fps: Option<f32>,
    pub max_frames: Option<u32>,
    pub max_frame_edge: Option<u32>,
    pub clip_start_seconds: Option<f32>,
    pub clip_duration_seconds: Option<f32>,
}

impl Default for VideoSamplingOptions {
    fn default() -> Self {
        Self {
            sample_fps: Some(Self::DEFAULT_SAMPLE_FPS),
            max_frames: Some(Self::DEFAULT_MAX_FRAMES),
            max_frame_edge: Some(Self::DEFAULT_MAX_FRAME_EDGE),
            clip_start_seconds: None,
            clip_duration_seconds: None,
        }
    }
}

impl VideoSamplingOptions {
    pub const DEFAULT_SAMPLE_FPS: f32 = 1.0;
    pub const DEFAULT_MAX_FRAMES: u32 = 32;
    pub const DEFAULT_MAX_FRAME_EDGE: u32 = 768;

    pub fn validate(&self) -> Result<(), VideoSamplingOptionsValidationError> {
        if let Some(sample_fps) = self.sample_fps {
            if !(0.1..=4.0).contains(&sample_fps) {
                return Err(VideoSamplingOptionsValidationError::SampleFps(sample_fps));
            }
        }
        if let Some(max_frames) = self.max_frames {
            if !(1..=128).contains(&max_frames) {
                return Err(VideoSamplingOptionsValidationError::MaxFrames(max_frames));
            }
        }
        if let Some(max_frame_edge) = self.max_frame_edge {
            if !(128..=1536).contains(&max_frame_edge) {
                return Err(VideoSamplingOptionsValidationError::MaxFrameEdge(
                    max_frame_edge,
                ));
            }
        }
        if let Some(clip_start_seconds) = self.clip_start_seconds {
            if clip_start_seconds < 0.0 {
                return Err(VideoSamplingOptionsValidationError::ClipStartSeconds(
                    clip_start_seconds,
                ));
            }
        }
        if let Some(clip_duration_seconds) = self.clip_duration_seconds {
            if clip_duration_seconds <= 0.0 {
                return Err(VideoSamplingOptionsValidationError::ClipDurationSeconds(
                    clip_duration_seconds,
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum VideoSamplingOptionsValidationError {
    #[error("`sample_fps` must be between 0.1 and 4.0; got {0}")]
    SampleFps(f32),
    #[error("`max_frames` must be between 1 and 128; got {0}")]
    MaxFrames(u32),
    #[error("`max_frame_edge` must be between 128 and 1536; got {0}")]
    MaxFrameEdge(u32),
    #[error("`clip_start_seconds` must be greater than or equal to 0; got {0}")]
    ClipStartSeconds(f32),
    #[error("`clip_duration_seconds` must be greater than 0; got {0}")]
    ClipDurationSeconds(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VideoUnderstandingBackend {
    TransformersVideoUnderstanding,
    MlxVlm,
}

impl VideoUnderstandingBackend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TransformersVideoUnderstanding => "transformers-video-understanding",
            Self::MlxVlm => "mlx-vlm",
        }
    }

    pub const fn from_model_format(format: ModelFormat) -> Option<Self> {
        match format {
            ModelFormat::Safetensors => Some(Self::TransformersVideoUnderstanding),
            ModelFormat::Diffusers | ModelFormat::Gguf | ModelFormat::Mlx => None,
        }
    }

    pub const fn from_model_format_and_mlx_family(
        format: ModelFormat,
        mlx_runtime_family: Option<MlxRuntimeFamily>,
    ) -> Option<Self> {
        match format {
            ModelFormat::Mlx => match mlx_runtime_family {
                Some(MlxRuntimeFamily::Vlm) => Some(Self::MlxVlm),
                None
                | Some(
                    MlxRuntimeFamily::Lm | MlxRuntimeFamily::Audio | MlxRuntimeFamily::Diffusion,
                ) => None,
            },
            _ => Self::from_model_format(format),
        }
    }
}

impl fmt::Display for VideoUnderstandingBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoUnderstandingRuntimeTarget {
    LocalModel {
        model_ref: ModelRef,
        backend: VideoUnderstandingBackend,
        source_repo: Option<String>,
        source_revision: Option<String>,
        model_capabilities: Vec<ModelCapability>,
    },
}

impl VideoUnderstandingRuntimeTarget {
    pub fn model_label(&self) -> String {
        match self {
            Self::LocalModel { model_ref, .. } => model_ref.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedVideoUnderstandingTarget {
    pub runtime: VideoUnderstandingRuntimeTarget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoUnderstandingRequest {
    pub target: ResolvedVideoUnderstandingTarget,
    pub video_path: PathBuf,
    pub video_media_type: Option<String>,
    pub prompt: VideoUnderstandingPrompt,
    pub output_format: VideoUnderstandingOutputFormat,
    pub options: VideoUnderstandingGenerationOptions,
    pub sampling: VideoSamplingOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoUnderstandingResponse {
    pub output_format: VideoUnderstandingOutputFormat,
    pub media_type: String,
    pub text: String,
    pub finish_reason: String,
    pub sampled_frames: Option<u32>,
}
