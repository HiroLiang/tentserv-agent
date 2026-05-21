use crate::features::model::domain::ModelCapability;
use crate::features::model::usecases::{ModelCatalogReadUseCase, ModelInspectRequest};
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{
    AudioSpeechBackend, AudioSpeechRuntimeTarget, AudioTranscriptionBackend,
    AudioTranscriptionRuntimeTarget,
};
use super::super::ports::{
    AudioSpeechModelResolveRequest, AudioSpeechModelResolveResult, AudioSpeechModelResolver,
    AudioTranscriptionModelResolveRequest, AudioTranscriptionModelResolveResult,
    AudioTranscriptionModelResolver,
};

/// Resolves audio transcription model targets by adapting the model catalog use-case boundary.
pub struct StdAudioTranscriptionModelResolver<'a> {
    model_catalog: &'a dyn ModelCatalogReadUseCase,
}

impl<'a> StdAudioTranscriptionModelResolver<'a> {
    pub fn new(model_catalog: &'a dyn ModelCatalogReadUseCase) -> Self {
        Self { model_catalog }
    }
}

impl AudioTranscriptionModelResolver for StdAudioTranscriptionModelResolver<'_> {
    fn resolve_audio_transcription_model(
        &self,
        request: AudioTranscriptionModelResolveRequest,
    ) -> KernelResult<AudioTranscriptionModelResolveResult> {
        let result = self.model_catalog.inspect_model(ModelInspectRequest {
            layout: request.layout,
            selector: request.selector,
        })?;
        let metadata = &result.model.metadata;

        if !metadata.supports_capability(ModelCapability::AudioTranscription) {
            return Err(KernelError::UnsupportedTarget(format!(
                "audio transcription endpoint requires model capability `audio-transcription`, but model `{}` advertises {}",
                metadata.model_ref,
                model_capabilities_label(&metadata.model_capabilities)
            )));
        }

        let backend = AudioTranscriptionBackend::from_model_format_and_mlx_family(
            metadata.primary_format,
            metadata.mlx_runtime_family,
        )
        .ok_or_else(|| {
            KernelError::UnsupportedTarget(format!(
                "audio transcription endpoint does not support `{}` model format{} yet for model `{}`",
                metadata.primary_format,
                mlx_runtime_family_suffix(metadata.mlx_runtime_family),
                metadata.model_ref
            ))
        })?;
        let target = AudioTranscriptionRuntimeTarget::LocalModel {
            model_ref: metadata.model_ref.clone(),
            backend,
            source_repo: metadata.source_repo.clone(),
            source_revision: metadata.source_revision.clone(),
            model_capabilities: metadata.model_capabilities.clone(),
        };

        Ok(AudioTranscriptionModelResolveResult {
            layout: result.layout,
            model: result.model,
            target,
        })
    }
}

/// Resolves audio speech model targets by adapting the model catalog use-case boundary.
pub struct StdAudioSpeechModelResolver<'a> {
    model_catalog: &'a dyn ModelCatalogReadUseCase,
}

impl<'a> StdAudioSpeechModelResolver<'a> {
    pub fn new(model_catalog: &'a dyn ModelCatalogReadUseCase) -> Self {
        Self { model_catalog }
    }
}

impl AudioSpeechModelResolver for StdAudioSpeechModelResolver<'_> {
    fn resolve_audio_speech_model(
        &self,
        request: AudioSpeechModelResolveRequest,
    ) -> KernelResult<AudioSpeechModelResolveResult> {
        let result = self.model_catalog.inspect_model(ModelInspectRequest {
            layout: request.layout,
            selector: request.selector,
        })?;
        let metadata = &result.model.metadata;

        if !metadata.supports_capability(ModelCapability::AudioSpeech) {
            return Err(KernelError::UnsupportedTarget(format!(
                "audio speech endpoint requires model capability `audio-speech`, but model `{}` advertises {}",
                metadata.model_ref,
                model_capabilities_label(&metadata.model_capabilities)
            )));
        }

        let backend = AudioSpeechBackend::from_model_format_and_mlx_family(
            metadata.primary_format,
            metadata.mlx_runtime_family,
        )
        .ok_or_else(|| {
            KernelError::UnsupportedTarget(format!(
                "audio speech endpoint does not support `{}` model format{} yet for model `{}`",
                metadata.primary_format,
                mlx_runtime_family_suffix(metadata.mlx_runtime_family),
                metadata.model_ref
            ))
        })?;
        let target = AudioSpeechRuntimeTarget::LocalModel {
            model_ref: metadata.model_ref.clone(),
            backend,
            source_repo: metadata.source_repo.clone(),
            source_revision: metadata.source_revision.clone(),
            model_capabilities: metadata.model_capabilities.clone(),
        };

        Ok(AudioSpeechModelResolveResult {
            layout: result.layout,
            model: result.model,
            target,
        })
    }
}

fn mlx_runtime_family_suffix(
    family: Option<crate::features::model::domain::MlxRuntimeFamily>,
) -> String {
    family
        .map(|family| format!(" with MLX runtime family `{family}`"))
        .unwrap_or_default()
}

fn model_capabilities_label(capabilities: &[ModelCapability]) -> String {
    if capabilities.is_empty() {
        return "[]".to_string();
    }

    format!(
        "[{}]",
        capabilities
            .iter()
            .map(|capability| capability.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    )
}
