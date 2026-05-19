use axum::{extract::State, Json};
use serde::Serialize;
use serde_json::Value;
use tentgent_kernel::{
    features::{
        model::{
            domain::ModelRefSelector,
            usecases::{ModelCatalogReadUseCase, ModelListRequest},
        },
        rerank::{
            domain::RerankInput,
            usecases::{RerankExecutionResult, RerankPreparationRequest, RerankUseCase},
        },
        runtime::domain::PythonRuntimeResolutionInput,
    },
    foundation::{error::KernelError, layout::LayoutResolveMode},
};

use crate::transport::rest::{error::RestError, state::RestState};

pub async fn create(
    State(state): State<RestState>,
    Json(request): Json<Value>,
) -> Result<Json<RerankResponseBody>, RestError> {
    let request = rerank_preparation_request(&state, request)?;
    let result = rerank(state, request).await?;

    Ok(Json(rerank_response(result)))
}

async fn rerank(
    state: RestState,
    request: RerankPreparationRequest,
) -> Result<RerankExecutionResult, RestError> {
    let handle = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        handle.block_on(async {
            state
                .app()
                .services()
                .kernel()
                .rerank_usecase()
                .rerank(request)
                .await
        })
    })
    .await
    .map_err(|error| RestError::internal("rerank_failed", format!("rerank task failed: {error}")))?
    .map_err(rerank_error)
}

fn rerank_preparation_request(
    state: &RestState,
    request: Value,
) -> Result<RerankPreparationRequest, RestError> {
    let request = RerankRequestBody::from_value(request)?;
    let model_selector = model_selector(state, &request.model_ref)?;

    Ok(RerankPreparationRequest {
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
            error: RestError::store_lookup("rerank_model_failed", error.to_string()),
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
            error: RestError::internal("rerank_model_failed", err.to_string()),
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
pub struct RerankRequestBody {
    pub model_ref: String,
    pub input: RerankInput,
}

impl RerankRequestBody {
    fn from_value(value: Value) -> Result<Self, RestError> {
        let object = value.as_object().ok_or_else(|| {
            RestError::bad_request("bad_request", "request body must be a JSON object")
        })?;
        let unknown = object
            .keys()
            .filter(|key| !matches!(key.as_str(), "model_ref" | "query" | "documents" | "top_n"))
            .map(String::as_str)
            .collect::<Vec<_>>();
        if !unknown.is_empty() {
            return Err(RestError::bad_request(
                "bad_request",
                format!("unsupported rerank request fields: {}", unknown.join(", ")),
            ));
        }

        let model_ref = object
            .get("model_ref")
            .and_then(Value::as_str)
            .ok_or_else(|| RestError::bad_request("bad_request", "`model_ref` must be a string"))?
            .to_string();
        let query = object
            .get("query")
            .and_then(Value::as_str)
            .ok_or_else(|| RestError::bad_request("bad_request", "`query` must be a string"))?
            .to_string();
        let documents = object
            .get("documents")
            .ok_or_else(|| RestError::bad_request("bad_request", "`documents` is required"))?;
        let documents = rerank_documents(documents)?;
        let top_n = object.get("top_n").map(rerank_top_n).transpose()?;
        let input = RerankInput::new(query, documents, top_n)
            .map_err(|err| RestError::bad_request("bad_request", err.to_string()))?;

        Ok(Self { model_ref, input })
    }
}

fn rerank_documents(value: &Value) -> Result<Vec<String>, RestError> {
    let Value::Array(items) = value else {
        return Err(RestError::bad_request(
            "bad_request",
            "`documents` must be a string array",
        ));
    };

    items
        .iter()
        .map(|item| {
            item.as_str().map(str::to_string).ok_or_else(|| {
                RestError::bad_request("bad_request", "`documents` must be a string array")
            })
        })
        .collect()
}

fn rerank_top_n(value: &Value) -> Result<usize, RestError> {
    let raw = value.as_u64().ok_or_else(|| {
        RestError::bad_request("bad_request", "`top_n` must be a positive integer")
    })?;
    usize::try_from(raw).map_err(|_| RestError::bad_request("bad_request", "`top_n` is too large"))
}

#[cfg(test)]
mod request_tests {
    use super::*;

    #[test]
    fn rerank_request_rejects_invalid_top_n() {
        let err = RerankRequestBody::from_value(serde_json::json!({
            "model_ref": "aaaaaaaaaaaa",
            "query": "q",
            "documents": ["one"],
            "top_n": 2
        }))
        .expect_err("invalid top_n");

        assert!(format!("{err:?}").contains("RestError"));
    }

    #[test]
    fn rerank_request_rejects_unknown_fields() {
        let err = RerankRequestBody::from_value(serde_json::json!({
            "model_ref": "aaaaaaaaaaaa",
            "query": "q",
            "documents": ["one"],
            "session_ref": "session"
        }))
        .expect_err("unknown field");

        assert!(format!("{err:?}").contains("RestError"));
    }
}

#[derive(Debug, Serialize)]
pub struct RerankResponseBody {
    pub model_ref: String,
    pub data: Vec<RerankItem>,
}

#[derive(Debug, Serialize)]
pub struct RerankItem {
    pub index: usize,
    pub score: f32,
}

fn rerank_response(result: RerankExecutionResult) -> RerankResponseBody {
    RerankResponseBody {
        model_ref: result.prepared.model.metadata.model_ref.into_string(),
        data: result
            .response
            .data
            .into_iter()
            .map(|item| RerankItem {
                index: item.index,
                score: item.score,
            })
            .collect(),
    }
}

fn rerank_error(error: KernelError) -> RestError {
    match error {
        KernelError::ModelStoreUnavailable(message) => {
            RestError::store_lookup("rerank_model_failed", message)
        }
        KernelError::UnsupportedTarget(message) => {
            RestError::bad_request("unsupported_target", message)
        }
        KernelError::RuntimeStateUnavailable(message) => {
            RestError::internal("rerank_runtime_unavailable", message)
        }
        KernelError::RerankRuntimeUnavailable(message) => {
            RestError::internal("rerank_runtime_failed", message)
        }
        other => RestError::kernel("rerank_failed", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_alias_matches_huggingface_repo_or_repo_name() {
        assert!(model_alias_matches(
            "BAAI/bge-reranker-base",
            Some("BAAI/bge-reranker-base")
        ));
        assert!(model_alias_matches(
            "bge-reranker-base",
            Some("BAAI/bge-reranker-base")
        ));
        assert!(!model_alias_matches(
            "different",
            Some("BAAI/bge-reranker-base")
        ));
    }
}
