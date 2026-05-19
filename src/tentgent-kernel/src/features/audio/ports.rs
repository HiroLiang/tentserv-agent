//! Audio feature package ports.

use std::{future::Future, pin::Pin};

use crate::features::model::domain::{ModelInspection, ModelRefSelector};
use crate::features::runtime::domain::PythonRuntimeLayout;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

use super::domain::{
    AudioTranscriptionRequest, AudioTranscriptionResponse, AudioTranscriptionRuntimeTarget,
};

pub type AudioPortFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioTranscriptionModelResolveRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: ModelRefSelector,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioTranscriptionModelResolveResult {
    pub layout: RuntimeLayout,
    pub model: ModelInspection,
    pub target: AudioTranscriptionRuntimeTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioTranscriptionRuntimeRequest {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
    pub request: AudioTranscriptionRequest,
}

/// Boundary for resolving a model selector into an audio-transcription target.
pub trait AudioTranscriptionModelResolver {
    /// Resolves a model ref or unique prefix and maps it to an ASR runtime target.
    fn resolve_audio_transcription_model(
        &self,
        request: AudioTranscriptionModelResolveRequest,
    ) -> KernelResult<AudioTranscriptionModelResolveResult>;
}

/// Boundary for executing a prepared batch audio transcription request.
pub trait AudioTranscriptionRuntimeClient {
    /// Writes transcript output to the prepared output path and returns metadata.
    fn transcribe_audio(
        &'_ self,
        request: AudioTranscriptionRuntimeRequest,
    ) -> AudioPortFuture<'_, AudioTranscriptionResponse>;
}
