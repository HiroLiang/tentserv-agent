//! Image generation request, target, and response domain types.

use std::{fmt, path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::features::adapter::domain::{AdapterBackendSupport, AdapterRef, LoraScale};
use crate::features::model::domain::{MlxRuntimeFamily, ModelCapability, ModelFormat, ModelRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImageGenerationOutputFormat {
    Png,
    Jpeg,
}

impl ImageGenerationOutputFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }

    pub const fn extension(self) -> &'static str {
        self.as_str()
    }

    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }

    pub fn default_filename(self) -> String {
        format!("image.{}", self.extension())
    }
}

impl Default for ImageGenerationOutputFormat {
    fn default() -> Self {
        Self::Png
    }
}

impl fmt::Display for ImageGenerationOutputFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ImageGenerationOutputFormat {
    type Err = ImageGenerationOutputFormatParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" => Err(ImageGenerationOutputFormatParseError::Empty),
            "png" => Ok(Self::Png),
            "jpg" | "jpeg" => Ok(Self::Jpeg),
            _ => Err(ImageGenerationOutputFormatParseError::Unsupported {
                value: value.trim().to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImageGenerationOutputFormatParseError {
    #[error("image generation output format must not be blank; expected one of: png, jpg")]
    Empty,
    #[error("unsupported image generation output format `{value}`; expected one of: png, jpg")]
    Unsupported { value: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageGenerationPrompt {
    pub prompt: String,
    pub negative_prompt: Option<String>,
}

impl ImageGenerationPrompt {
    pub const MAX_PROMPT_BYTES: usize = 8 * 1024;

    pub fn new(
        prompt: impl Into<String>,
        negative_prompt: Option<String>,
    ) -> Result<Self, ImageGenerationPromptValidationError> {
        let prompt = prompt.into().trim().to_string();
        if prompt.is_empty() {
            return Err(ImageGenerationPromptValidationError::EmptyPrompt);
        }
        if prompt.len() > Self::MAX_PROMPT_BYTES {
            return Err(ImageGenerationPromptValidationError::PromptTooLarge {
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
                    ImageGenerationPromptValidationError::NegativePromptTooLarge {
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
pub enum ImageGenerationPromptValidationError {
    #[error("image generation prompt must not be empty")]
    EmptyPrompt,
    #[error("image generation prompt must be at most {max_bytes} bytes")]
    PromptTooLarge { max_bytes: usize },
    #[error("image generation negative prompt must be at most {max_bytes} bytes")]
    NegativePromptTooLarge { max_bytes: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageGenerationDimensions {
    pub width: u32,
    pub height: u32,
}

impl ImageGenerationDimensions {
    pub const DEFAULT_WIDTH: u32 = 512;
    pub const DEFAULT_HEIGHT: u32 = 512;
    pub const MIN_SIDE: u32 = 64;
    pub const MAX_SIDE: u32 = 1024;

    pub fn new(width: u32, height: u32) -> Result<Self, ImageGenerationDimensionsError> {
        validate_side("width", width)?;
        validate_side("height", height)?;
        Ok(Self { width, height })
    }
}

impl Default for ImageGenerationDimensions {
    fn default() -> Self {
        Self {
            width: Self::DEFAULT_WIDTH,
            height: Self::DEFAULT_HEIGHT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImageGenerationDimensionsError {
    #[error("image generation {axis} must be between 64 and 1024 pixels; got {value}")]
    OutOfRange { axis: &'static str, value: u32 },
    #[error("image generation {axis} must be divisible by 8 pixels; got {value}")]
    NotDivisibleByEight { axis: &'static str, value: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageGenerationOptions {
    pub dimensions: ImageGenerationDimensions,
    pub steps: u32,
    pub guidance_scale: f32,
    pub seed: Option<u64>,
}

impl ImageGenerationOptions {
    pub const DEFAULT_STEPS: u32 = 20;
    pub const DEFAULT_GUIDANCE_SCALE: f32 = 7.5;
    pub const MIN_STEPS: u32 = 1;
    pub const MAX_STEPS: u32 = 100;
    pub const MIN_GUIDANCE_SCALE: f32 = 0.0;
    pub const MAX_GUIDANCE_SCALE: f32 = 30.0;

    pub fn new(
        dimensions: ImageGenerationDimensions,
        steps: u32,
        guidance_scale: f32,
        seed: Option<u64>,
    ) -> Result<Self, ImageGenerationOptionsError> {
        if !(Self::MIN_STEPS..=Self::MAX_STEPS).contains(&steps) {
            return Err(ImageGenerationOptionsError::StepsOutOfRange { steps });
        }
        if !guidance_scale.is_finite()
            || !(Self::MIN_GUIDANCE_SCALE..=Self::MAX_GUIDANCE_SCALE).contains(&guidance_scale)
        {
            return Err(ImageGenerationOptionsError::GuidanceScaleOutOfRange { guidance_scale });
        }

        Ok(Self {
            dimensions,
            steps,
            guidance_scale,
            seed,
        })
    }
}

impl Default for ImageGenerationOptions {
    fn default() -> Self {
        Self {
            dimensions: ImageGenerationDimensions::default(),
            steps: Self::DEFAULT_STEPS,
            guidance_scale: Self::DEFAULT_GUIDANCE_SCALE,
            seed: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ImageGenerationOptionsError {
    #[error("image generation steps must be between 1 and 100; got {steps}")]
    StepsOutOfRange { steps: u32 },
    #[error("image generation guidance scale must be between 0 and 30; got {guidance_scale}")]
    GuidanceScaleOutOfRange { guidance_scale: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImageGenerationWorkflowKind {
    TextToImage,
    ImageToImage,
    Inpaint,
}

impl ImageGenerationWorkflowKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TextToImage => "text-to-image",
            Self::ImageToImage => "image-to-image",
            Self::Inpaint => "inpaint",
        }
    }
}

impl fmt::Display for ImageGenerationWorkflowKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ImageTransformStrength(f32);

impl ImageTransformStrength {
    pub const DEFAULT: f32 = 0.6;
    pub const MIN: f32 = 0.0;
    pub const MAX: f32 = 1.0;

    pub fn new(value: f32) -> Result<Self, ImageTransformStrengthError> {
        if !value.is_finite() || !(Self::MIN..=Self::MAX).contains(&value) {
            return Err(ImageTransformStrengthError::OutOfRange { strength: value });
        }
        Ok(Self(value))
    }

    pub const fn as_f32(self) -> f32 {
        self.0
    }
}

impl Default for ImageTransformStrength {
    fn default() -> Self {
        Self(Self::DEFAULT)
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ImageTransformStrengthError {
    #[error("image denoising strength must be between 0 and 1; got {strength}")]
    OutOfRange { strength: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImageGenerationBackend {
    DiffusersTextToImage,
    DiffusersImageToImage,
    DiffusersInpaint,
    MlxDiffusionTextToImage,
    MlxDiffusionImageToImage,
    MlxDiffusionInpaint,
}

impl ImageGenerationBackend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DiffusersTextToImage => "diffusers-text-to-image",
            Self::DiffusersImageToImage => "diffusers-image-to-image",
            Self::DiffusersInpaint => "diffusers-inpaint",
            Self::MlxDiffusionTextToImage => "mlx-diffusion-text-to-image",
            Self::MlxDiffusionImageToImage => "mlx-diffusion-image-to-image",
            Self::MlxDiffusionInpaint => "mlx-diffusion-inpaint",
        }
    }

    pub const fn from_model_format(format: ModelFormat) -> Option<Self> {
        match format {
            ModelFormat::Diffusers => Some(Self::DiffusersTextToImage),
            ModelFormat::Safetensors | ModelFormat::Gguf | ModelFormat::Mlx => None,
        }
    }

    pub const fn from_model_format_and_mlx_family(
        format: ModelFormat,
        mlx_runtime_family: Option<MlxRuntimeFamily>,
    ) -> Option<Self> {
        Self::from_model_format_and_mlx_family_for_workflow(
            format,
            mlx_runtime_family,
            ImageGenerationWorkflowKind::TextToImage,
        )
    }

    pub const fn from_model_format_and_mlx_family_for_workflow(
        format: ModelFormat,
        mlx_runtime_family: Option<MlxRuntimeFamily>,
        workflow: ImageGenerationWorkflowKind,
    ) -> Option<Self> {
        match (format, mlx_runtime_family) {
            (ModelFormat::Diffusers, _) => match workflow {
                ImageGenerationWorkflowKind::TextToImage => Some(Self::DiffusersTextToImage),
                ImageGenerationWorkflowKind::ImageToImage => Some(Self::DiffusersImageToImage),
                ImageGenerationWorkflowKind::Inpaint => Some(Self::DiffusersInpaint),
            },
            (ModelFormat::Mlx, Some(MlxRuntimeFamily::Diffusion)) => match workflow {
                ImageGenerationWorkflowKind::TextToImage => Some(Self::MlxDiffusionTextToImage),
                ImageGenerationWorkflowKind::ImageToImage => Some(Self::MlxDiffusionImageToImage),
                ImageGenerationWorkflowKind::Inpaint => Some(Self::MlxDiffusionInpaint),
            },
            (ModelFormat::Safetensors | ModelFormat::Gguf, _)
            | (
                ModelFormat::Mlx,
                Some(MlxRuntimeFamily::Lm | MlxRuntimeFamily::Vlm | MlxRuntimeFamily::Audio),
            )
            | (ModelFormat::Mlx, None) => None,
        }
    }

    pub const fn adapter_backend_support(self) -> AdapterBackendSupport {
        match self {
            Self::DiffusersTextToImage | Self::DiffusersImageToImage | Self::DiffusersInpaint => {
                AdapterBackendSupport::Diffusers
            }
            Self::MlxDiffusionTextToImage
            | Self::MlxDiffusionImageToImage
            | Self::MlxDiffusionInpaint => AdapterBackendSupport::MlxDiffusion,
        }
    }
}

impl fmt::Display for ImageGenerationBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageGenerationRuntimeTarget {
    LocalModel {
        model_ref: ModelRef,
        backend: ImageGenerationBackend,
        source_repo: Option<String>,
        source_revision: Option<String>,
        model_capabilities: Vec<ModelCapability>,
    },
}

impl ImageGenerationRuntimeTarget {
    pub fn model_label(&self) -> String {
        match self {
            Self::LocalModel { model_ref, .. } => model_ref.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedImageGenerationTarget {
    pub runtime: ImageGenerationRuntimeTarget,
    pub adapter: Option<ResolvedImageGenerationAdapter>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedImageGenerationAdapter {
    pub adapter_ref: AdapterRef,
    pub backend: AdapterBackendSupport,
    pub source_path: PathBuf,
    pub weight_file: Option<String>,
    pub scale: LoraScale,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ImageGenerationInput {
    TextToImage,
    ImageToImage {
        image_path: PathBuf,
        media_type: Option<String>,
        strength: ImageTransformStrength,
    },
    Inpaint {
        image_path: PathBuf,
        image_media_type: Option<String>,
        mask_path: PathBuf,
        mask_media_type: Option<String>,
        strength: ImageTransformStrength,
    },
}

impl ImageGenerationInput {
    pub const fn workflow_kind(&self) -> ImageGenerationWorkflowKind {
        match self {
            Self::TextToImage => ImageGenerationWorkflowKind::TextToImage,
            Self::ImageToImage { .. } => ImageGenerationWorkflowKind::ImageToImage,
            Self::Inpaint { .. } => ImageGenerationWorkflowKind::Inpaint,
        }
    }
}

impl Default for ImageGenerationInput {
    fn default() -> Self {
        Self::TextToImage
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageGenerationRequest {
    pub target: ResolvedImageGenerationTarget,
    pub input: ImageGenerationInput,
    pub prompt: ImageGenerationPrompt,
    pub output_path: PathBuf,
    pub output_format: ImageGenerationOutputFormat,
    pub options: ImageGenerationOptions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageGenerationResponse {
    pub output_format: ImageGenerationOutputFormat,
    pub media_type: String,
    pub output_path: PathBuf,
    pub total_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub seed: Option<u64>,
}

fn validate_side(axis: &'static str, value: u32) -> Result<(), ImageGenerationDimensionsError> {
    if !(ImageGenerationDimensions::MIN_SIDE..=ImageGenerationDimensions::MAX_SIDE).contains(&value)
    {
        return Err(ImageGenerationDimensionsError::OutOfRange { axis, value });
    }
    if value % 8 != 0 {
        return Err(ImageGenerationDimensionsError::NotDivisibleByEight { axis, value });
    }
    Ok(())
}
