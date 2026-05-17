//! Dataset synthesis use case.

use crate::features::auth::usecases::AuthSecretResolverUseCase;
use crate::features::dataset::ports::{
    DatasetSynthPromptRuntimeRequest, DatasetSynthRuntimeClient, DatasetSynthRuntimeRequest,
};
use crate::features::runtime::usecases::{RuntimeResolutionRequest, RuntimeResolutionUseCase};

use super::common::resolve_dataset_runtime_auth;
use super::port::{
    DatasetSynthPromptRenderRequest, DatasetSynthPromptRenderResult, DatasetSynthesisUseCase,
    DatasetSynthesizeRequest, DatasetSynthesizeResult, DatasetUseCaseFuture,
};

/// Standard provider-backed dataset synthesis orchestration.
pub struct StdDatasetSynthesisUseCase<'a> {
    runtime_resolution: &'a dyn RuntimeResolutionUseCase,
    auth_resolver: &'a dyn AuthSecretResolverUseCase,
    runtime_client: &'a dyn DatasetSynthRuntimeClient,
}

impl<'a> StdDatasetSynthesisUseCase<'a> {
    pub fn new(
        runtime_resolution: &'a dyn RuntimeResolutionUseCase,
        auth_resolver: &'a dyn AuthSecretResolverUseCase,
        runtime_client: &'a dyn DatasetSynthRuntimeClient,
    ) -> Self {
        Self {
            runtime_resolution,
            auth_resolver,
            runtime_client,
        }
    }
}

impl DatasetSynthesisUseCase for StdDatasetSynthesisUseCase<'_> {
    fn render_synth_prompt<'a>(
        &'a self,
        request: DatasetSynthPromptRenderRequest,
    ) -> DatasetUseCaseFuture<'a, DatasetSynthPromptRenderResult> {
        Box::pin(async move {
            let runtime = self
                .runtime_resolution
                .resolve_runtime(RuntimeResolutionRequest {
                    layout: request.layout,
                    runtime: request.runtime,
                })?;
            let prompt = self
                .runtime_client
                .render_synth_prompt(DatasetSynthPromptRuntimeRequest {
                    runtime: runtime.runtime.clone(),
                    request: request.prompt,
                })
                .await?;

            Ok(DatasetSynthPromptRenderResult {
                layout: runtime.layout,
                runtime: runtime.runtime,
                prompt,
            })
        })
    }

    fn synthesize_dataset<'a>(
        &'a self,
        request: DatasetSynthesizeRequest,
    ) -> DatasetUseCaseFuture<'a, DatasetSynthesizeResult> {
        Box::pin(async move {
            let provider = request.synth.provider;
            let runtime = self
                .runtime_resolution
                .resolve_runtime(RuntimeResolutionRequest {
                    layout: request.layout,
                    runtime: request.runtime,
                })?;
            let auth = resolve_dataset_runtime_auth(
                self.auth_resolver,
                request.auth,
                provider,
                "dataset synthesis",
            )?;
            let output = self
                .runtime_client
                .synthesize_dataset(DatasetSynthRuntimeRequest {
                    runtime: runtime.runtime.clone(),
                    auth,
                    request: request.synth,
                })
                .await?;

            Ok(DatasetSynthesizeResult {
                layout: runtime.layout,
                runtime: runtime.runtime,
                output,
            })
        })
    }
}
