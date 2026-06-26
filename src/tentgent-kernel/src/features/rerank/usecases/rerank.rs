//! Standard rerank use case orchestration.

use crate::features::model::{
    domain::{ModelCapability, ModelCapabilityProofStatus},
    usecases::{ModelRuntimeExecutionEvidenceRecordRequest, ModelRuntimeExecutionEvidenceRecorder},
};
use crate::features::rerank::domain::{RerankRequest, ResolvedRerankTarget};
use crate::features::rerank::ports::{
    RerankModelResolveRequest, RerankModelResolver, RerankRuntimeClient, RerankRuntimeRequest,
};
use crate::features::runtime::usecases::{RuntimeResolutionRequest, RuntimeResolutionUseCase};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutInput;

use super::port::{
    RerankExecutionResult, RerankPreparationRequest, RerankPreparationResult,
    RerankPreparationUseCase, RerankUseCase, RerankUseCaseFuture,
};

/// Standard orchestration for preparing and executing rerank requests.
pub struct StdRerankUseCase<'a> {
    runtime_resolution: &'a dyn RuntimeResolutionUseCase,
    model_resolver: &'a dyn RerankModelResolver,
    runtime_client: &'a dyn RerankRuntimeClient,
    runtime_evidence: Option<&'a dyn ModelRuntimeExecutionEvidenceRecorder>,
}

impl<'a> StdRerankUseCase<'a> {
    pub fn new(
        runtime_resolution: &'a dyn RuntimeResolutionUseCase,
        model_resolver: &'a dyn RerankModelResolver,
        runtime_client: &'a dyn RerankRuntimeClient,
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
        model_resolver: &'a dyn RerankModelResolver,
        runtime_client: &'a dyn RerankRuntimeClient,
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
        prepared: &RerankPreparationResult,
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
                capability: ModelCapability::Rerank,
                status,
                server_ref: None,
                runtime_profile: None,
                runtime_profile_version: None,
                error,
            },
        );
    }
}

impl RerankPreparationUseCase for StdRerankUseCase<'_> {
    fn prepare_rerank(
        &self,
        request: RerankPreparationRequest,
    ) -> KernelResult<RerankPreparationResult> {
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
            .resolve_rerank_model(RerankModelResolveRequest {
                layout: resolved_layout_input,
                selector: request.model_selector,
            })?;
        let target = ResolvedRerankTarget {
            runtime: model.target.clone(),
        };

        Ok(RerankPreparationResult {
            layout: runtime.layout,
            runtime: runtime.runtime,
            model: model.model,
            request: RerankRequest {
                target,
                input: request.input,
            },
        })
    }
}

impl RerankUseCase for StdRerankUseCase<'_> {
    fn rerank(
        &'_ self,
        request: RerankPreparationRequest,
    ) -> RerankUseCaseFuture<'_, RerankExecutionResult> {
        Box::pin(async move {
            let prepared = self.prepare_rerank(request)?;
            let result = self
                .runtime_client
                .rerank(RerankRuntimeRequest {
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

            Ok(RerankExecutionResult { prepared, response })
        })
    }
}
