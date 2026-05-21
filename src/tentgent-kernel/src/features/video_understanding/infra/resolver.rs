use crate::features::model::domain::ModelCapability;
use crate::features::model::usecases::{ModelCatalogReadUseCase, ModelInspectRequest};
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{VideoUnderstandingBackend, VideoUnderstandingRuntimeTarget};
use super::super::ports::{
    VideoUnderstandingModelResolveRequest, VideoUnderstandingModelResolveResult,
    VideoUnderstandingModelResolver,
};

pub struct StdVideoUnderstandingModelResolver<'a> {
    model_catalog: &'a dyn ModelCatalogReadUseCase,
}

impl<'a> StdVideoUnderstandingModelResolver<'a> {
    pub fn new(model_catalog: &'a dyn ModelCatalogReadUseCase) -> Self {
        Self { model_catalog }
    }
}

impl VideoUnderstandingModelResolver for StdVideoUnderstandingModelResolver<'_> {
    fn resolve_video_understanding_model(
        &self,
        request: VideoUnderstandingModelResolveRequest,
    ) -> KernelResult<VideoUnderstandingModelResolveResult> {
        let result = self.model_catalog.inspect_model(ModelInspectRequest {
            layout: request.layout,
            selector: request.selector,
        })?;
        let metadata = &result.model.metadata;

        if !metadata.supports_capability(ModelCapability::VideoUnderstanding) {
            return Err(KernelError::UnsupportedTarget(format!(
                "video understanding endpoint requires model capability `video-understanding`, but model `{}` advertises {}",
                metadata.model_ref,
                model_capabilities_label(&metadata.model_capabilities)
            )));
        }

        let backend = VideoUnderstandingBackend::from_model_format_and_mlx_family(
            metadata.primary_format,
            metadata.mlx_runtime_family,
        )
        .ok_or_else(|| {
            KernelError::UnsupportedTarget(format!(
                "video understanding endpoint does not support `{}` model format{} yet for model `{}`",
                metadata.primary_format,
                mlx_runtime_family_suffix(metadata.mlx_runtime_family),
                metadata.model_ref
            ))
        })?;
        let target = VideoUnderstandingRuntimeTarget::LocalModel {
            model_ref: metadata.model_ref.clone(),
            backend,
            source_repo: metadata.source_repo.clone(),
            source_revision: metadata.source_revision.clone(),
            model_capabilities: metadata.model_capabilities.clone(),
        };

        Ok(VideoUnderstandingModelResolveResult {
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
