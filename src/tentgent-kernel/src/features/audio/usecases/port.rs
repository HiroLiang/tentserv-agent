//! Audio use case ports.

use std::{future::Future, path::PathBuf, pin::Pin};

use crate::features::audio::domain::{
    AudioTranscriptionOutputFormat, AudioTranscriptionRequest, AudioTranscriptionResponse,
};
use crate::features::model::domain::{ModelInspection, ModelRefSelector};
use crate::features::runtime::domain::{PythonRuntimeLayout, PythonRuntimeResolutionInput};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

/// Boxed async return type used by audio use cases that execute runtime work.
pub type AudioUseCaseFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

/// Request for preparing one batch audio transcription request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioTranscriptionPreparationRequest {
    pub layout: RuntimeLayoutInput,
    pub runtime: PythonRuntimeResolutionInput,
    pub model_selector: ModelRefSelector,
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub output_format: AudioTranscriptionOutputFormat,
    pub language: Option<String>,
    pub timestamps: bool,
}

/// Result of resolving layout, runtime, model, and the runtime request.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioTranscriptionPreparationResult {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
    pub model: ModelInspection,
    pub request: AudioTranscriptionRequest,
}

/// Result of executing one prepared audio transcription request.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioTranscriptionExecutionResult {
    pub prepared: AudioTranscriptionPreparationResult,
    pub response: AudioTranscriptionResponse,
}

/// Use-case boundary for preparing audio transcription runtime requests.
pub trait AudioTranscriptionPreparationUseCase {
    /// Resolves the selected model target and builds the canonical runtime request.
    fn prepare_audio_transcription(
        &self,
        request: AudioTranscriptionPreparationRequest,
    ) -> KernelResult<AudioTranscriptionPreparationResult>;
}

/// Use-case boundary for batch audio transcription inference.
pub trait AudioTranscriptionUseCase {
    /// Resolves target/runtime and writes transcript output for the provided audio path.
    fn transcribe_audio(
        &'_ self,
        request: AudioTranscriptionPreparationRequest,
    ) -> AudioUseCaseFuture<'_, AudioTranscriptionExecutionResult>;
}
