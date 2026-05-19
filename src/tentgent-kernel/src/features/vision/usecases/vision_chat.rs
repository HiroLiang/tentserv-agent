//! Standard vision-chat use case orchestration.

use crate::features::runtime::usecases::{RuntimeResolutionRequest, RuntimeResolutionUseCase};
use crate::features::vision::domain::{
    ResolvedVisionChatTarget, VisionChatPrompt, VisionChatRequest,
};
use crate::features::vision::ports::{
    VisionChatModelResolveRequest, VisionChatModelResolver, VisionChatRuntimeClient,
    VisionChatRuntimeRequest,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutInput;

use super::port::{
    VisionChatExecutionResult, VisionChatPreparationRequest, VisionChatPreparationResult,
    VisionChatPreparationUseCase, VisionChatUseCase, VisionUseCaseFuture,
};

/// Standard orchestration for preparing and executing vision-chat requests.
pub struct StdVisionChatUseCase<'a> {
    runtime_resolution: &'a dyn RuntimeResolutionUseCase,
    model_resolver: &'a dyn VisionChatModelResolver,
    runtime_client: &'a dyn VisionChatRuntimeClient,
}

impl<'a> StdVisionChatUseCase<'a> {
    pub fn new(
        runtime_resolution: &'a dyn RuntimeResolutionUseCase,
        model_resolver: &'a dyn VisionChatModelResolver,
        runtime_client: &'a dyn VisionChatRuntimeClient,
    ) -> Self {
        Self {
            runtime_resolution,
            model_resolver,
            runtime_client,
        }
    }
}

impl VisionChatPreparationUseCase for StdVisionChatUseCase<'_> {
    fn prepare_vision_chat(
        &self,
        request: VisionChatPreparationRequest,
    ) -> KernelResult<VisionChatPreparationResult> {
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
                .resolve_vision_chat_model(VisionChatModelResolveRequest {
                    layout: resolved_layout_input,
                    selector: request.model_selector,
                })?;
        let target = ResolvedVisionChatTarget {
            runtime: model.target.clone(),
        };
        let prompt =
            VisionChatPrompt::new(request.prompt, request.system_prompt).map_err(|error| {
                crate::foundation::error::KernelError::UnsupportedTarget(error.to_string())
            })?;

        Ok(VisionChatPreparationResult {
            layout: runtime.layout,
            runtime: runtime.runtime,
            model: model.model,
            request: VisionChatRequest {
                target,
                image_path: request.image_path,
                image_media_type: request.image_media_type,
                prompt,
                output_format: request.output_format,
                options: request.options,
            },
        })
    }
}

impl VisionChatUseCase for StdVisionChatUseCase<'_> {
    fn generate_vision_chat(
        &'_ self,
        request: VisionChatPreparationRequest,
    ) -> VisionUseCaseFuture<'_, VisionChatExecutionResult> {
        Box::pin(async move {
            let prepared = self.prepare_vision_chat(request)?;
            let response = self
                .runtime_client
                .generate_vision_chat(VisionChatRuntimeRequest {
                    layout: prepared.layout.clone(),
                    runtime: prepared.runtime.clone(),
                    request: prepared.request.clone(),
                })
                .await?;

            Ok(VisionChatExecutionResult { prepared, response })
        })
    }
}
