use crate::features::model::domain::ModelCapability;
use crate::features::model::usecases::{ModelCatalogReadUseCase, ModelInspectRequest};
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{EmbeddingBackend, EmbeddingRuntimeTarget};
use super::super::ports::{
    EmbeddingModelResolveRequest, EmbeddingModelResolveResult, EmbeddingModelResolver,
};

/// Resolves embedding model targets by adapting the model catalog use-case boundary.
pub struct StdEmbeddingModelResolver<'a> {
    model_catalog: &'a dyn ModelCatalogReadUseCase,
}

impl<'a> StdEmbeddingModelResolver<'a> {
    pub fn new(model_catalog: &'a dyn ModelCatalogReadUseCase) -> Self {
        Self { model_catalog }
    }
}

impl EmbeddingModelResolver for StdEmbeddingModelResolver<'_> {
    fn resolve_embedding_model(
        &self,
        request: EmbeddingModelResolveRequest,
    ) -> KernelResult<EmbeddingModelResolveResult> {
        let result = self.model_catalog.inspect_model(ModelInspectRequest {
            layout: request.layout,
            selector: request.selector,
        })?;
        let metadata = &result.model.metadata;

        if !metadata.supports_capability(ModelCapability::Embedding) {
            return Err(KernelError::UnsupportedTarget(format!(
                "embedding endpoint requires model capability `embedding`, but model `{}` advertises {}",
                metadata.model_ref,
                model_capabilities_label(&metadata.model_capabilities)
            )));
        }

        let backend =
            EmbeddingBackend::from_model_format(metadata.primary_format).ok_or_else(|| {
                KernelError::UnsupportedTarget(format!(
                    "embedding endpoint does not support `{}` model format yet for model `{}`",
                    metadata.primary_format, metadata.model_ref
                ))
            })?;
        let target = EmbeddingRuntimeTarget::LocalModel {
            model_ref: metadata.model_ref.clone(),
            backend,
            source_repo: metadata.source_repo.clone(),
            source_revision: metadata.source_revision.clone(),
            model_capabilities: metadata.model_capabilities.clone(),
        };

        Ok(EmbeddingModelResolveResult {
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
