//! Video-understanding use case ports.

use std::{future::Future, path::PathBuf, pin::Pin};

use crate::features::model::domain::{ModelInspection, ModelRefSelector};
use crate::features::runtime::domain::{PythonRuntimeLayout, PythonRuntimeResolutionInput};
use crate::features::video_understanding::domain::{
    VideoSamplingOptions, VideoUnderstandingGenerationOptions, VideoUnderstandingOutputFormat,
    VideoUnderstandingRequest, VideoUnderstandingResponse,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

pub type VideoUnderstandingUseCaseFuture<'a, T> =
    Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

#[derive(Debug, Clone, PartialEq)]
pub struct VideoUnderstandingPreparationRequest {
    pub layout: RuntimeLayoutInput,
    pub runtime: PythonRuntimeResolutionInput,
    pub model_selector: ModelRefSelector,
    pub video_path: PathBuf,
    pub video_media_type: Option<String>,
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub output_format: VideoUnderstandingOutputFormat,
    pub options: VideoUnderstandingGenerationOptions,
    pub sampling: VideoSamplingOptions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VideoUnderstandingPreparationResult {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
    pub model: ModelInspection,
    pub request: VideoUnderstandingRequest,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VideoUnderstandingExecutionResult {
    pub prepared: VideoUnderstandingPreparationResult,
    pub response: VideoUnderstandingResponse,
}

pub trait VideoUnderstandingPreparationUseCase {
    fn prepare_video_understanding(
        &self,
        request: VideoUnderstandingPreparationRequest,
    ) -> KernelResult<VideoUnderstandingPreparationResult>;
}

pub trait VideoUnderstandingUseCase {
    fn understand_video(
        &'_ self,
        request: VideoUnderstandingPreparationRequest,
    ) -> VideoUnderstandingUseCaseFuture<'_, VideoUnderstandingExecutionResult>;
}
