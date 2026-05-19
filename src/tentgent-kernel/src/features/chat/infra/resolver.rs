use crate::features::adapter::usecases::{
    AdapterCompatibilityCheckRequest, AdapterCompatibilityCheckUseCase,
};
use crate::features::model::domain::ModelCapability;
use crate::features::model::usecases::{ModelCatalogReadUseCase, ModelInspectRequest};
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{ChatBackend, ChatRuntimeTarget, ResolvedChatAdapter};
use super::super::ports::{
    ChatAdapterResolveRequest, ChatAdapterResolveResult, ChatAdapterResolver,
    ChatModelResolveRequest, ChatModelResolveResult, ChatModelResolver,
};

/// Resolves chat model targets by adapting the model catalog use-case boundary.
pub struct StdChatModelResolver<'a> {
    model_catalog: &'a dyn ModelCatalogReadUseCase,
}

impl<'a> StdChatModelResolver<'a> {
    pub fn new(model_catalog: &'a dyn ModelCatalogReadUseCase) -> Self {
        Self { model_catalog }
    }
}

impl ChatModelResolver for StdChatModelResolver<'_> {
    fn resolve_chat_model(
        &self,
        request: ChatModelResolveRequest,
    ) -> KernelResult<ChatModelResolveResult> {
        let result = self.model_catalog.inspect_model(ModelInspectRequest {
            layout: request.layout,
            selector: request.selector,
        })?;
        let metadata = &result.model.metadata;

        if !metadata.supports_capability(ModelCapability::Chat) {
            return Err(KernelError::UnsupportedTarget(format!(
                "chat endpoint requires model capability `chat`, but model `{}` advertises {}",
                metadata.model_ref,
                model_capabilities_label(&metadata.model_capabilities)
            )));
        }

        let target = ChatRuntimeTarget::LocalModel {
            model_ref: metadata.model_ref.clone(),
            backend: ChatBackend::from_model_format(metadata.primary_format),
            source_repo: metadata.source_repo.clone(),
            source_revision: metadata.source_revision.clone(),
            model_capabilities: metadata.model_capabilities.clone(),
        };

        Ok(ChatModelResolveResult {
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

/// Resolves chat adapters by adapting the adapter compatibility use-case boundary.
pub struct StdChatAdapterResolver<'a> {
    compatibility: &'a dyn AdapterCompatibilityCheckUseCase,
}

impl<'a> StdChatAdapterResolver<'a> {
    pub fn new(compatibility: &'a dyn AdapterCompatibilityCheckUseCase) -> Self {
        Self { compatibility }
    }
}

impl ChatAdapterResolver for StdChatAdapterResolver<'_> {
    fn resolve_chat_adapter(
        &self,
        request: ChatAdapterResolveRequest,
    ) -> KernelResult<ChatAdapterResolveResult> {
        let backend = request.target.backend;
        let result =
            self.compatibility
                .check_adapter_compatibility(AdapterCompatibilityCheckRequest {
                    layout: request.layout,
                    adapter_selector: request.selector,
                    target: request.target,
                })?;
        let target = ResolvedChatAdapter {
            adapter_ref: result.adapter.metadata.adapter_ref.clone(),
            backend,
            source_path: result.adapter.source_path.clone(),
        };

        Ok(ChatAdapterResolveResult {
            layout: result.layout,
            adapter: result.adapter,
            target,
        })
    }
}
