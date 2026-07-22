use axum::{extract::State, response::Response, Json};
use serde::Deserialize;
use tentgent_kernel::{
    features::{
        adapter::{
            domain::{AdapterBackendSupport, AdapterCompatibilityTarget, AdapterRefSelector},
            infra::FileAdapterCatalogStore,
            usecases::{
                AdapterCompatibilityCheckRequest, AdapterCompatibilityCheckUseCase,
                StdAdapterCompatibilityCheckUseCase,
            },
        },
        model::{
            domain::{ModelRefSelector, ModelStoreLayout},
            infra::FileModelCatalogStore,
            ports::ModelCatalogStore,
        },
        server::domain::ServerRuntimeBackend,
    },
    foundation::layout::{LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver},
};

use super::{
    capability::ensure_model_endpoint,
    error::LocalServerError,
    evidence::record_runtime_execution_result,
    native::{NativeAdapterRecordPayload, NativeLocalChatMessage, NativeLocalChatRequest},
    proxy::response_from_upstream,
    LocalServerState, RUNTIME_CHAT_PATH, RUNTIME_CHAT_STREAM_PATH,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::server) struct ManagedNativeChatRequest {
    messages: Vec<NativeLocalChatMessage>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    adapter_ref: Option<String>,
}

pub(in crate::server) async fn managed_native_chat(
    State(state): State<LocalServerState>,
    Json(request): Json<ManagedNativeChatRequest>,
) -> Result<Response, LocalServerError> {
    forward_managed_chat(state, request, false).await
}

pub(in crate::server) async fn managed_native_chat_stream(
    State(state): State<LocalServerState>,
    Json(request): Json<ManagedNativeChatRequest>,
) -> Result<Response, LocalServerError> {
    forward_managed_chat(state, request, true).await
}

async fn forward_managed_chat(
    state: LocalServerState,
    request: ManagedNativeChatRequest,
    stream: bool,
) -> Result<Response, LocalServerError> {
    if request.messages.is_empty() {
        return Err(LocalServerError::bad_request(
            "bad_request",
            "chat requests must contain at least one message",
        ));
    }
    let adapter = request
        .adapter_ref
        .as_deref()
        .map(|adapter_ref| resolve_managed_adapter(&state, adapter_ref))
        .transpose()?;
    let payload = NativeLocalChatRequest {
        messages: request.messages,
        max_tokens: request.max_tokens,
        temperature: request.temperature,
        adapter,
    };
    let endpoint = ensure_model_endpoint(&state).await?;
    let path = if stream {
        RUNTIME_CHAT_STREAM_PATH
    } else {
        RUNTIME_CHAT_PATH
    };
    let result = async {
        let upstream = state
            .client
            .post(endpoint.url(path))
            .json(&payload)
            .send()
            .await
            .map_err(|error| {
                LocalServerError::bad_gateway(format!(
                    "model runtime managed chat proxy failed: {error}"
                ))
            })?;
        response_from_upstream(upstream)
    }
    .await;
    record_runtime_execution_result(&state, &result);
    result
}

pub(in crate::server) fn resolve_managed_adapter(
    state: &LocalServerState,
    adapter_ref: &str,
) -> Result<NativeAdapterRecordPayload, LocalServerError> {
    let model_selector = ModelRefSelector::parse(&state.config.model_ref)
        .map_err(|error| LocalServerError::internal(error.to_string()))?;
    let model = FileModelCatalogStore
        .inspect_model(
            &ModelStoreLayout::from_models_dir(state.layout.models_dir.clone()),
            &model_selector,
        )
        .map_err(|error| LocalServerError::internal(error.to_string()))?;
    let runtime_backend = ServerRuntimeBackend::from_model_format(model.metadata.primary_format)
        .ok_or_else(|| {
            LocalServerError::bad_request(
                "adapter_unsupported",
                "selected model format does not support managed adapters",
            )
        })?;
    let backend = match runtime_backend {
        ServerRuntimeBackend::Diffusers => AdapterBackendSupport::Diffusers,
        ServerRuntimeBackend::TransformersPeft => AdapterBackendSupport::TransformersPeft,
        ServerRuntimeBackend::Mlx => AdapterBackendSupport::Mlx,
        ServerRuntimeBackend::LlamaCpp => AdapterBackendSupport::LlamaCpp,
    };
    let adapter_selector = AdapterRefSelector::parse(adapter_ref)
        .map_err(|error| LocalServerError::bad_request("invalid_adapter_ref", error.to_string()))?;
    let compatibility = StdAdapterCompatibilityCheckUseCase::new(
        &StdRuntimeLayoutResolver,
        &FileAdapterCatalogStore,
    );
    let result = compatibility
        .check_adapter_compatibility(AdapterCompatibilityCheckRequest {
            layout: RuntimeLayoutInput {
                mode: LayoutResolveMode::ReadOnly,
                home_dir: Some(state.layout.home_dir.clone()),
                data_root_dir: None,
            },
            adapter_selector,
            target: AdapterCompatibilityTarget {
                base_model_ref: model.metadata.model_ref.clone(),
                base_model_source_repo: model.metadata.source_repo.clone(),
                base_model_source_revision: model.metadata.source_revision.clone(),
                base_model_capabilities: model.metadata.model_capabilities.clone(),
                required_capability: state.config.capability.required_model_capability(),
                backend,
            },
        })
        .map_err(|error| {
            LocalServerError::bad_request("adapter_incompatible", error.to_string())
        })?;
    Ok(NativeAdapterRecordPayload {
        adapter_ref: result.adapter.metadata.adapter_ref.to_string(),
        source_path: result.adapter.source_path.display().to_string(),
        adapter_format: match backend {
            AdapterBackendSupport::TransformersPeft => "peft",
            AdapterBackendSupport::Mlx => "mlx",
            AdapterBackendSupport::Diffusers => "diffusers-lora",
            AdapterBackendSupport::MlxDiffusion => "mlx-diffusion-lora",
            AdapterBackendSupport::LlamaCpp => "llama-cpp",
        }
        .to_string(),
        adapter_type: "lora",
        short_ref: result.adapter.metadata.short_ref,
    })
}

#[cfg(test)]
mod tests {
    use super::ManagedNativeChatRequest;
    use serde_json::json;

    #[test]
    fn native_request_accepts_managed_adapter_ref() {
        let request: ManagedNativeChatRequest = serde_json::from_value(json!({
            "messages": [{"role": "user", "content": "hello"}],
            "adapter_ref": "aabbcc"
        }))
        .expect("managed adapter ref request");
        assert_eq!(request.adapter_ref.as_deref(), Some("aabbcc"));
    }

    #[test]
    fn native_request_rejects_trusted_internal_adapter_payload() {
        let error = serde_json::from_value::<ManagedNativeChatRequest>(json!({
            "messages": [{"role": "user", "content": "hello"}],
            "adapter": {
                "adapter_ref": "aabbcc",
                "source_path": "/private/adapter/path",
                "adapter_format": "peft",
                "adapter_type": "lora",
                "short_ref": "aabbcc"
            }
        }))
        .expect_err("public request must not accept internal adapter paths");
        assert!(error.to_string().contains("unknown field `adapter`"));
    }
}
