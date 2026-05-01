use std::{
    io,
    path::{Path, PathBuf},
};

use tentgent_core::dataset::{
    render_dataset_template, validate_dataset_path, DatasetDiffOutcome, DatasetDiffSide,
    DatasetError, DatasetExportOutcome, DatasetManager,
    DatasetTemplateRequest as CoreTemplateRequest, DatasetValidationOutcome,
    DATASET_TEMPLATE_VERSION,
};
use tokio::task;

use crate::{
    app::DaemonHttpState,
    dto::{
        DatasetDiffFileItem, DatasetDiffItem, DatasetDiffRequest, DatasetDiffResponse,
        DatasetDiffSideItem, DatasetDiffSummaryItem, DatasetExportItem, DatasetExportRequest,
        DatasetExportResponse, DatasetTemplateRequestBody, DatasetTemplateResponse,
        DatasetToolSourceItem, DatasetValidateRequest, DatasetValidationErrorItem,
        DatasetValidationResponse, DatasetValidationSplitItem, ErrorResponse,
    },
    http::{HttpRequest, HttpResponse},
    response::{bad_request_response, json_response, parse_json_body},
};

use super::store::{dataset_inspection_item, path_string};

const DIFF_FILE_LIMIT: usize = 500;

pub(crate) async fn validate_dataset_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
) -> HttpResponse {
    let body = match parse_json_body::<DatasetValidateRequest>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let source = match resolve_validate_source(state, body) {
        Ok(source) => source,
        Err(response) => return response,
    };

    let path = source.path.clone();
    let result = task::spawn_blocking(move || validate_dataset_path(&path)).await;

    match result {
        Ok(Ok(outcome)) => json_response(200, validation_response(source.item, outcome)),
        Ok(Err(error)) => dataset_tool_error_response(error),
        Err(error) => blocking_join_error_response(error),
    }
}

pub(crate) fn dataset_template_response(request: &HttpRequest) -> HttpResponse {
    let body = match parse_json_body::<DatasetTemplateRequestBody>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let template = CoreTemplateRequest::new(body.task, body.language);
    let content = render_dataset_template(&template);

    json_response(
        200,
        DatasetTemplateResponse {
            template_version: DATASET_TEMPLATE_VERSION,
            task: template.task,
            language: template.language,
            content,
        },
    )
}

pub(crate) async fn export_dataset_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
    reference: &str,
) -> HttpResponse {
    let body = match parse_json_body::<DatasetExportRequest>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let output_path = match absolute_path_from_request(&body.output_path, "output_path") {
        Ok(path) => path,
        Err(response) => return response,
    };
    let reference = reference.to_string();
    let home = state.home_dir().to_path_buf();

    let result = task::spawn_blocking(move || {
        let manager = DatasetManager::new_with_home(Some(&home))?;
        let outcome = manager.export_to(&reference, &output_path)?;
        let inspection = manager.inspect(&outcome.metadata.dataset_ref)?;
        Ok::<_, DatasetError>((outcome, inspection))
    })
    .await;

    match result {
        Ok(Ok((outcome, inspection))) => json_response(
            200,
            DatasetExportResponse {
                dataset: dataset_inspection_item(inspection),
                export: dataset_export_item(outcome),
            },
        ),
        Ok(Err(error)) => dataset_tool_error_response(error),
        Err(error) => blocking_join_error_response(error),
    }
}

pub(crate) async fn diff_dataset_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
    left_reference: &str,
) -> HttpResponse {
    let body = match parse_json_body::<DatasetDiffRequest>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let right = match resolve_diff_right(body) {
        Ok(right) => right,
        Err(response) => return response,
    };
    let left_reference = left_reference.to_string();
    let home = state.home_dir().to_path_buf();

    let result = task::spawn_blocking(move || {
        let manager = DatasetManager::new_with_home(Some(&home))?;
        match right {
            DiffRight::Dataset(reference) => manager.diff_refs(&left_reference, &reference),
            DiffRight::Path(path) => manager.diff_ref_to_path(&left_reference, &path),
        }
    })
    .await;

    match result {
        Ok(Ok(outcome)) => json_response(200, dataset_diff_response(outcome)),
        Ok(Err(error)) => dataset_tool_error_response(error),
        Err(error) => blocking_join_error_response(error),
    }
}

struct ValidateSource {
    path: PathBuf,
    item: DatasetToolSourceItem,
}

enum DiffRight {
    Dataset(String),
    Path(PathBuf),
}

fn resolve_validate_source(
    state: &DaemonHttpState,
    body: DatasetValidateRequest,
) -> Result<ValidateSource, HttpResponse> {
    let dataset_ref = normalize_optional_ref(body.dataset_ref, "dataset_ref")?;
    match (normalize_optional_path(body.path), dataset_ref) {
        (Some(_), Some(_)) => Err(bad_request_response(
            "exactly one of `path` or `dataset_ref` is required",
        )),
        (None, None) => Err(bad_request_response(
            "exactly one of `path` or `dataset_ref` is required",
        )),
        (Some(path), None) => {
            let path = canonical_input_path(&path, "path")?;
            Ok(ValidateSource {
                item: DatasetToolSourceItem {
                    kind: "path",
                    path: path_string(&path),
                    dataset_ref: None,
                    short_ref: None,
                },
                path,
            })
        }
        (None, Some(reference)) => {
            let manager = DatasetManager::open_readonly_with_home(Some(state.home_dir()))
                .map_err(dataset_tool_error_response)?;
            let inspection = manager
                .inspect(&reference)
                .map_err(dataset_tool_error_response)?;
            Ok(ValidateSource {
                path: inspection.source_path.clone(),
                item: DatasetToolSourceItem {
                    kind: "dataset",
                    path: path_string(&inspection.source_path),
                    dataset_ref: Some(inspection.metadata.dataset_ref),
                    short_ref: Some(inspection.metadata.short_ref),
                },
            })
        }
    }
}

