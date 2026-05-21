//! Video-generation artifact contract domain types.

use std::{fmt, path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VideoGenerationOutputFormat {
    Mp4,
    Webm,
}

impl VideoGenerationOutputFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Webm => "webm",
        }
    }

    pub const fn extension(self) -> &'static str {
        self.as_str()
    }

    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Mp4 => "video/mp4",
            Self::Webm => "video/webm",
        }
    }

    pub fn default_filename(self) -> String {
        format!("video.{}", self.extension())
    }
}

impl Default for VideoGenerationOutputFormat {
    fn default() -> Self {
        Self::Mp4
    }
}

impl fmt::Display for VideoGenerationOutputFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for VideoGenerationOutputFormat {
    type Err = VideoGenerationOutputFormatParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" => Err(VideoGenerationOutputFormatParseError::Empty),
            "mp4" => Ok(Self::Mp4),
            "webm" => Ok(Self::Webm),
            _ => Err(VideoGenerationOutputFormatParseError::Unsupported {
                value: value.trim().to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VideoGenerationOutputFormatParseError {
    #[error("video generation output format must not be blank; expected one of: mp4, webm")]
    Empty,
    #[error("unsupported video generation output format `{value}`; expected one of: mp4, webm")]
    Unsupported { value: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoGenerationPrompt {
    pub prompt: String,
    pub negative_prompt: Option<String>,
}

impl VideoGenerationPrompt {
    pub const MAX_PROMPT_BYTES: usize = 8 * 1024;

    pub fn new(
        prompt: impl Into<String>,
        negative_prompt: Option<String>,
    ) -> Result<Self, VideoGenerationPromptValidationError> {
        let prompt = prompt.into().trim().to_string();
        if prompt.is_empty() {
            return Err(VideoGenerationPromptValidationError::EmptyPrompt);
        }
        if prompt.len() > Self::MAX_PROMPT_BYTES {
            return Err(VideoGenerationPromptValidationError::PromptTooLarge {
                max_bytes: Self::MAX_PROMPT_BYTES,
            });
        }

        let negative_prompt = negative_prompt.and_then(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        });
        if let Some(negative_prompt) = &negative_prompt {
            if negative_prompt.len() > Self::MAX_PROMPT_BYTES {
                return Err(
                    VideoGenerationPromptValidationError::NegativePromptTooLarge {
                        max_bytes: Self::MAX_PROMPT_BYTES,
                    },
                );
            }
        }

        Ok(Self {
            prompt,
            negative_prompt,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VideoGenerationPromptValidationError {
    #[error("video generation prompt must not be empty")]
    EmptyPrompt,
    #[error("video generation prompt must be at most {max_bytes} bytes")]
    PromptTooLarge { max_bytes: usize },
    #[error("video generation negative prompt must be at most {max_bytes} bytes")]
    NegativePromptTooLarge { max_bytes: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoGenerationDimensions {
    pub width: u32,
    pub height: u32,
}

impl VideoGenerationDimensions {
    pub const DEFAULT_WIDTH: u32 = 256;
    pub const DEFAULT_HEIGHT: u32 = 256;
    pub const MIN_SIDE: u32 = 64;
    pub const MAX_SIDE: u32 = 1024;

    pub fn new(width: u32, height: u32) -> Result<Self, VideoGenerationDimensionsError> {
        validate_side("width", width)?;
        validate_side("height", height)?;
        Ok(Self { width, height })
    }
}

impl Default for VideoGenerationDimensions {
    fn default() -> Self {
        Self {
            width: Self::DEFAULT_WIDTH,
            height: Self::DEFAULT_HEIGHT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VideoGenerationDimensionsError {
    #[error("video generation {axis} must be between 64 and 1024 pixels; got {value}")]
    OutOfRange { axis: &'static str, value: u32 },
    #[error("video generation {axis} must be divisible by 8 pixels; got {value}")]
    NotDivisibleByEight { axis: &'static str, value: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoGenerationOptions {
    pub dimensions: VideoGenerationDimensions,
    pub duration_seconds: f32,
    pub fps: u32,
    pub num_frames: Option<u32>,
    pub steps: u32,
    pub guidance_scale: f32,
    pub seed: Option<u64>,
}

impl VideoGenerationOptions {
    pub const DEFAULT_DURATION_SECONDS: f32 = 2.0;
    pub const MIN_DURATION_SECONDS: f32 = 0.25;
    pub const MAX_DURATION_SECONDS: f32 = 4.0;
    pub const DEFAULT_FPS: u32 = 8;
    pub const MIN_FPS: u32 = 1;
    pub const MAX_FPS: u32 = 12;
    pub const MAX_FRAMES: u32 = 120;
    pub const DEFAULT_STEPS: u32 = 12;
    pub const MIN_STEPS: u32 = 1;
    pub const MAX_STEPS: u32 = 60;
    pub const DEFAULT_GUIDANCE_SCALE: f32 = 5.0;
    pub const MIN_GUIDANCE_SCALE: f32 = 0.0;
    pub const MAX_GUIDANCE_SCALE: f32 = 30.0;

    pub fn new(
        dimensions: VideoGenerationDimensions,
        duration_seconds: f32,
        fps: u32,
        num_frames: Option<u32>,
        steps: u32,
        guidance_scale: f32,
        seed: Option<u64>,
    ) -> Result<Self, VideoGenerationOptionsError> {
        if !duration_seconds.is_finite()
            || !(Self::MIN_DURATION_SECONDS..=Self::MAX_DURATION_SECONDS)
                .contains(&duration_seconds)
        {
            return Err(VideoGenerationOptionsError::DurationOutOfRange { duration_seconds });
        }
        if !(Self::MIN_FPS..=Self::MAX_FPS).contains(&fps) {
            return Err(VideoGenerationOptionsError::FpsOutOfRange { fps });
        }
        if let Some(num_frames) = num_frames {
            if !(1..=Self::MAX_FRAMES).contains(&num_frames) {
                return Err(VideoGenerationOptionsError::FrameCountOutOfRange { num_frames });
            }
        }
        if !(Self::MIN_STEPS..=Self::MAX_STEPS).contains(&steps) {
            return Err(VideoGenerationOptionsError::StepsOutOfRange { steps });
        }
        if !guidance_scale.is_finite()
            || !(Self::MIN_GUIDANCE_SCALE..=Self::MAX_GUIDANCE_SCALE).contains(&guidance_scale)
        {
            return Err(VideoGenerationOptionsError::GuidanceScaleOutOfRange { guidance_scale });
        }
        let planned_frames =
            num_frames.unwrap_or_else(|| (duration_seconds * fps as f32).ceil() as u32);
        if planned_frames > Self::MAX_FRAMES {
            return Err(VideoGenerationOptionsError::FrameCountOutOfRange {
                num_frames: planned_frames,
            });
        }

        Ok(Self {
            dimensions,
            duration_seconds,
            fps,
            num_frames,
            steps,
            guidance_scale,
            seed,
        })
    }

    pub fn planned_frames(&self) -> u32 {
        self.num_frames
            .unwrap_or_else(|| (self.duration_seconds * self.fps as f32).ceil() as u32)
    }
}

impl Default for VideoGenerationOptions {
    fn default() -> Self {
        Self {
            dimensions: VideoGenerationDimensions::default(),
            duration_seconds: Self::DEFAULT_DURATION_SECONDS,
            fps: Self::DEFAULT_FPS,
            num_frames: None,
            steps: Self::DEFAULT_STEPS,
            guidance_scale: Self::DEFAULT_GUIDANCE_SCALE,
            seed: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum VideoGenerationOptionsError {
    #[error(
        "video generation duration must be between 0.25 and 4 seconds; got {duration_seconds}"
    )]
    DurationOutOfRange { duration_seconds: f32 },
    #[error("video generation fps must be between 1 and 12; got {fps}")]
    FpsOutOfRange { fps: u32 },
    #[error("video generation frame count must be between 1 and 120; got {num_frames}")]
    FrameCountOutOfRange { num_frames: u32 },
    #[error("video generation steps must be between 1 and 60; got {steps}")]
    StepsOutOfRange { steps: u32 },
    #[error("video generation guidance scale must be between 0 and 30; got {guidance_scale}")]
    GuidanceScaleOutOfRange { guidance_scale: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VideoGenerationWorkflowKind {
    TextToVideo,
}

impl VideoGenerationWorkflowKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TextToVideo => "text-to-video",
        }
    }
}

impl fmt::Display for VideoGenerationWorkflowKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum VideoGenerationInput {
    TextToVideo,
}

impl VideoGenerationInput {
    pub const fn workflow_kind(&self) -> VideoGenerationWorkflowKind {
        match self {
            Self::TextToVideo => VideoGenerationWorkflowKind::TextToVideo,
        }
    }
}

impl Default for VideoGenerationInput {
    fn default() -> Self {
        Self::TextToVideo
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoGenerationArtifactPlan {
    pub input: VideoGenerationInput,
    pub prompt: VideoGenerationPrompt,
    pub output_path: PathBuf,
    pub output_format: VideoGenerationOutputFormat,
    pub options: VideoGenerationOptions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoGenerationArtifact {
    pub output_format: VideoGenerationOutputFormat,
    pub media_type: String,
    pub output_path: PathBuf,
    pub total_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub frame_count: u32,
    pub duration_seconds: f32,
    pub seed: Option<u64>,
}

fn validate_side(axis: &'static str, value: u32) -> Result<(), VideoGenerationDimensionsError> {
    if !(VideoGenerationDimensions::MIN_SIDE..=VideoGenerationDimensions::MAX_SIDE).contains(&value)
    {
        return Err(VideoGenerationDimensionsError::OutOfRange { axis, value });
    }
    if value % 8 != 0 {
        return Err(VideoGenerationDimensionsError::NotDivisibleByEight { axis, value });
    }
    Ok(())
}
