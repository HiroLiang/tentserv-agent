//! Dataset evaluation use case.

use crate::features::auth::usecases::AuthSecretResolverUseCase;
use crate::features::dataset::domain::DatasetEvalRequest;
use crate::features::dataset::ports::{
    DatasetCatalogStore, DatasetEvalRuntimeClient, DatasetEvalRuntimeRequest,
};
use crate::features::runtime::usecases::{RuntimeResolutionRequest, RuntimeResolutionUseCase};

use super::common::{dataset_store_layout, resolve_dataset_runtime_auth};
use super::port::{
    DatasetEvaluateRequest, DatasetEvaluateResult, DatasetEvaluationInputSelection,
    DatasetEvaluationUseCase, DatasetUseCaseFuture,
};

/// Standard provider-backed dataset evaluation orchestration.
pub struct StdDatasetEvaluationUseCase<'a> {
    runtime_resolution: &'a dyn RuntimeResolutionUseCase,
    auth_resolver: &'a dyn AuthSecretResolverUseCase,
    catalog: &'a dyn DatasetCatalogStore,
    runtime_client: &'a dyn DatasetEvalRuntimeClient,
}

impl<'a> StdDatasetEvaluationUseCase<'a> {
    pub fn new(
        runtime_resolution: &'a dyn RuntimeResolutionUseCase,
        auth_resolver: &'a dyn AuthSecretResolverUseCase,
        catalog: &'a dyn DatasetCatalogStore,
        runtime_client: &'a dyn DatasetEvalRuntimeClient,
    ) -> Self {
        Self {
            runtime_resolution,
            auth_resolver,
            catalog,
            runtime_client,
        }
    }
}

impl DatasetEvaluationUseCase for StdDatasetEvaluationUseCase<'_> {
    fn evaluate_dataset(
        &self,
        request: DatasetEvaluateRequest,
    ) -> DatasetUseCaseFuture<'_, DatasetEvaluateResult> {
        Box::pin(async move {
            let provider = request.provider;
            let runtime = self
                .runtime_resolution
                .resolve_runtime(RuntimeResolutionRequest {
                    layout: request.layout,
                    runtime: request.runtime,
                })?;
            let store = dataset_store_layout(&runtime.layout);
            let (dataset, input_path) = match request.input {
                DatasetEvaluationInputSelection::LocalPath(path) => (None, path),
                DatasetEvaluationInputSelection::ManagedDataset(selector) => {
                    let inspection = self.catalog.inspect_dataset(&store, &selector)?;
                    let path = inspection.source_path.clone();
                    (Some(inspection), path)
                }
            };
            let auth = resolve_dataset_runtime_auth(
                self.auth_resolver,
                request.auth,
                provider,
                "dataset evaluation",
            )?;
            let report = self
                .runtime_client
                .evaluate_dataset(DatasetEvalRuntimeRequest {
                    runtime: runtime.runtime.clone(),
                    auth,
                    request: DatasetEvalRequest {
                        provider,
                        provider_model: request.provider_model,
                        input: input_path.clone(),
                        output_dir: request.output_dir,
                        split: request.split,
                        max_records: request.max_records,
                        criteria: request.criteria,
                        max_tokens: request.max_tokens,
                        temperature: request.temperature,
                        timeout_seconds: request.timeout_seconds,
                    },
                })
                .await?;

            Ok(DatasetEvaluateResult {
                layout: runtime.layout,
                store,
                runtime: runtime.runtime,
                dataset,
                input_path,
                report,
            })
        })
    }
}
