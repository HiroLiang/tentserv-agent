use std::{io, path::PathBuf};

use tentgent_core::{
    adapter::{AdapterBindOutcome, AdapterError, AdapterImportOutcome, AdapterManager},
    dataset::{DatasetError, DatasetImportOutcome, DatasetManager},
    model::{ImportOutcome, ModelError, ModelManager},
};
use tokio::task;

use crate::{
    app::DaemonHttpState,
    dto::{
        AdapterBindRequest, AdapterImportRequest, AdapterMutationResponse, AdapterPullRequest,
        DatasetMutationResponse, ErrorResponse, ModelMutationResponse, StoreImportRequest,
        StoreMutationItem, StorePullRequest,
    },
    http::{HttpRequest, HttpResponse},
    response::{bad_request_response, json_response, parse_json_body},
};

use super::store::{
    adapter_inspection_item, dataset_inspection_item, model_inspection_item, path_string,
};

pub(crate) async fn import_model_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
) -> HttpResponse {
    let body = match parse_json_body::<StoreImportRequest>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let path = match canonical_import_path(&body.path) {
        Ok(path) => path,
        Err(response) => return response,
    };
    let home = state.home_dir().to_path_buf();

    let result = task::spawn_blocking(move || {
        let manager = ModelManager::new_with_home(Some(&home))?;
        let outcome = manager.add_path(&path)?;
        let inspection = manager.inspect(&outcome.metadata.model_ref)?;
        Ok::<_, ModelError>((outcome, inspection))
    })
    .await;

    match result {
        Ok(Ok((outcome, inspection))) => json_response(
            200,
            ModelMutationResponse {
                model: model_inspection_item(inspection),
                mutation: model_mutation_item("import", outcome),
            },
        ),
        Ok(Err(error)) => model_mutation_error_response(error),
        Err(error) => blocking_join_error_response(error),
    }
}

pub(crate) async fn pull_model_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
) -> HttpResponse {
    let body = match parse_json_body::<StorePullRequest>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let repo_id = match normalize_repo_id(&body.repo_id) {
        Ok(repo_id) => repo_id,
        Err(response) => return response,
    };
    let revision = match normalize_revision(body.revision) {
        Ok(revision) => revision,
        Err(response) => return response,
    };
    let home = state.home_dir().to_path_buf();

    let result = task::spawn_blocking(move || {
        let manager = ModelManager::new_with_home(Some(&home))?;
        let outcome = manager.pull_hf(&repo_id, revision.as_deref())?;
        let inspection = manager.inspect(&outcome.metadata.model_ref)?;
        Ok::<_, ModelError>((outcome, inspection))
    })
    .await;

    match result {
        Ok(Ok((outcome, inspection))) => json_response(
            200,
            ModelMutationResponse {
                model: model_inspection_item(inspection),
                mutation: model_mutation_item("pull", outcome),
            },
        ),
        Ok(Err(error)) => model_mutation_error_response(error),
        Err(error) => blocking_join_error_response(error),
    }
}

pub(crate) async fn import_adapter_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
) -> HttpResponse {
    let body = match parse_json_body::<AdapterImportRequest>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let path = match canonical_import_path(&body.path) {
        Ok(path) => path,
        Err(response) => return response,
    };
    let base_model_ref = match normalize_optional_ref(body.base_model_ref, "base_model_ref") {
        Ok(reference) => reference,
        Err(response) => return response,
    };
    let home = state.home_dir().to_path_buf();

    let result = task::spawn_blocking(move || {
        let manager = AdapterManager::new_with_home(Some(&home))?;
        let outcome = manager.add_path(&path, base_model_ref.as_deref())?;
        let inspection = manager.inspect(&outcome.metadata.adapter_ref)?;
        Ok::<_, AdapterError>((outcome, inspection))
    })
    .await;

    match result {
        Ok(Ok((outcome, inspection))) => json_response(
            200,
            AdapterMutationResponse {
                adapter: adapter_inspection_item(inspection),
                mutation: adapter_mutation_item("import", outcome),
            },
        ),
        Ok(Err(error)) => adapter_mutation_error_response(error),
        Err(error) => blocking_join_error_response(error),
    }
}

