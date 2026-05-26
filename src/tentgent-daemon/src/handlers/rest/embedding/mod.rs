use axum::{extract::State, Json};
use serde::Serialize;
use serde_json::Value;
use tentgent_kernel::{
    features::{
        auth::{
            domain::{AuthEnvLoadPolicy, Provider},
            usecases::{AuthSecretResolutionRequest, AuthSecretResolverUseCase},
        },
        cloud::{domain::CloudEmbeddingRequest, infra::ReqwestCloudModelClient},
        embedding::{
            domain::EmbeddingInput,
            usecases::{EmbeddingExecutionResult, EmbeddingPreparationRequest, EmbeddingUseCase},
        },
        model::{
            domain::ModelRefSelector,
            usecases::{ModelCatalogReadUseCase, ModelListRequest},
        },
        runtime::domain::PythonRuntimeResolutionInput,
    },
    foundation::{error::KernelError, layout::LayoutResolveMode},
};

use crate::transport::rest::{error::RestError, state::RestState};

pub async fn create(
    State(state): State<RestState>,
    Json(request): Json<Value>,
) -> Result<Json<EmbeddingResponseBody>, RestError> {
    let body = EmbeddingRequestBody::from_value(request)?;
    if let Some(provider) = body.cloud_provider {
        let result = cloud_embed(&state, provider, &body).await?;
        return Ok(Json(result));
    }
    let request = embedding_preparation_request(&state, body)?;
    let result = embed(state, request).await?;

    Ok(Json(embedding_response(result)))
}

async fn cloud_embed(
    state: &RestState,
    provider: Provider,
    request: &EmbeddingRequestBody,
) -> Result<EmbeddingResponseBody, RestError> {
    let secret = state
        .app()
        .services()
        .kernel()
        .auth()
        .resolve_secret(AuthSecretResolutionRequest::for_secret_use(
            provider,
            AuthEnvLoadPolicy::CwdDotenvOverride,
        ))
        .map_err(|error| RestError::kernel("embedding_auth_failed", error))?
        .secret
        .ok_or_else(|| {
            RestError::bad_request(
                "embedding_auth_failed",
                format!("{} API key is required", provider.display_name()),
            )
        })?;
    let client = ReqwestCloudModelClient::new()
        .map_err(|error| RestError::kernel("embedding_runtime_failed", error))?;
    let response = client
        .create_embedding(
            CloudEmbeddingRequest {
                provider,
                model: request.model_ref.clone(),
                input: request.input.items.clone(),
            },
            secret.secret(),
        )
        .await
        .map_err(|error| RestError::kernel("embedding_runtime_failed", error))?;

    Ok(EmbeddingResponseBody {
        model_ref: request.model_ref.clone(),
        data: response
            .vectors
            .into_iter()
            .enumerate()
            .map(|(index, embedding)| EmbeddingItem { index, embedding })
            .collect(),
    })
}

async fn embed(
    state: RestState,
    request: EmbeddingPreparationRequest,
) -> Result<EmbeddingExecutionResult, RestError> {
    let handle = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        handle.block_on(async {
            state
                .app()
                .services()
                .kernel()
                .embedding_usecase()
                .embed(request)
                .await
        })
    })
    .await
    .map_err(|error| {
        RestError::internal(
            "embedding_failed",
            format!("embedding task failed: {error}"),
        )
    })?
    .map_err(embedding_error)
}

fn embedding_preparation_request(
    state: &RestState,
    request: EmbeddingRequestBody,
) -> Result<EmbeddingPreparationRequest, RestError> {
    let model_selector = model_selector(state, &request.model_ref)?;

    Ok(EmbeddingPreparationRequest {
        layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
        runtime: PythonRuntimeResolutionInput::default(),
        model_selector,
        input: request.input,
    })
}

fn model_selector(state: &RestState, value: &str) -> Result<ModelRefSelector, RestError> {
    match ModelRefSelector::parse(value) {
        Ok(selector) => Ok(selector),
        Err(_) => model_alias_selector(state, value).map_err(|alias_error| alias_error.error),
    }
}

fn model_alias_selector(
    state: &RestState,
    value: &str,
) -> Result<ModelRefSelector, ModelAliasError> {
    let alias = value.trim();
    if alias.is_empty() {
        return Err(ModelAliasError {
            error: RestError::bad_request("bad_request", "model reference is empty"),
        });
    }
    let result = state
        .app()
        .services()
        .kernel()
        .models()
        .list_models(ModelListRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
        })
        .map_err(|error| ModelAliasError {
            error: RestError::store_lookup("embedding_model_failed", error.to_string()),
        })?;

    let matches = result
        .models
        .into_iter()
        .filter(|model| model_alias_matches(alias, model.metadata.source_repo.as_deref()))
        .map(|model| model.metadata.model_ref)
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [] => Err(ModelAliasError {
            error: RestError::not_found(
                "not_found",
                format!("model alias `{alias}` was not found"),
            ),
        }),
        [model_ref] => ModelRefSelector::parse(model_ref.as_str()).map_err(|err| ModelAliasError {
            error: RestError::internal("embedding_model_failed", err.to_string()),
        }),
        _ => Err(ModelAliasError {
            error: RestError::conflict(
                "ambiguous_ref",
                format!("model alias `{alias}` matches multiple stored models"),
            ),
        }),
    }
}

