//! Vision use case ports.

use std::{future::Future, path::PathBuf, pin::Pin};

use crate::features::model::domain::{ModelInspection, ModelRefSelector};
use crate::features::runtime::domain::{PythonRuntimeLayout, PythonRuntimeResolutionInput};
use crate::features::vision::domain::{
    VisionChatGenerationOptions, VisionChatOutputFormat, VisionChatRequest, VisionChatResponse,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

/// Boxed async return type used by vision use cases that execute runtime work.
pub type VisionUseCaseFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

/// Request for preparing one vision-chat request.
#[derive(Debug, Clone, PartialEq)]
pub struct VisionChatPreparationRequest {
    pub layout: RuntimeLayoutInput,
    pub runtime: PythonRuntimeResolutionInput,
    pub model_selector: ModelRefSelector,
    pub image_path: PathBuf,
    pub image_media_type: Option<String>,
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub output_format: VisionChatOutputFormat,
    pub options: VisionChatGenerationOptions,
}

/// Result of resolving layout, runtime, model, and the runtime request.
#[derive(Debug, Clone, PartialEq)]
pub struct VisionChatPreparationResult {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
    pub model: ModelInspection,
    pub request: VisionChatRequest,
}

/// Result of executing one prepared vision-chat request.
#[derive(Debug, Clone, PartialEq)]
pub struct VisionChatExecutionResult {
    pub prepared: VisionChatPreparationResult,
    pub response: VisionChatResponse,
}

/// Use-case boundary for preparing vision-chat runtime requests.
pub trait VisionChatPreparationUseCase {
    /// Resolves the selected model target and builds the canonical runtime request.
    fn prepare_vision_chat(
        &self,
        request: VisionChatPreparationRequest,
    ) -> KernelResult<VisionChatPreparationResult>;
}

/// Use-case boundary for one-shot vision-chat inference.
pub trait VisionChatUseCase {
    /// Resolves target/runtime and returns the complete generated response.
    fn generate_vision_chat(
        &'_ self,
        request: VisionChatPreparationRequest,
    ) -> VisionUseCaseFuture<'_, VisionChatExecutionResult>;
}
