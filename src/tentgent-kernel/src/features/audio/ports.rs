//! Audio feature package ports.

use std::{future::Future, pin::Pin};

use crate::features::model::domain::{ModelInspection, ModelRefSelector};
use crate::features::runtime::domain::PythonRuntimeLayout;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

use super::domain::{
    AudioSpeechRequest, AudioSpeechResponse, AudioSpeechRuntimeTarget, AudioTranscriptionRequest,
    AudioTranscriptionResponse, AudioTranscriptionRuntimeTarget,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSpeechModelResolveRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: ModelRefSelector,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSpeechModelResolveResult {
    pub layout: RuntimeLayout,
    pub model: ModelInspection,
    pub target: AudioSpeechRuntimeTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSpeechRuntimeRequest {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
    pub request: AudioSpeechRequest,
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

/// Boundary for resolving a model selector into an audio-speech target.
pub trait AudioSpeechModelResolver {
    /// Resolves a model ref or unique prefix and maps it to a TTS runtime target.
    fn resolve_audio_speech_model(
        &self,
        request: AudioSpeechModelResolveRequest,
    ) -> KernelResult<AudioSpeechModelResolveResult>;
}

/// Boundary for executing a prepared audio speech request.
pub trait AudioSpeechRuntimeClient {
    /// Writes speech audio output to the prepared output path and returns metadata.
    fn synthesize_speech(
        &'_ self,
        request: AudioSpeechRuntimeRequest,
    ) -> AudioPortFuture<'_, AudioSpeechResponse>;
}
