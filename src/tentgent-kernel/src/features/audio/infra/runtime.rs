use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::features::runtime::infra::{ModelRuntimeCapability, ModelRuntimeDaemonSupervisor};
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{
    AudioSpeechOutputFormat, AudioSpeechRequest, AudioSpeechResponse, AudioSpeechRuntimeTarget,
    AudioTranscriptionOutputFormat, AudioTranscriptionRequest, AudioTranscriptionResponse,
    AudioTranscriptionRuntimeTarget,
};
use super::super::ports::{
    AudioPortFuture, AudioSpeechRuntimeClient, AudioSpeechRuntimeRequest,
    AudioTranscriptionRuntimeClient, AudioTranscriptionRuntimeRequest,
};

/// Executes prepared audio transcription requests through the model-runtime HTTP daemon.
pub struct PythonAudioTranscriptionModelRuntimeClient<'a> {
    executable_resolver: &'a dyn RuntimeExecutableResolver,
    supervisor: &'a ModelRuntimeDaemonSupervisor,
}

impl<'a> PythonAudioTranscriptionModelRuntimeClient<'a> {
    pub fn new(
        executable_resolver: &'a dyn RuntimeExecutableResolver,
        supervisor: &'a ModelRuntimeDaemonSupervisor,
    ) -> Self {
        Self {
            executable_resolver,
            supervisor,
        }
    }

    async fn transcribe_http(
        &self,
        request: AudioTranscriptionRuntimeRequest,
    ) -> KernelResult<AudioTranscriptionResponse> {
        let model_ref = local_transcription_model_ref(&request.request);
        let endpoint = self
            .supervisor
            .ensure_model_bound(
                &request.layout,
                &request.runtime,
                self.executable_resolver,
                ModelRuntimeCapability::AudioTranscription,
                model_ref,
            )
            .await?;
        let payload = AudioTranscriptionPayload {
            input_path: request.request.input_path.display().to_string(),
            output_path: request.request.output_path.display().to_string(),
            output_format: request.request.output_format,
            language: request.request.language,
            timestamps: request.request.timestamps,
        };
        let response: AudioTranscriptionResponsePayload = self
            .supervisor
            .post_json(
                &endpoint,
                "/v1/audio/transcriptions",
                &payload,
                audio_runtime_error,
            )
            .await?;
        Ok(AudioTranscriptionResponse {
            output_format: response.output_format,
            media_type: response.media_type,
            output_path: response.output_path,
            total_bytes: response.total_bytes,
            text: response.text,
        })
    }
}

impl AudioTranscriptionRuntimeClient for PythonAudioTranscriptionModelRuntimeClient<'_> {
    fn transcribe_audio(
        &'_ self,
        request: AudioTranscriptionRuntimeRequest,
    ) -> AudioPortFuture<'_, AudioTranscriptionResponse> {
        Box::pin(async move { self.transcribe_http(request).await })
    }
}

/// Executes prepared audio speech requests through the model-runtime HTTP daemon.
pub struct PythonAudioSpeechModelRuntimeClient<'a> {
    executable_resolver: &'a dyn RuntimeExecutableResolver,
    supervisor: &'a ModelRuntimeDaemonSupervisor,
}

impl<'a> PythonAudioSpeechModelRuntimeClient<'a> {
    pub fn new(
        executable_resolver: &'a dyn RuntimeExecutableResolver,
        supervisor: &'a ModelRuntimeDaemonSupervisor,
    ) -> Self {
        Self {
            executable_resolver,
            supervisor,
        }
    }

    async fn synthesize_http(
        &self,
        request: AudioSpeechRuntimeRequest,
    ) -> KernelResult<AudioSpeechResponse> {
        let model_ref = local_speech_model_ref(&request.request);
        let endpoint = self
            .supervisor
            .ensure_model_bound(
                &request.layout,
                &request.runtime,
                self.executable_resolver,
                ModelRuntimeCapability::AudioSpeech,
                model_ref,
            )
            .await?;
        let payload = AudioSpeechPayload {
            text: request.request.text,
            output_path: request.request.output_path.display().to_string(),
            output_format: request.request.output_format,
            language: request.request.language,
            voice: request.request.voice,
        };
        let response: AudioSpeechResponsePayload = self
            .supervisor
            .post_json(&endpoint, "/v1/audio/speech", &payload, audio_runtime_error)
            .await?;
        Ok(AudioSpeechResponse {
            output_format: response.output_format,
            media_type: response.media_type,
            output_path: response.output_path,
            total_bytes: response.total_bytes,
            sample_rate: response.sample_rate,
        })
    }
}

impl AudioSpeechRuntimeClient for PythonAudioSpeechModelRuntimeClient<'_> {
    fn synthesize_speech(
        &'_ self,
        request: AudioSpeechRuntimeRequest,
    ) -> AudioPortFuture<'_, AudioSpeechResponse> {
        Box::pin(async move { self.synthesize_http(request).await })
    }
}

#[derive(Debug, Serialize)]
struct AudioTranscriptionPayload {
    input_path: String,
    output_path: String,
    output_format: AudioTranscriptionOutputFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    timestamps: bool,
}

#[derive(Debug, Deserialize)]
struct AudioTranscriptionResponsePayload {
    output_format: AudioTranscriptionOutputFormat,
    media_type: String,
    output_path: PathBuf,
    total_bytes: u64,
    text: Option<String>,
}

#[derive(Debug, Serialize)]
struct AudioSpeechPayload {
    text: String,
    output_path: String,
    output_format: AudioSpeechOutputFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    voice: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AudioSpeechResponsePayload {
    output_format: AudioSpeechOutputFormat,
    media_type: String,
    output_path: PathBuf,
    total_bytes: u64,
    sample_rate: Option<u32>,
}

fn local_transcription_model_ref(request: &AudioTranscriptionRequest) -> &str {
    match &request.target.runtime {
        AudioTranscriptionRuntimeTarget::LocalModel { model_ref, .. } => model_ref.as_str(),
    }
}

fn local_speech_model_ref(request: &AudioSpeechRequest) -> &str {
    match &request.target.runtime {
        AudioSpeechRuntimeTarget::LocalModel { model_ref, .. } => model_ref.as_str(),
    }
}

fn audio_runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::AudioRuntimeUnavailable(message.into())
}
