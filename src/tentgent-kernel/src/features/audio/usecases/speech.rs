//! Standard audio speech use case orchestration.

use crate::features::audio::domain::{AudioSpeechRequest, ResolvedAudioSpeechTarget};
use crate::features::audio::ports::{
    AudioSpeechModelResolveRequest, AudioSpeechModelResolver, AudioSpeechRuntimeClient,
    AudioSpeechRuntimeRequest,
};
use crate::features::runtime::usecases::{RuntimeResolutionRequest, RuntimeResolutionUseCase};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutInput;

use super::port::{
    AudioSpeechExecutionResult, AudioSpeechPreparationRequest, AudioSpeechPreparationResult,
    AudioSpeechPreparationUseCase, AudioSpeechUseCase, AudioUseCaseFuture,
};

/// Standard orchestration for preparing and executing audio speech requests.
pub struct StdAudioSpeechUseCase<'a> {
    runtime_resolution: &'a dyn RuntimeResolutionUseCase,
    model_resolver: &'a dyn AudioSpeechModelResolver,
    runtime_client: &'a dyn AudioSpeechRuntimeClient,
}

impl<'a> StdAudioSpeechUseCase<'a> {
    pub fn new(
        runtime_resolution: &'a dyn RuntimeResolutionUseCase,
        model_resolver: &'a dyn AudioSpeechModelResolver,
        runtime_client: &'a dyn AudioSpeechRuntimeClient,
    ) -> Self {
        Self {
            runtime_resolution,
            model_resolver,
            runtime_client,
        }
    }
}

impl AudioSpeechPreparationUseCase for StdAudioSpeechUseCase<'_> {
    fn prepare_audio_speech(
        &self,
        request: AudioSpeechPreparationRequest,
    ) -> KernelResult<AudioSpeechPreparationResult> {
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
        let model =
            self.model_resolver
                .resolve_audio_speech_model(AudioSpeechModelResolveRequest {
                    layout: resolved_layout_input,
                    selector: request.model_selector,
                })?;
        let target = ResolvedAudioSpeechTarget {
            runtime: model.target.clone(),
        };

        Ok(AudioSpeechPreparationResult {
            layout: runtime.layout,
            runtime: runtime.runtime,
            model: model.model,
            request: AudioSpeechRequest {
                target,
                text: request.text,
                output_path: request.output_path,
                output_format: request.output_format,
                language: request.language,
                voice: request.voice,
            },
        })
    }
}

impl AudioSpeechUseCase for StdAudioSpeechUseCase<'_> {
    fn synthesize_speech(
        &'_ self,
        request: AudioSpeechPreparationRequest,
    ) -> AudioUseCaseFuture<'_, AudioSpeechExecutionResult> {
        Box::pin(async move {
            let prepared = self.prepare_audio_speech(request)?;
            let response = self
                .runtime_client
                .synthesize_speech(AudioSpeechRuntimeRequest {
                    layout: prepared.layout.clone(),
                    runtime: prepared.runtime.clone(),
                    request: prepared.request.clone(),
                })
                .await?;

            Ok(AudioSpeechExecutionResult { prepared, response })
        })
    }
}
