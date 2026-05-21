//! Standard image generation use case orchestration.

use crate::features::adapter::domain::AdapterCompatibilityTarget;
use crate::features::image_generation::domain::{
    ImageGenerationPrompt, ImageGenerationRequest, ImageGenerationRuntimeTarget,
    ResolvedImageGenerationTarget,
};
use crate::features::image_generation::ports::{
    ImageGenerationAdapterResolveRequest, ImageGenerationAdapterResolver,
    ImageGenerationModelResolveRequest, ImageGenerationModelResolver, ImageGenerationRuntimeClient,
    ImageGenerationRuntimeRequest,
};
use crate::features::model::domain::ModelCapability;
use crate::features::runtime::usecases::{RuntimeResolutionRequest, RuntimeResolutionUseCase};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayoutInput;

use super::port::{
    ImageGenerationExecutionResult, ImageGenerationPreparationRequest,
    ImageGenerationPreparationResult, ImageGenerationPreparationUseCase, ImageGenerationUseCase,
    ImageGenerationUseCaseFuture,
};

/// Standard orchestration for preparing and executing image-generation requests.
pub struct StdImageGenerationUseCase<'a> {
    runtime_resolution: &'a dyn RuntimeResolutionUseCase,
    model_resolver: &'a dyn ImageGenerationModelResolver,
    adapter_resolver: &'a dyn ImageGenerationAdapterResolver,
    runtime_client: &'a dyn ImageGenerationRuntimeClient,
}

impl<'a> StdImageGenerationUseCase<'a> {
    pub fn new(
        runtime_resolution: &'a dyn RuntimeResolutionUseCase,
        model_resolver: &'a dyn ImageGenerationModelResolver,
        adapter_resolver: &'a dyn ImageGenerationAdapterResolver,
        runtime_client: &'a dyn ImageGenerationRuntimeClient,
    ) -> Self {
        Self {
            runtime_resolution,
            model_resolver,
            adapter_resolver,
            runtime_client,
        }
    }
}

impl ImageGenerationPreparationUseCase for StdImageGenerationUseCase<'_> {
    fn prepare_image_generation(
        &self,
        request: ImageGenerationPreparationRequest,
    ) -> KernelResult<ImageGenerationPreparationResult> {
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
        let model = self.model_resolver.resolve_image_generation_model(
            ImageGenerationModelResolveRequest {
                layout: resolved_layout_input.clone(),
                selector: request.model_selector,
                workflow: request.input.workflow_kind(),
            },
        )?;
        let adapter = match (&request.adapter_selector, request.lora_scale, &model.target) {
            (
                Some(adapter_selector),
                lora_scale,
                ImageGenerationRuntimeTarget::LocalModel {
                    model_ref,
                    backend,
                    source_repo,
                    source_revision,
                    model_capabilities,
                },
            ) => Some(self.adapter_resolver.resolve_image_generation_adapter(
                ImageGenerationAdapterResolveRequest {
                    layout: resolved_layout_input,
                    selector: adapter_selector.clone(),
                    target: AdapterCompatibilityTarget {
                        base_model_ref: model_ref.clone(),
                        base_model_source_repo: source_repo.clone(),
                        base_model_source_revision: source_revision.clone(),
                        base_model_capabilities: model_capabilities.clone(),
                        required_capability: ModelCapability::ImageGeneration,
                        backend: backend.adapter_backend_support(),
                    },
                    lora_scale: lora_scale.unwrap_or_default(),
                },
            )?),
            (None, Some(_), _) => {
                return Err(KernelError::UnsupportedTarget(
                    "image generation LoRA scale requires an adapter reference".to_string(),
                ));
            }
            (None, None, _) => None,
        };
        let target = ResolvedImageGenerationTarget {
            runtime: model.target.clone(),
            adapter: adapter.as_ref().map(|adapter| adapter.target.clone()),
        };
        let prompt = ImageGenerationPrompt::new(request.prompt, request.negative_prompt)
            .map_err(|error| KernelError::UnsupportedTarget(error.to_string()))?;

        Ok(ImageGenerationPreparationResult {
            layout: runtime.layout,
            runtime: runtime.runtime,
            model: model.model,
            request: ImageGenerationRequest {
                target,
                input: request.input,
                prompt,
                output_path: request.output_path,
                output_format: request.output_format,
                options: request.options,
            },
        })
    }
}

impl ImageGenerationUseCase for StdImageGenerationUseCase<'_> {
    fn generate_image(
        &'_ self,
        request: ImageGenerationPreparationRequest,
    ) -> ImageGenerationUseCaseFuture<'_, ImageGenerationExecutionResult> {
        Box::pin(async move {
            let prepared = self.prepare_image_generation(request)?;
            let response = self
                .runtime_client
                .generate_image(ImageGenerationRuntimeRequest {
                    layout: prepared.layout.clone(),
                    runtime: prepared.runtime.clone(),
                    request: prepared.request.clone(),
                })
                .await?;

            Ok(ImageGenerationExecutionResult { prepared, response })
        })
    }
}
