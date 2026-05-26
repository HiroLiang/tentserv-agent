use serde::{Deserialize, Serialize};

use crate::features::runtime::infra::{ModelRuntimeCapability, ModelRuntimeDaemonSupervisor};
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{
    EmbeddingRequest, EmbeddingResponse, EmbeddingRuntimeTarget, EmbeddingVector,
};
use super::super::ports::{EmbeddingPortFuture, EmbeddingRuntimeClient, EmbeddingRuntimeRequest};

/// Executes prepared embedding requests through the shared model-runtime HTTP daemon.
pub struct PythonEmbeddingModelRuntimeClient<'a> {
    executable_resolver: &'a dyn RuntimeExecutableResolver,
    supervisor: &'a ModelRuntimeDaemonSupervisor,
}

impl<'a> PythonEmbeddingModelRuntimeClient<'a> {
    pub fn new(
        executable_resolver: &'a dyn RuntimeExecutableResolver,
        supervisor: &'a ModelRuntimeDaemonSupervisor,
    ) -> Self {
        Self {
            executable_resolver,
            supervisor,
        }
    }

    async fn embed_http(
        &self,
        request: EmbeddingRuntimeRequest,
    ) -> KernelResult<EmbeddingResponse> {
        let model_ref = local_model_ref(&request.request);
        let endpoint = self
            .supervisor
            .ensure_model_bound(
                &request.layout,
                &request.runtime,
                self.executable_resolver,
                ModelRuntimeCapability::Embedding,
                model_ref,
            )
            .await?;
        let payload = EmbeddingPayload {
            input: request.request.input.items,
        };
        let response: EmbeddingResponsePayload = self
            .supervisor
            .post_json(
                &endpoint,
                "/v1/embeddings",
                &payload,
                embedding_runtime_error,
            )
            .await?;
        Ok(EmbeddingResponse {
            data: response
                .data
                .into_iter()
                .map(|item| EmbeddingVector {
                    index: item.index,
                    embedding: item.embedding,
                })
                .collect(),
        })
    }
}

impl EmbeddingRuntimeClient for PythonEmbeddingModelRuntimeClient<'_> {
    fn embed(
        &'_ self,
        request: EmbeddingRuntimeRequest,
    ) -> EmbeddingPortFuture<'_, EmbeddingResponse> {
        Box::pin(async move { self.embed_http(request).await })
    }
}

#[derive(Debug, Serialize)]
struct EmbeddingPayload {
    input: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponsePayload {
    data: Vec<EmbeddingVectorPayload>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingVectorPayload {
    index: usize,
    embedding: Vec<f32>,
}

fn local_model_ref(request: &EmbeddingRequest) -> &str {
    match &request.target.runtime {
        EmbeddingRuntimeTarget::LocalModel { model_ref, .. } => model_ref.as_str(),
    }
}

fn embedding_runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::EmbeddingRuntimeUnavailable(message.into())
}
