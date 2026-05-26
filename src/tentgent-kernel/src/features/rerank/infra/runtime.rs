use serde::{Deserialize, Serialize};

use crate::features::runtime::infra::{ModelRuntimeCapability, ModelRuntimeDaemonSupervisor};
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{RerankRequest, RerankResponse, RerankRuntimeTarget, RerankScore};
use super::super::ports::{RerankPortFuture, RerankRuntimeClient, RerankRuntimeRequest};

/// Executes prepared rerank requests through the shared model-runtime HTTP daemon.
pub struct PythonRerankModelRuntimeClient<'a> {
    executable_resolver: &'a dyn RuntimeExecutableResolver,
    supervisor: &'a ModelRuntimeDaemonSupervisor,
}

impl<'a> PythonRerankModelRuntimeClient<'a> {
    pub fn new(
        executable_resolver: &'a dyn RuntimeExecutableResolver,
        supervisor: &'a ModelRuntimeDaemonSupervisor,
    ) -> Self {
        Self {
            executable_resolver,
            supervisor,
        }
    }

    async fn rerank_http(&self, request: RerankRuntimeRequest) -> KernelResult<RerankResponse> {
        let model_ref = local_model_ref(&request.request);
        let endpoint = self
            .supervisor
            .ensure_model_bound(
                &request.layout,
                &request.runtime,
                self.executable_resolver,
                ModelRuntimeCapability::Rerank,
                model_ref,
            )
            .await?;
        let payload = RerankPayload {
            query: request.request.input.query,
            documents: request.request.input.documents,
            top_n: request.request.input.top_n,
        };
        let response: RerankResponsePayload = self
            .supervisor
            .post_json(&endpoint, "/v1/rerank", &payload, rerank_runtime_error)
            .await?;
        Ok(RerankResponse {
            data: response
                .data
                .into_iter()
                .map(|item| RerankScore {
                    index: item.index,
                    score: item.score,
                })
                .collect(),
        })
    }
}

impl RerankRuntimeClient for PythonRerankModelRuntimeClient<'_> {
    fn rerank(&'_ self, request: RerankRuntimeRequest) -> RerankPortFuture<'_, RerankResponse> {
        Box::pin(async move { self.rerank_http(request).await })
    }
}

#[derive(Debug, Serialize)]
struct RerankPayload {
    query: String,
    documents: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_n: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct RerankResponsePayload {
    data: Vec<RerankScorePayload>,
}

#[derive(Debug, Deserialize)]
struct RerankScorePayload {
    index: usize,
    score: f32,
}

fn local_model_ref(request: &RerankRequest) -> &str {
    match &request.target.runtime {
        RerankRuntimeTarget::LocalModel { model_ref, .. } => model_ref.as_str(),
    }
}

fn rerank_runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::RerankRuntimeUnavailable(message.into())
}
