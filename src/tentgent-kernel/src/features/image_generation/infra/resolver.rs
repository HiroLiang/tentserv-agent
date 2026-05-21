use crate::features::adapter::usecases::{
    AdapterCompatibilityCheckRequest, AdapterCompatibilityCheckUseCase,
};
use crate::features::model::domain::ModelCapability;
use crate::features::model::usecases::{ModelCatalogReadUseCase, ModelInspectRequest};
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{
    ImageGenerationBackend, ImageGenerationRuntimeTarget, ResolvedImageGenerationAdapter,
};
use super::super::ports::{
    ImageGenerationAdapterResolveRequest, ImageGenerationAdapterResolveResult,
    ImageGenerationAdapterResolver, ImageGenerationModelResolveRequest,
    ImageGenerationModelResolveResult, ImageGenerationModelResolver,
};

/// Resolves image-generation model targets by adapting the model catalog use-case boundary.
pub struct StdImageGenerationModelResolver<'a> {
    model_catalog: &'a dyn ModelCatalogReadUseCase,
}

/// Resolves image-generation adapters by adapting the adapter compatibility use-case boundary.
pub struct StdImageGenerationAdapterResolver<'a> {
    compatibility: &'a dyn AdapterCompatibilityCheckUseCase,
}

impl<'a> StdImageGenerationAdapterResolver<'a> {
    pub fn new(compatibility: &'a dyn AdapterCompatibilityCheckUseCase) -> Self {
        Self { compatibility }
    }
}

impl ImageGenerationAdapterResolver for StdImageGenerationAdapterResolver<'_> {
    fn resolve_image_generation_adapter(
        &self,
        request: ImageGenerationAdapterResolveRequest,
    ) -> KernelResult<ImageGenerationAdapterResolveResult> {
        let backend = request.target.backend;
        let scale = request.lora_scale;
        let result =
            self.compatibility
                .check_adapter_compatibility(AdapterCompatibilityCheckRequest {
                    layout: request.layout,
                    adapter_selector: request.selector,
                    target: request.target,
                })?;
        let target = ResolvedImageGenerationAdapter {
            adapter_ref: result.adapter.metadata.adapter_ref.clone(),
            backend,
            source_path: result.adapter.source_path.clone(),
            weight_file: result.adapter.metadata.weight_file.clone(),
            scale,
        };

        Ok(ImageGenerationAdapterResolveResult {
            layout: result.layout,
            adapter: result.adapter,
            target,
        })
    }
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

        let backend = ImageGenerationBackend::from_model_format_and_mlx_family(
            metadata.primary_format,
            metadata.mlx_runtime_family,
        )
        .ok_or_else(|| {
            KernelError::UnsupportedTarget(format!(
                "image generation endpoint does not support `{}` model format{} yet for model `{}`",
                metadata.primary_format,
                mlx_runtime_family_suffix(metadata.mlx_runtime_family),
                metadata.model_ref
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