pub(crate) async fn pull_adapter_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
) -> HttpResponse {
    let body = match parse_json_body::<AdapterPullRequest>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let repo_id = match normalize_repo_id(&body.repo_id) {
        Ok(repo_id) => repo_id,
        Err(response) => return response,
    };
    let revision = match normalize_revision(body.revision) {
        Ok(revision) => revision,
        Err(response) => return response,
    };
    let base_model_ref = match normalize_optional_ref(body.base_model_ref, "base_model_ref") {
        Ok(reference) => reference,
        Err(response) => return response,
    };
    let home = state.home_dir().to_path_buf();

    let result = task::spawn_blocking(move || {
        let manager = AdapterManager::new_with_home(Some(&home))?;
        let outcome = manager.pull_hf(&repo_id, revision.as_deref(), base_model_ref.as_deref())?;
        let inspection = manager.inspect(&outcome.metadata.adapter_ref)?;
        Ok::<_, AdapterError>((outcome, inspection))
    })
    .await;

    match result {
        Ok(Ok((outcome, inspection))) => json_response(
            200,
            AdapterMutationResponse {
                adapter: adapter_inspection_item(inspection),
                mutation: adapter_mutation_item("pull", outcome),
            },
        ),
        Ok(Err(error)) => adapter_mutation_error_response(error),
        Err(error) => blocking_join_error_response(error),
    }
}

pub(crate) async fn bind_adapter_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
    reference: &str,
) -> HttpResponse {
    let body = match parse_json_body::<AdapterBindRequest>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let base_model_ref = match normalize_required_ref(body.base_model_ref, "base_model_ref") {
        Ok(reference) => reference,
        Err(response) => return response,
    };
    let adapter_ref = reference.to_string();
    let home = state.home_dir().to_path_buf();

    let result = task::spawn_blocking(move || {
        let manager = AdapterManager::new_with_home(Some(&home))?;
        let outcome = manager.bind_to_model(&adapter_ref, &base_model_ref)?;
        let inspection = manager.inspect(&outcome.metadata.adapter_ref)?;
        Ok::<_, AdapterError>((outcome, inspection))
    })
    .await;

    match result {
        Ok(Ok((outcome, inspection))) => json_response(
            200,
            AdapterMutationResponse {
                adapter: adapter_inspection_item(inspection),
                mutation: adapter_bind_mutation_item(outcome),
            },
        ),
        Ok(Err(error)) => adapter_mutation_error_response(error),
        Err(error) => blocking_join_error_response(error),
    }
}

pub(crate) async fn import_dataset_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
) -> HttpResponse {
    let body = match parse_json_body::<StoreImportRequest>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let path = match canonical_import_path(&body.path) {
        Ok(path) => path,
        Err(response) => return response,
    };
    let home = state.home_dir().to_path_buf();

    let result = task::spawn_blocking(move || {
        let manager = DatasetManager::new_with_home(Some(&home))?;
        let outcome = manager.add_path(&path)?;
        let inspection = manager.inspect(&outcome.metadata.dataset_ref)?;
        Ok::<_, DatasetError>((outcome, inspection))
    })
    .await;

    match result {
        Ok(Ok((outcome, inspection))) => json_response(
            200,
            DatasetMutationResponse {
                dataset: dataset_inspection_item(inspection),
                mutation: dataset_mutation_item("import", outcome),
            },
        ),
        Ok(Err(error)) => dataset_mutation_error_response(error),
        Err(error) => blocking_join_error_response(error),
    }
}

fn model_mutation_item(kind: &'static str, outcome: ImportOutcome) -> StoreMutationItem {
    StoreMutationItem {
        kind,
        deduplicated: Some(outcome.deduplicated),
        store_path: Some(path_string(&outcome.store_path)),
        source_index_path: Some(path_string(&outcome.source_index_path)),
        base_index_path: None,
        base_model_ref: None,
    }
}

fn adapter_mutation_item(kind: &'static str, outcome: AdapterImportOutcome) -> StoreMutationItem {
    StoreMutationItem {
        kind,
        deduplicated: Some(outcome.deduplicated),
        store_path: Some(path_string(&outcome.store_path)),
        source_index_path: Some(path_string(&outcome.source_index_path)),
        base_index_path: outcome.base_index_path.as_deref().map(path_string),
        base_model_ref: None,
    }
}

