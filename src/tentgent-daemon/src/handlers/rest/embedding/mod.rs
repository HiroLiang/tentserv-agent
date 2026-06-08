use axum::{extract::State, Json};
use serde::Serialize;
use serde_json::Value;
use tentgent_kernel::{
    features::{
        auth::{
            domain::{AuthEnvLoadPolicy, Provider},
            usecases::{AuthSecretResolutionRequest, AuthSecretResolverUseCase},
        },
        cloud::{
            domain::{CloudEmbeddingRequest, CloudEndpointCapability},
            infra::ReqwestCloudModelClient,
        },
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

use crate::{
    provider_compat::{
        ensure_provider_capability, map_provider_kernel_error, ProviderCompatRejection,
    },
    transport::rest::{error::RestError, state::RestState},
};

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

    Ok(Json(EmbeddingResponseBody::Native(
        native_embedding_response(result),
    )))
}

async fn cloud_embed(
    state: &RestState,
    provider: Provider,
    request: &EmbeddingRequestBody,
) -> Result<EmbeddingResponseBody, RestError> {
    ensure_provider_capability(provider, CloudEndpointCapability::Embedding)?;
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
        .map_err(|error| map_provider_kernel_error("embedding_runtime_failed", error))?;

    Ok(embedding_response_for_shape(
        request.response_shape,
        request.model_ref.clone(),
        response.vectors,
    ))
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
    response_shape: EmbeddingResponseShape,
}

impl EmbeddingRequestBody {
    fn from_value(value: Value) -> Result<Self, RestError> {
        let object = value.as_object().ok_or_else(|| {
            RestError::bad_request("bad_request", "request body must be a JSON object")
        })?;
        let unknown = object
            .keys()
            .filter(|key| {
                !matches!(
                    key.as_str(),
                    "model_ref"
                        | "model"
                        | "input"
                        | "provider"
                        | "dimensions"
                        | "encoding_format"
                        | "user"
                )
            })
            .map(String::as_str)
            .collect::<Vec<_>>();
        if !unknown.is_empty() {
            return Err(ProviderCompatRejection::unsupported_field(format!(
                "unsupported embedding request fields: {}",
                unknown.join(", ")
            ))
            .into());
        }
        reject_unsupported_embedding_fields(object)?;

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
        let response_shape = match (object.get("model"), cloud_provider) {
            (Some(_), Some(Provider::OpenAI)) => EmbeddingResponseShape::OpenAi,
            _ => EmbeddingResponseShape::Native,
        };
        let input = object
            .get("input")
            .ok_or_else(|| RestError::bad_request("bad_request", "`input` is required"))?;
        let input = embedding_input(input)?;

        Ok(Self {
            model_ref: model_value,
            input,
            cloud_provider,
            response_shape,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmbeddingResponseShape {
    Native,
    OpenAi,
}

fn reject_unsupported_embedding_fields(
    object: &serde_json::Map<String, Value>,
) -> Result<(), RestError> {
    if object.get("dimensions").is_some() {
        return Err(ProviderCompatRejection::unsupported_field(
            "OpenAI-compatible embeddings do not support dimensions overrides yet",
        )
        .into());
    }
    if let Some(format) = object.get("encoding_format") {
        match format.as_str() {
            Some("float") => {}
            Some("base64") => {
                return Err(ProviderCompatRejection::unsupported_field(
                    "OpenAI-compatible embeddings do not support base64 encoding yet",
                )
                .into())
            }
            _ => {
                return Err(RestError::bad_request(
                    "bad_request",
                    "`encoding_format` must be `float` or `base64`",
                ))
            }
        }
    }
    if object.get("user").is_some() {
        return Err(ProviderCompatRejection::unsupported_field(
            "OpenAI-compatible embeddings do not support user tracking metadata",
        )
        .into());
    }
    Ok(())
}

fn parse_cloud_provider(value: &str) -> Result<Provider, RestError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "openai" => Ok(Provider::OpenAI),
        "gemini" | "google" => Ok(Provider::Gemini),
        "anthropic" | "claude" => Ok(Provider::Anthropic),
        other => Err(ProviderCompatRejection::unsupported_capability(format!(
            "unsupported embedding provider `{other}`"
        ))
        .into()),
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
    fn openai_embedding_request_accepts_string_input_and_float_format() {
        let request = EmbeddingRequestBody::from_value(serde_json::json!({
            "model": "text-embedding-3-small",
            "input": "hello",
            "encoding_format": "float"
        }))
        .expect("request");

        assert_eq!(request.model_ref, "text-embedding-3-small");
        assert_eq!(request.input.items, vec!["hello"]);
        assert_eq!(request.cloud_provider, Some(Provider::OpenAI));
        assert_eq!(request.response_shape, EmbeddingResponseShape::OpenAi);
    }

    #[test]
    fn openai_embedding_request_accepts_string_array_input() {
        let request = EmbeddingRequestBody::from_value(serde_json::json!({
            "model": "text-embedding-3-small",
            "input": ["first", "second"]
        }))
        .expect("request");

        assert_eq!(request.input.items, vec!["first", "second"]);
        assert_eq!(request.response_shape, EmbeddingResponseShape::OpenAi);
    }

    #[test]
    fn gemini_embedding_request_accepts_provider_and_keeps_native_shape() {
        let request = EmbeddingRequestBody::from_value(serde_json::json!({
            "provider": "gemini",
            "model": "gemini-embedding-001",
            "input": "hello"
        }))
        .expect("request");

        assert_eq!(request.model_ref, "gemini-embedding-001");
        assert_eq!(request.input.items, vec!["hello"]);
        assert_eq!(request.cloud_provider, Some(Provider::Gemini));
        assert_eq!(request.response_shape, EmbeddingResponseShape::Native);
    }

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
        assert!(format!("{err:?}").contains("unsupported_provider_field"));
    }

    #[test]
    fn openai_embedding_request_rejects_dimensions_override() {
        let err = EmbeddingRequestBody::from_value(serde_json::json!({
            "model": "text-embedding-3-small",
            "input": "hello",
            "dimensions": 384
        }))
        .expect_err("dimensions unsupported");

        assert!(format!("{err:?}").contains("unsupported_provider_field"));
    }

    #[test]
    fn openai_embedding_request_rejects_base64_encoding() {
        let err = EmbeddingRequestBody::from_value(serde_json::json!({
            "model": "text-embedding-3-small",
            "input": "hello",
            "encoding_format": "base64"
        }))
        .expect_err("base64 unsupported");

        assert!(format!("{err:?}").contains("unsupported_provider_field"));
    }

    #[test]
    fn openai_embedding_response_uses_openai_list_shape() {
        let response = openai_embedding_response_from_vectors(
            "text-embedding-3-small".to_string(),
            vec![vec![0.1, 0.2], vec![0.3, 0.4]],
        );
        let value = serde_json::to_value(response).expect("json");

        assert_eq!(value["object"], "list");
        assert_eq!(value["model"], "text-embedding-3-small");
        assert_eq!(value["usage"], Value::Null);
        assert_eq!(value["data"][0]["object"], "embedding");
        assert_eq!(value["data"][0]["index"], 0);
        assert_eq!(
            value["data"][0]["embedding"],
            serde_json::json!([0.1f32, 0.2f32])
        );
        assert_eq!(value["data"][1]["object"], "embedding");
        assert_eq!(value["data"][1]["index"], 1);
        assert_eq!(
            value["data"][1]["embedding"],
            serde_json::json!([0.3f32, 0.4f32])
        );
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum EmbeddingResponseBody {
    Native(NativeEmbeddingResponseBody),
    OpenAi(OpenAiEmbeddingResponseBody),
}

#[derive(Debug, Serialize)]
pub struct NativeEmbeddingResponseBody {
    pub model_ref: String,
    pub data: Vec<EmbeddingItem>,
}

#[derive(Debug, Serialize)]
pub struct EmbeddingItem {
    pub index: usize,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Serialize)]
pub struct OpenAiEmbeddingResponseBody {
    object: &'static str,
    data: Vec<OpenAiEmbeddingItem>,
    model: String,
    usage: Option<Value>,
}

#[derive(Debug, Serialize)]
struct OpenAiEmbeddingItem {
    object: &'static str,
    index: usize,
    embedding: Vec<f32>,
}

fn native_embedding_response(result: EmbeddingExecutionResult) -> NativeEmbeddingResponseBody {
    NativeEmbeddingResponseBody {
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

fn embedding_response_for_shape(
    shape: EmbeddingResponseShape,
    model_ref: String,
    vectors: Vec<Vec<f32>>,
) -> EmbeddingResponseBody {
    match shape {
        EmbeddingResponseShape::Native => EmbeddingResponseBody::Native(
            native_embedding_response_from_vectors(model_ref, vectors),
        ),
        EmbeddingResponseShape::OpenAi => EmbeddingResponseBody::OpenAi(
            openai_embedding_response_from_vectors(model_ref, vectors),
        ),
    }
}

fn native_embedding_response_from_vectors(
    model_ref: String,
    vectors: Vec<Vec<f32>>,
) -> NativeEmbeddingResponseBody {
    NativeEmbeddingResponseBody {
        model_ref,
        data: vectors
            .into_iter()
            .enumerate()
            .map(|(index, embedding)| EmbeddingItem { index, embedding })
            .collect(),
    }
}

fn openai_embedding_response_from_vectors(
    model: String,
    vectors: Vec<Vec<f32>>,
) -> OpenAiEmbeddingResponseBody {
    OpenAiEmbeddingResponseBody {
        object: "list",
        data: vectors
            .into_iter()
            .enumerate()
            .map(|(index, embedding)| OpenAiEmbeddingItem {
                object: "embedding",
                index,
                embedding,
            })
            .collect(),
        model,
        usage: None,
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
