//! Standard audio transcription use case orchestration.

use crate::features::audio::domain::{AudioTranscriptionRequest, ResolvedAudioTranscriptionTarget};
use crate::features::audio::ports::{
    AudioTranscriptionModelResolveRequest, AudioTranscriptionModelResolver,
    AudioTranscriptionRuntimeClient, AudioTranscriptionRuntimeRequest,
};
use crate::features::runtime::usecases::{RuntimeResolutionRequest, RuntimeResolutionUseCase};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutInput;

use super::port::{
    AudioTranscriptionExecutionResult, AudioTranscriptionPreparationRequest,
    AudioTranscriptionPreparationResult, AudioTranscriptionPreparationUseCase,
    AudioTranscriptionUseCase, AudioUseCaseFuture,
};

/// Standard orchestration for preparing and executing batch audio transcription requests.
pub struct StdAudioTranscriptionUseCase<'a> {
    runtime_resolution: &'a dyn RuntimeResolutionUseCase,
    model_resolver: &'a dyn AudioTranscriptionModelResolver,
    runtime_client: &'a dyn AudioTranscriptionRuntimeClient,
}

impl<'a> StdAudioTranscriptionUseCase<'a> {
    pub fn new(
        runtime_resolution: &'a dyn RuntimeResolutionUseCase,
        model_resolver: &'a dyn AudioTranscriptionModelResolver,
        runtime_client: &'a dyn AudioTranscriptionRuntimeClient,
    ) -> Self {
        Self {
            runtime_resolution,
            model_resolver,
            runtime_client,
        }
    }
}

impl AudioTranscriptionPreparationUseCase for StdAudioTranscriptionUseCase<'_> {
    fn prepare_audio_transcription(
        &self,
        request: AudioTranscriptionPreparationRequest,
    ) -> KernelResult<AudioTranscriptionPreparationResult> {
        let mode = request.layout.mode;
        let runtime = self
            .runtime_resolution
            .resolve_runtime(RuntimeResolutionRequest {
                layout: request.layout,
                runtime: request.runtime,
            })?;
        let resolved_layout_input = RuntimeLayoutInput {
            mode,
            home_dir: Some(runtime.layout.home_dir.clone()),
            data_root_dir: Some(runtime.layout.data_root_dir.clone()),
        };
        let model = self.model_resolver.resolve_audio_transcription_model(
            AudioTranscriptionModelResolveRequest {
                layout: resolved_layout_input,
                selector: request.model_selector,
            },
        )?;
        let target = ResolvedAudioTranscriptionTarget {
            runtime: model.target.clone(),
        };

        Ok(AudioTranscriptionPreparationResult {
            layout: runtime.layout,
            runtime: runtime.runtime,
            model: model.model,
            request: AudioTranscriptionRequest {
                target,
                input_path: request.input_path,
                output_path: request.output_path,
                output_format: request.output_format,
                language: request.language,
                timestamps: request.timestamps,
            },
        })
    }
}

impl AudioTranscriptionUseCase for StdAudioTranscriptionUseCase<'_> {
    fn transcribe_audio(
        &'_ self,
        request: AudioTranscriptionPreparationRequest,
    ) -> AudioUseCaseFuture<'_, AudioTranscriptionExecutionResult> {
        Box::pin(async move {
            let prepared = self.prepare_audio_transcription(request)?;
            let response = self
                .runtime_client
                .transcribe_audio(AudioTranscriptionRuntimeRequest {
                    layout: prepared.layout.clone(),
                    runtime: prepared.runtime.clone(),
                    request: prepared.request.clone(),
                })
                .await?;

            Ok(AudioTranscriptionExecutionResult { prepared, response })
        })
    }
}