fn adapter_bind_mutation_item(outcome: AdapterBindOutcome) -> StoreMutationItem {
    StoreMutationItem {
        kind: "bind",
        deduplicated: None,
        store_path: Some(path_string(&outcome.store_path)),
        source_index_path: None,
        base_index_path: None,
        base_model_ref: outcome.metadata.base_model_ref,
    }
}

fn dataset_mutation_item(kind: &'static str, outcome: DatasetImportOutcome) -> StoreMutationItem {
    StoreMutationItem {
        kind,
        deduplicated: Some(outcome.deduplicated),
        store_path: Some(path_string(&outcome.store_path)),
        source_index_path: Some(path_string(&outcome.source_index_path)),
        base_index_path: None,
        base_model_ref: None,
    }
}

fn canonical_import_path(value: &str) -> Result<PathBuf, HttpResponse> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(bad_request_response("path must not be blank"));
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        return Err(bad_request_response(
            "path must be an absolute path on the daemon host filesystem",
        ));
    }
    path.canonicalize().map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => json_response(
            404,
            ErrorResponse {
                error: "path_not_found",
                message: format!("path `{trimmed}` was not found on the daemon host"),
            },
        ),
        _ => json_response(
            500,
            ErrorResponse {
                error: "store_mutation_failed",
                message: format!("failed to canonicalize path `{trimmed}`: {error}"),
            },
        ),
    })
}

fn normalize_repo_id(value: &str) -> Result<String, HttpResponse> {
    let repo_id = value.trim();
    if repo_id.is_empty() {
        return Err(bad_request_response("repo_id must not be blank"));
    }
    if repo_id.contains("://")
        || repo_id.contains("/tree/")
        || repo_id.starts_with('/')
        || repo_id.contains('\\')
    {
        return Err(bad_request_response(
            "repo_id must be a Hugging Face repo id such as `owner/name`, not a URL or path",
        ));
    }

    let segments = repo_id.split('/').collect::<Vec<_>>();
    if segments.len() != 2 || segments.iter().any(|segment| invalid_repo_segment(segment)) {
        return Err(bad_request_response(
            "repo_id must be a Hugging Face repo id such as `owner/name`",
        ));
    }

    Ok(repo_id.to_string())
}

fn invalid_repo_segment(segment: &str) -> bool {
    segment.is_empty()
        || segment == "."
        || segment == ".."
        || segment
            .chars()
            .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.'))
}

fn normalize_revision(value: Option<String>) -> Result<Option<String>, HttpResponse> {
    match value {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Err(bad_request_response(
                    "revision must not be blank when provided",
                ))
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        None => Ok(None),
    }
}

fn normalize_optional_ref(
    value: Option<String>,
    field: &'static str,
) -> Result<Option<String>, HttpResponse> {
    match value {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else if trimmed.contains('/') {
                Err(bad_request_response(format!(
                    "{field} must be a managed ref, not a path"
                )))
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        None => Ok(None),
    }
}

fn normalize_required_ref(value: String, field: &'static str) -> Result<String, HttpResponse> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(bad_request_response(format!("{field} must not be blank")));
    }
    if trimmed.contains('/') {
        return Err(bad_request_response(format!(
            "{field} must be a managed ref, not a path"
        )));
    }
    Ok(trimmed.to_string())
}

