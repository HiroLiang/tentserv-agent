use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use tentgent_kernel::features::{
    embedding::domain::EmbeddingInput, server::domain::ServerCapability,
};

use crate::provider_compat::ProviderCompatRejection;

use super::{
    capability::{ensure_local_provider_capability, ensure_model_endpoint},
    error::LocalServerError,
    native::{NativeLocalEmbeddingRequest, NativeLocalEmbeddingResponse},
    proxy::response_from_upstream,
    LocalServerState, RUNTIME_EMBEDDINGS_PATH,
};

pub(super) async fn openai_embeddings(
    State(state): State<LocalServerState>,
    Json(request): Json<Value>,
) -> Result<Response, LocalServerError> {
    ensure_local_provider_capability(
        state.config.capability,
        ServerCapability::Embedding,
        "local embeddings",
    )?;
    let endpoint = ensure_model_endpoint(&state).await?;
    if local_embedding_request_uses_openai_shape(&request) {
        let request = LocalOpenAiEmbeddingRequest::from_value(request)?;
        return openai_embeddings_to_upstream(
            &state.client,
            request,
            &endpoint.base_url,
            &state.config.model_ref,
            state.config.capability,
        )
        .await;
    }
    native_embedding_to_upstream(&state.client, request, &endpoint.base_url).await
}

pub(super) async fn openai_embeddings_to_upstream(
    client: &reqwest::Client,
    request: LocalOpenAiEmbeddingRequest,
    upstream_base_url: &str,
    bound_model_ref: &str,
    capability: ServerCapability,
) -> Result<Response, LocalServerError> {
    if capability != ServerCapability::Embedding {
        return Err(ProviderCompatRejection::unsupported_capability(format!(
            "OpenAI-compatible local embeddings require an embedding server; this server is bound to {}",
            capability.as_str()
        ))
        .into());
    }
    let upstream = client
        .post(format!(
            "{}{}",
            upstream_base_url.trim_end_matches('/'),
            RUNTIME_EMBEDDINGS_PATH
        ))
        .json(&NativeLocalEmbeddingRequest {
            input: request.input,
        })
        .send()
        .await
        .map_err(|err| {
            LocalServerError::bad_gateway(format!("model runtime proxy failed: {err}"))
        })?;
    openai_embedding_response_from_upstream(upstream, bound_model_ref).await
}

pub(super) async fn native_embedding_to_upstream(
    client: &reqwest::Client,
    request: Value,
    upstream_base_url: &str,
) -> Result<Response, LocalServerError> {
    let upstream = client
        .post(format!(
            "{}{}",
            upstream_base_url.trim_end_matches('/'),
            RUNTIME_EMBEDDINGS_PATH
        ))
        .json(&request)
        .send()
        .await
        .map_err(|err| {
            LocalServerError::bad_gateway(format!("model runtime proxy failed: {err}"))
        })?;
    response_from_upstream(upstream)
}

pub(super) async fn openai_embedding_response_from_upstream(
    upstream: reqwest::Response,
    bound_model_ref: &str,
) -> Result<Response, LocalServerError> {
    if !upstream.status().is_success() {
        return response_from_upstream(upstream);
    }
    let response = upstream
        .json::<NativeLocalEmbeddingResponse>()
        .await
        .map_err(|err| {
            LocalServerError::bad_gateway(format!("decode embedding response failed: {err}"))
        })?;
    Ok(Json(json!({
        "object": "list",
        "data": response.data.into_iter().map(|item| {
            json!({
                "object": "embedding",
                "index": item.index,
                "embedding": item.embedding
            })
        }).collect::<Vec<_>>(),
        "model": if response.model_ref.is_empty() {
            bound_model_ref.to_string()
        } else {
            response.model_ref
        },
        "usage": null
    }))
    .into_response())
}

#[derive(Debug)]
pub(super) struct LocalOpenAiEmbeddingRequest {
    input: Vec<String>,
}

impl LocalOpenAiEmbeddingRequest {
    pub(super) fn from_value(value: Value) -> Result<Self, LocalServerError> {
        let object = value.as_object().ok_or_else(|| {
            LocalServerError::bad_request("bad_request", "request body must be a JSON object")
        })?;
        let unknown = object
            .keys()
            .filter(|key| {
                !matches!(
                    key.as_str(),
                    "model" | "input" | "dimensions" | "encoding_format" | "user"
                )
            })
            .map(String::as_str)
            .collect::<Vec<_>>();
        if !unknown.is_empty() {
            return Err(ProviderCompatRejection::unsupported_field(format!(
                "unsupported OpenAI-compatible embedding request fields: {}",
                unknown.join(", ")
            ))
            .into());
        }
        if object.get("model").is_some_and(|value| !value.is_string()) {
            return Err(LocalServerError::bad_request(
                "bad_request",
                "`model` must be a string when present",
            ));
        }
        reject_unsupported_local_embedding_fields(object)?;
        let input = object
            .get("input")
            .ok_or_else(|| LocalServerError::bad_request("bad_request", "`input` is required"))?;
        Ok(Self {
            input: local_embedding_input(input)?,
        })
    }
}

pub(super) fn local_embedding_request_uses_openai_shape(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object.get("model").is_some_and(Value::is_string)
        || object.get("dimensions").is_some()
        || object.get("encoding_format").is_some()
        || object.get("user").is_some()
}

fn reject_unsupported_local_embedding_fields(
    object: &serde_json::Map<String, Value>,
) -> Result<(), LocalServerError> {
    if object.get("dimensions").is_some() {
        return Err(ProviderCompatRejection::unsupported_field(
            "OpenAI-compatible local embeddings do not support dimensions overrides yet",
        )
        .into());
    }
    if let Some(format) = object.get("encoding_format") {
        match format.as_str() {
            Some("float") => {}
            Some("base64") => {
                return Err(ProviderCompatRejection::unsupported_field(
                    "OpenAI-compatible local embeddings do not support base64 encoding yet",
                )
                .into())
            }
            _ => {
                return Err(LocalServerError::bad_request(
                    "bad_request",
                    "`encoding_format` must be `float` or `base64`",
                ))
            }
        }
    }
    if object.get("user").is_some() {
        return Err(ProviderCompatRejection::unsupported_field(
            "OpenAI-compatible local embeddings do not support user tracking metadata",
        )
        .into());
    }
    Ok(())
}

fn local_embedding_input(value: &Value) -> Result<Vec<String>, LocalServerError> {
    let items = match value {
        Value::String(item) => vec![item.clone()],
        Value::Array(items) => items
            .iter()
            .map(|item| {
                item.as_str().map(str::to_string).ok_or_else(|| {
                    LocalServerError::bad_request(
                        "bad_request",
                        "`input` must be a string or string array",
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(LocalServerError::bad_request(
                "bad_request",
                "`input` must be a string or string array",
            ))
        }
    };

    EmbeddingInput::new(items)
        .map(|input| input.items)
        .map_err(|err| LocalServerError::bad_request("bad_request", err.to_string()))
}
