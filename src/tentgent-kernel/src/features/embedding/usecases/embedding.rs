//! Standard embedding use case orchestration.

use crate::features::embedding::domain::{EmbeddingRequest, ResolvedEmbeddingTarget};
use crate::features::embedding::ports::{
    EmbeddingModelResolveRequest, EmbeddingModelResolver, EmbeddingRuntimeClient,
    EmbeddingRuntimeRequest,
};
use crate::features::runtime::usecases::{RuntimeResolutionRequest, RuntimeResolutionUseCase};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutInput;

use super::port::{
    EmbeddingExecutionResult, EmbeddingPreparationRequest, EmbeddingPreparationResult,
    EmbeddingPreparationUseCase, EmbeddingUseCase, EmbeddingUseCaseFuture,
};

/// Standard orchestration for preparing and executing embedding requests.
pub struct StdEmbeddingUseCase<'a> {
    runtime_resolution: &'a dyn RuntimeResolutionUseCase,
    model_resolver: &'a dyn EmbeddingModelResolver,
    runtime_client: &'a dyn EmbeddingRuntimeClient,
}

impl<'a> StdEmbeddingUseCase<'a> {
    pub fn new(
        runtime_resolution: &'a dyn RuntimeResolutionUseCase,
        model_resolver: &'a dyn EmbeddingModelResolver,
        runtime_client: &'a dyn EmbeddingRuntimeClient,
    ) -> Self {
        Self {
            runtime_resolution,
            model_resolver,
            runtime_client,
        }
    }
}

impl EmbeddingPreparationUseCase for StdEmbeddingUseCase<'_> {
    fn prepare_embedding(
        &self,
        request: EmbeddingPreparationRequest,
    ) -> KernelResult<EmbeddingPreparationResult> {
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
        let model = self
            .model_resolver
            .resolve_embedding_model(EmbeddingModelResolveRequest {
                layout: resolved_layout_input,
                selector: request.model_selector,
            })?;
        let target = ResolvedEmbeddingTarget {
            runtime: model.target.clone(),
        };

        Ok(EmbeddingPreparationResult {
            layout: runtime.layout,
            runtime: runtime.runtime,
            model: model.model,
            request: EmbeddingRequest {
                target,
                input: request.input,
            },
        })
    }
}

impl EmbeddingUseCase for StdEmbeddingUseCase<'_> {
    fn embed(
        &'_ self,
        request: EmbeddingPreparationRequest,
    ) -> EmbeddingUseCaseFuture<'_, EmbeddingExecutionResult> {
        Box::pin(async move {
            let prepared = self.prepare_embedding(request)?;
            let response = self
                .runtime_client
                .embed(EmbeddingRuntimeRequest {
                    layout: prepared.layout.clone(),
                    runtime: prepared.runtime.clone(),
                    request: prepared.request.clone(),
                })
                .await?;

            Ok(EmbeddingExecutionResult { prepared, response })
        })
    }
}