fn model_mutation_error_response(error: ModelError) -> HttpResponse {
    match error {
        ModelError::MissingPath(path) => path_not_found_response(path),
        ModelError::UnsupportedPath(_) | ModelError::UnsupportedLayout { .. } => json_response(
            400,
            ErrorResponse {
                error: "unsupported_layout",
                message: error.to_string(),
            },
        ),
        ModelError::NotFound(reference) => json_response(
            404,
            ErrorResponse {
                error: "not_found",
                message: format!("model reference `{reference}` was not found"),
            },
        ),
        ModelError::AmbiguousRef(reference) => json_response(
            409,
            ErrorResponse {
                error: "ambiguous_ref",
                message: format!("model reference `{reference}` is ambiguous; use a longer prefix"),
            },
        ),
        ModelError::InUse { server_refs, .. } => json_response(
            409,
            ErrorResponse {
                error: "in_use",
                message: format!("model is still referenced by server spec(s): {server_refs}"),
            },
        ),
        ModelError::Auth(error) => json_response(
            409,
            ErrorResponse {
                error: "provider_auth_failed",
                message: error.to_string(),
            },
        ),
        ModelError::HfHelper { message } | ModelError::HfHelperOutput { message } => json_response(
            502,
            ErrorResponse {
                error: "pull_failed",
                message,
            },
        ),
        other => store_mutation_failed_response("models", other),
    }
}

fn adapter_mutation_error_response(error: AdapterError) -> HttpResponse {
    match error {
        AdapterError::MissingPath(path) => path_not_found_response(path),
        AdapterError::UnsupportedPath(_) | AdapterError::UnsupportedLayout { .. } => json_response(
            400,
            ErrorResponse {
                error: "unsupported_layout",
                message: error.to_string(),
            },
        ),
        AdapterError::NotFound(reference) => json_response(
            404,
            ErrorResponse {
                error: "not_found",
                message: format!("adapter reference `{reference}` was not found"),
            },
        ),
        AdapterError::AmbiguousRef(reference) => json_response(
            409,
            ErrorResponse {
                error: "ambiguous_ref",
                message: format!(
                    "adapter reference `{reference}` is ambiguous; use a longer prefix"
                ),
            },
        ),
        AdapterError::InUse { server_refs, .. } => json_response(
            409,
            ErrorResponse {
                error: "in_use",
                message: format!("adapter is still referenced by server spec(s): {server_refs}"),
            },
        ),
        AdapterError::BaseModelMismatch { .. } | AdapterError::BaseRevisionMismatch { .. } => {
            json_response(
                409,
                ErrorResponse {
                    error: "base_model_mismatch",
                    message: error.to_string(),
                },
            )
        }
        AdapterError::Auth(error) => json_response(
            409,
            ErrorResponse {
                error: "provider_auth_failed",
                message: error.to_string(),
            },
        ),
        AdapterError::Model(error) => model_mutation_error_response(error),
        AdapterError::HfHelper { message } | AdapterError::HfHelperOutput { message } => {
            json_response(
                502,
                ErrorResponse {
                    error: "pull_failed",
                    message,
                },
            )
        }
        other => store_mutation_failed_response("adapters", other),
    }
}

fn dataset_mutation_error_response(error: DatasetError) -> HttpResponse {
    match error {
        DatasetError::MissingPath(path) => path_not_found_response(path),
        DatasetError::UnsupportedPath(_) | DatasetError::UnsupportedLayout { .. } => json_response(
            400,
            ErrorResponse {
                error: "unsupported_layout",
                message: error.to_string(),
            },
        ),
        DatasetError::NotFound(reference) => json_response(
            404,
            ErrorResponse {
                error: "not_found",
                message: format!("dataset reference `{reference}` was not found"),
            },
        ),
        DatasetError::AmbiguousRef(reference) => json_response(
            409,
            ErrorResponse {
                error: "ambiguous_ref",
                message: format!(
                    "dataset reference `{reference}` is ambiguous; use a longer prefix"
                ),
            },
        ),
        other => store_mutation_failed_response("datasets", other),
    }
}

fn path_not_found_response(path: PathBuf) -> HttpResponse {
    json_response(
        404,
        ErrorResponse {
            error: "path_not_found",
            message: format!("path `{}` was not found on the daemon host", path.display()),
        },
    )
}

fn store_mutation_failed_response(
    context: &'static str,
    error: impl std::fmt::Display,
) -> HttpResponse {
    json_response(
        500,
        ErrorResponse {
            error: "store_mutation_failed",
            message: format!("failed to mutate {context}: {error}"),
        },
    )
}

fn blocking_join_error_response(error: task::JoinError) -> HttpResponse {
    json_response(
        500,
        ErrorResponse {
            error: "store_mutation_failed",
            message: format!("store mutation task failed: {error}"),
        },
    )
}
