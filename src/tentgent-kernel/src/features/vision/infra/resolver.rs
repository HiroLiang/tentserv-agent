use crate::features::model::domain::ModelCapability;
use crate::features::model::usecases::{ModelCatalogReadUseCase, ModelInspectRequest};
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{VisionChatBackend, VisionChatRuntimeTarget};
use super::super::ports::{
    VisionChatModelResolveRequest, VisionChatModelResolveResult, VisionChatModelResolver,
};

/// Resolves vision-chat model targets by adapting the model catalog use-case boundary.
pub struct StdVisionChatModelResolver<'a> {
    model_catalog: &'a dyn ModelCatalogReadUseCase,
}

impl<'a> StdVisionChatModelResolver<'a> {
    pub fn new(model_catalog: &'a dyn ModelCatalogReadUseCase) -> Self {
        Self { model_catalog }
    }
}

impl VisionChatModelResolver for StdVisionChatModelResolver<'_> {
    fn resolve_vision_chat_model(
        &self,
        request: VisionChatModelResolveRequest,
    ) -> KernelResult<VisionChatModelResolveResult> {
        let result = self.model_catalog.inspect_model(ModelInspectRequest {
            layout: request.layout,
            selector: request.selector,
        })?;
        let metadata = &result.model.metadata;

        if !metadata.supports_capability(ModelCapability::VisionChat) {
            return Err(KernelError::UnsupportedTarget(format!(
                "vision chat endpoint requires model capability `vision-chat`, but model `{}` advertises {}",
                metadata.model_ref,
                model_capabilities_label(&metadata.model_capabilities)
            )));
        }

        let backend =
            VisionChatBackend::from_model_format(metadata.primary_format).ok_or_else(|| {
                KernelError::UnsupportedTarget(format!(
                    "vision chat endpoint does not support `{}` model format yet for model `{}`",
                    metadata.primary_format, metadata.model_ref
                ))
            })?;
        let target = VisionChatRuntimeTarget::LocalModel {
            model_ref: metadata.model_ref.clone(),
            backend,
            source_repo: metadata.source_repo.clone(),
            source_revision: metadata.source_revision.clone(),
            model_capabilities: metadata.model_capabilities.clone(),
        };

        Ok(VisionChatModelResolveResult {
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