fn resolve_diff_right(body: DatasetDiffRequest) -> Result<DiffRight, HttpResponse> {
    let right_dataset_ref = normalize_optional_ref(body.right_dataset_ref, "right_dataset_ref")?;
    match (right_dataset_ref, normalize_optional_path(body.right_path)) {
        (Some(_), Some(_)) => Err(bad_request_response(
            "exactly one of `right_dataset_ref` or `right_path` is required",
        )),
        (None, None) => Err(bad_request_response(
            "exactly one of `right_dataset_ref` or `right_path` is required",
        )),
        (Some(reference), None) => Ok(DiffRight::Dataset(reference)),
        (None, Some(path)) => canonical_input_path(&path, "right_path").map(DiffRight::Path),
    }
}

fn normalize_optional_path(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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

fn canonical_input_path(value: &str, field: &'static str) -> Result<PathBuf, HttpResponse> {
    let path = absolute_path_from_request(value, field)?;
    path.canonicalize().map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => path_not_found_response(&path),
        _ => json_response(
            500,
            ErrorResponse {
                error: "dataset_tool_failed",
                message: format!(
                    "failed to canonicalize {field} `{}`: {error}",
                    path.display()
                ),
            },
        ),
    })
}

fn absolute_path_from_request(value: &str, field: &'static str) -> Result<PathBuf, HttpResponse> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(bad_request_response(format!("{field} must not be blank")));
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        return Err(bad_request_response(format!(
            "{field} must be an absolute path on the daemon host filesystem"
        )));
    }
    Ok(path)
}

fn validation_response(
    source: DatasetToolSourceItem,
    outcome: DatasetValidationOutcome,
) -> DatasetValidationResponse {
    DatasetValidationResponse {
        valid: outcome.is_valid(),
        source,
        target: outcome.target_kind.as_str().to_string(),
        tuning_ready: outcome.tuning_ready,
        records: outcome.record_count(),
        errors_count: outcome.errors.len(),
        splits: outcome
            .splits
            .into_iter()
            .map(|split| DatasetValidationSplitItem {
                name: split.name,
                path: path_string(&split.path),
                records: split.records,
                errors: split.errors,
            })
            .collect(),
        warnings: outcome.warnings,
        errors: outcome
            .errors
            .into_iter()
            .map(|error| DatasetValidationErrorItem {
                path: path_string(&error.path),
                line: error.line,
                message: error.message,
            })
            .collect(),
    }
}

fn dataset_export_item(outcome: DatasetExportOutcome) -> DatasetExportItem {
    DatasetExportItem {
        output_path: path_string(&outcome.destination_path),
        managed_source_path: path_string(&outcome.managed_source_path),
        file_count: outcome.metadata.file_count,
        total_bytes: outcome.metadata.total_bytes,
    }
}

fn dataset_diff_response(outcome: DatasetDiffOutcome) -> DatasetDiffResponse {
    let total_files = outcome.diff.files.len();
    let files = outcome
        .diff
        .files
        .into_iter()
        .take(DIFF_FILE_LIMIT)
        .map(|file| DatasetDiffFileItem {
            status: file.status.as_str().to_string(),
            relative_path: file.relative_path,
            left_size_bytes: file.left_size_bytes,
            right_size_bytes: file.right_size_bytes,
        })
        .collect();

    DatasetDiffResponse {
        left: diff_side_item(outcome.left),
        right: diff_side_item(outcome.right),
        diff: DatasetDiffItem {
            summary: DatasetDiffSummaryItem {
                added: outcome.diff.summary.added,
                removed: outcome.diff.summary.removed,
                modified: outcome.diff.summary.modified,
                unchanged: outcome.diff.summary.unchanged,
                left_total_bytes: outcome.diff.summary.left_total_bytes,
                right_total_bytes: outcome.diff.summary.right_total_bytes,
            },
            files,
            file_limit: DIFF_FILE_LIMIT,
            truncated: total_files > DIFF_FILE_LIMIT,
        },
    }
}

fn diff_side_item(side: DatasetDiffSide) -> DatasetDiffSideItem {
    DatasetDiffSideItem {
        label: side.label,
        short_ref: side.short_ref,
        path: side.path.as_deref().map(path_string),
        tuning_ready: side.tuning_ready,
        splits: side.splits,
    }
}

fn dataset_tool_error_response(error: DatasetError) -> HttpResponse {
    match error {
        DatasetError::MissingPath(path) => path_not_found_response(&path),
        DatasetError::UnsupportedPath(_) | DatasetError::UnsupportedLayout { .. } => json_response(
            400,
            ErrorResponse {
                error: "unsupported_layout",
                message: error.to_string(),
            },
        ),
        DatasetError::ExportDestinationNotEmpty(_)
        | DatasetError::ExportDestinationNotDirectory(_) => json_response(
            409,
            ErrorResponse {
                error: "output_exists",
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
        other => json_response(
            500,
            ErrorResponse {
                error: "dataset_tool_failed",
                message: format!("dataset tool failed: {other}"),
            },
        ),
    }
}

fn path_not_found_response(path: &Path) -> HttpResponse {
    json_response(
        404,
        ErrorResponse {
            error: "path_not_found",
            message: format!("path `{}` was not found on the daemon host", path.display()),
        },
    )
}

fn blocking_join_error_response(error: task::JoinError) -> HttpResponse {
    json_response(
        500,
        ErrorResponse {
            error: "dataset_tool_failed",
            message: format!("dataset tool task failed: {error}"),
        },
    )
}