fn model_alias_matches(alias: &str, source_repo: Option<&str>) -> bool {
    let Some(source_repo) = source_repo else {
        return false;
    };
    source_repo.eq_ignore_ascii_case(alias)
        || source_repo
            .rsplit('/')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case(alias))
}

struct ModelAliasError {
    error: RestError,
}

#[derive(Debug)]
pub struct EmbeddingRequestBody {
    pub model_ref: String,
    pub input: EmbeddingInput,
    pub cloud_provider: Option<Provider>,
}

impl EmbeddingRequestBody {
    fn from_value(value: Value) -> Result<Self, RestError> {
        let object = value.as_object().ok_or_else(|| {
            RestError::bad_request("bad_request", "request body must be a JSON object")
        })?;
        let unknown = object
            .keys()
            .filter(|key| !matches!(key.as_str(), "model_ref" | "model" | "input" | "provider"))
            .map(String::as_str)
            .collect::<Vec<_>>();
        if !unknown.is_empty() {
            return Err(RestError::bad_request(
                "bad_request",
                format!(
                    "unsupported embedding request fields: {}",
                    unknown.join(", ")
                ),
            ));
        }

        let model_value = object
            .get("model_ref")
            .or_else(|| object.get("model"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                RestError::bad_request("bad_request", "`model_ref` or `model` must be a string")
            })?
            .to_string();
        let cloud_provider = object
            .get("provider")
            .and_then(Value::as_str)
            .map(parse_cloud_provider)
            .transpose()?
            .or_else(|| object.get("model").map(|_| Provider::OpenAI));
        let input = object
            .get("input")
            .ok_or_else(|| RestError::bad_request("bad_request", "`input` is required"))?;
        let input = embedding_input(input)?;

        Ok(Self {
            model_ref: model_value,
            input,
            cloud_provider,
        })
    }
}

fn parse_cloud_provider(value: &str) -> Result<Provider, RestError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "openai" => Ok(Provider::OpenAI),
        "gemini" | "google" => Ok(Provider::Gemini),
        "anthropic" | "claude" => Ok(Provider::Anthropic),
        other => Err(RestError::bad_request(
            "bad_request",
            format!("unsupported embedding provider `{other}`"),
        )),
    }
}

fn embedding_input(value: &Value) -> Result<EmbeddingInput, RestError> {
    let items = match value {
        Value::String(item) => vec![item.clone()],
        Value::Array(items) => items
            .iter()
            .map(|item| {
                item.as_str().map(str::to_string).ok_or_else(|| {
                    RestError::bad_request(
                        "bad_request",
                        "`input` must be a string or string array",
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(RestError::bad_request(
                "bad_request",
                "`input` must be a string or string array",
            ))
        }
    };

    EmbeddingInput::new(items).map_err(|err| RestError::bad_request("bad_request", err.to_string()))
}

#[cfg(test)]
mod request_tests {
    use super::*;

    #[test]
    fn embedding_input_rejects_empty_array() {
        let err = embedding_input(&Value::Array(Vec::new())).expect_err("empty array");

        assert!(format!("{err:?}").contains("RestError"));
    }

    #[test]
    fn embedding_request_rejects_unknown_fields() {
        let err = EmbeddingRequestBody::from_value(serde_json::json!({
            "model_ref": "aaaaaaaaaaaa",
            "input": "hello",
            "session_ref": "session"
        }))
        .expect_err("unknown field");

        assert!(format!("{err:?}").contains("RestError"));
    }
}

#[derive(Debug, Serialize)]
pub struct EmbeddingResponseBody {
    pub model_ref: String,
    pub data: Vec<EmbeddingItem>,
}

#[derive(Debug, Serialize)]
pub struct EmbeddingItem {
    pub index: usize,
    pub embedding: Vec<f32>,
}

fn embedding_response(result: EmbeddingExecutionResult) -> EmbeddingResponseBody {
    EmbeddingResponseBody {
        model_ref: result.prepared.model.metadata.model_ref.into_string(),
        data: result
            .response
            .data
            .into_iter()
            .map(|item| EmbeddingItem {
                index: item.index,
                embedding: item.embedding,
            })
            .collect(),
    }
}

fn embedding_error(error: KernelError) -> RestError {
    match error {
        KernelError::ModelStoreUnavailable(message) => {
            RestError::store_lookup("embedding_model_failed", message)
        }
        KernelError::UnsupportedTarget(message) => {
            RestError::bad_request("unsupported_target", message)
        }
        KernelError::RuntimeStateUnavailable(message) => {
            RestError::internal("embedding_runtime_unavailable", message)
        }
        KernelError::EmbeddingRuntimeUnavailable(message) => {
            RestError::internal("embedding_runtime_failed", message)
        }
        other => RestError::kernel("embedding_failed", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_alias_matches_huggingface_repo_or_repo_name() {
        assert!(model_alias_matches(
            "sentence-transformers/all-MiniLM-L6-v2",
            Some("sentence-transformers/all-MiniLM-L6-v2")
        ));
        assert!(model_alias_matches(
            "all-MiniLM-L6-v2",
            Some("sentence-transformers/all-MiniLM-L6-v2")
        ));
        assert!(!model_alias_matches(
            "all-MiniLM-L12-v2",
            Some("sentence-transformers/all-MiniLM-L6-v2")
        ));
    }
}
