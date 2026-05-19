use crate::features::model::domain::ModelCapability;
use crate::features::model::usecases::{ModelCatalogReadUseCase, ModelInspectRequest};
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{RerankBackend, RerankRuntimeTarget};
use super::super::ports::{
    RerankModelResolveRequest, RerankModelResolveResult, RerankModelResolver,
};

/// Resolves rerank model targets by adapting the model catalog use-case boundary.
pub struct StdRerankModelResolver<'a> {
    model_catalog: &'a dyn ModelCatalogReadUseCase,
}

impl<'a> StdRerankModelResolver<'a> {
    pub fn new(model_catalog: &'a dyn ModelCatalogReadUseCase) -> Self {
        Self { model_catalog }
    }
}

impl RerankModelResolver for StdRerankModelResolver<'_> {
    fn resolve_rerank_model(
        &self,
        request: RerankModelResolveRequest,
    ) -> KernelResult<RerankModelResolveResult> {
        let result = self.model_catalog.inspect_model(ModelInspectRequest {
            layout: request.layout,
            selector: request.selector,
        })?;
        let metadata = &result.model.metadata;

        if !metadata.supports_capability(ModelCapability::Rerank) {
            return Err(KernelError::UnsupportedTarget(format!(
                "rerank endpoint requires model capability `rerank`, but model `{}` advertises {}",
                metadata.model_ref,
                model_capabilities_label(&metadata.model_capabilities)
            )));
        }

        let backend =
            RerankBackend::from_model_format(metadata.primary_format).ok_or_else(|| {
                KernelError::UnsupportedTarget(format!(
                    "rerank endpoint does not support `{}` model format yet for model `{}`",
                    metadata.primary_format, metadata.model_ref
                ))
            })?;
        let target = RerankRuntimeTarget::LocalModel {
            model_ref: metadata.model_ref.clone(),
            backend,
            source_repo: metadata.source_repo.clone(),
            source_revision: metadata.source_revision.clone(),
            model_capabilities: metadata.model_capabilities.clone(),
        };

        Ok(RerankModelResolveResult {
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
