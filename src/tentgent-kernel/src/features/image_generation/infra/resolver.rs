use crate::features::model::domain::ModelCapability;
use crate::features::model::usecases::{ModelCatalogReadUseCase, ModelInspectRequest};
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{ImageGenerationBackend, ImageGenerationRuntimeTarget};
use super::super::ports::{
    ImageGenerationModelResolveRequest, ImageGenerationModelResolveResult,
    ImageGenerationModelResolver,
};

/// Resolves image-generation model targets by adapting the model catalog use-case boundary.
pub struct StdImageGenerationModelResolver<'a> {
    model_catalog: &'a dyn ModelCatalogReadUseCase,
}

impl<'a> StdImageGenerationModelResolver<'a> {
    pub fn new(model_catalog: &'a dyn ModelCatalogReadUseCase) -> Self {
        Self { model_catalog }
    }
}

impl ImageGenerationModelResolver for StdImageGenerationModelResolver<'_> {
    fn resolve_image_generation_model(
        &self,
        request: ImageGenerationModelResolveRequest,
    ) -> KernelResult<ImageGenerationModelResolveResult> {
        let result = self.model_catalog.inspect_model(ModelInspectRequest {
            layout: request.layout,
            selector: request.selector,
        })?;
        let metadata = &result.model.metadata;

        if !metadata.supports_capability(ModelCapability::ImageGeneration) {
            return Err(KernelError::UnsupportedTarget(format!(
                "image generation endpoint requires model capability `image-generation`, but model `{}` advertises {}",
                metadata.model_ref,
                model_capabilities_label(&metadata.model_capabilities)
            )));
        }

        let backend = ImageGenerationBackend::from_model_format(metadata.primary_format)
            .ok_or_else(|| {
                KernelError::UnsupportedTarget(format!(
                    "image generation endpoint does not support `{}` model format yet for model `{}`",
                    metadata.primary_format, metadata.model_ref
                ))
            })?;
        let target = ImageGenerationRuntimeTarget::LocalModel {
            model_ref: metadata.model_ref.clone(),
            backend,
            source_repo: metadata.source_repo.clone(),
            source_revision: metadata.source_revision.clone(),
            model_capabilities: metadata.model_capabilities.clone(),
        };

        Ok(ImageGenerationModelResolveResult {
            layout: result.layout,
            model: result.model,
            target,
        })
    }
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
