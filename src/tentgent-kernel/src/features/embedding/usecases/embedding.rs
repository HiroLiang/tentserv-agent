//! Standard embedding use case orchestration.

use crate::features::embedding::domain::{EmbeddingRequest, ResolvedEmbeddingTarget};
use crate::features::embedding::ports::{
    EmbeddingModelResolveRequest, EmbeddingModelResolver, EmbeddingRuntimeClient,
    EmbeddingRuntimeRequest,
};
use crate::features::model::{
    domain::{ModelCapability, ModelCapabilityProofStatus},
    usecases::{ModelRuntimeExecutionEvidenceRecordRequest, ModelRuntimeExecutionEvidenceRecorder},
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
    runtime_evidence: Option<&'a dyn ModelRuntimeExecutionEvidenceRecorder>,
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
            runtime_evidence: None,
        }
    }

    pub fn new_with_runtime_evidence(
        runtime_resolution: &'a dyn RuntimeResolutionUseCase,
        model_resolver: &'a dyn EmbeddingModelResolver,
        runtime_client: &'a dyn EmbeddingRuntimeClient,
        runtime_evidence: &'a dyn ModelRuntimeExecutionEvidenceRecorder,
    ) -> Self {
        Self {
            runtime_resolution,
            model_resolver,
            runtime_client,
            runtime_evidence: Some(runtime_evidence),
        }
    }

    fn record_runtime_execution_evidence(
        &self,
        prepared: &EmbeddingPreparationResult,
        status: ModelCapabilityProofStatus,
        error: Option<String>,
    ) {
        let Some(recorder) = self.runtime_evidence else {
            return;
        };
        let _ = recorder.record_runtime_execution_evidence(
            ModelRuntimeExecutionEvidenceRecordRequest {
                layout: prepared.layout.clone(),
                metadata: prepared.model.metadata.clone(),
                capability: ModelCapability::Embedding,
                status,
                server_ref: None,
                runtime_profile: None,
                runtime_profile_version: None,
                error,
            },
        );
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
            let result = self
                .runtime_client
                .embed(EmbeddingRuntimeRequest {
                    layout: prepared.layout.clone(),
                    runtime: prepared.runtime.clone(),
                    request: prepared.request.clone(),
                })
                .await;
            match &result {
                Ok(_) => self.record_runtime_execution_evidence(
                    &prepared,
                    ModelCapabilityProofStatus::Verified,
                    None,
                ),
                Err(error) => self.record_runtime_execution_evidence(
                    &prepared,
                    ModelCapabilityProofStatus::Failed,
                    Some(error.to_string()),
                ),
            }
            let response = result?;

            Ok(EmbeddingExecutionResult { prepared, response })
        })
    }
}
