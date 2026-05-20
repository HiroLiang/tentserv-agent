//! Image generation use-case ports.

use std::{future::Future, path::PathBuf, pin::Pin};

use crate::features::image_generation::domain::{
    ImageGenerationOptions, ImageGenerationOutputFormat, ImageGenerationRequest,
    ImageGenerationResponse,
};
use crate::features::model::domain::{ModelInspection, ModelRefSelector};
use crate::features::runtime::domain::{PythonRuntimeLayout, PythonRuntimeResolutionInput};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

/// Boxed async return type used by image-generation use cases that execute runtime work.
pub type ImageGenerationUseCaseFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

/// Request for preparing one text-to-image generation request.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageGenerationPreparationRequest {
    pub layout: RuntimeLayoutInput,
    pub runtime: PythonRuntimeResolutionInput,
    pub model_selector: ModelRefSelector,
    pub prompt: String,
    pub negative_prompt: Option<String>,
    pub output_path: PathBuf,
    pub output_format: ImageGenerationOutputFormat,
    pub options: ImageGenerationOptions,
}

/// Result of resolving layout, runtime, model, and the runtime request.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageGenerationPreparationResult {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
    pub model: ModelInspection,
    pub request: ImageGenerationRequest,
}

/// Result of executing one prepared image-generation request.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageGenerationExecutionResult {
    pub prepared: ImageGenerationPreparationResult,
    pub response: ImageGenerationResponse,
}

/// Use-case boundary for preparing image-generation runtime requests.
pub trait ImageGenerationPreparationUseCase {
    /// Resolves the selected model target and builds the canonical runtime request.
    fn prepare_image_generation(
        &self,
        request: ImageGenerationPreparationRequest,
    ) -> KernelResult<ImageGenerationPreparationResult>;
}

/// Use-case boundary for one-shot text-to-image inference.
pub trait ImageGenerationUseCase {
    /// Resolves target/runtime and writes one generated image output file.
    fn generate_image(
        &'_ self,
        request: ImageGenerationPreparationRequest,
    ) -> ImageGenerationUseCaseFuture<'_, ImageGenerationExecutionResult>;
}
