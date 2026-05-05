use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tentgent_core::{
    dataset::{DatasetError, DatasetManager},
    dataset_runtime::{
        preflight_dataset_provider_auth, run_dataset_eval_runtime,
        run_dataset_synth_prompt_runtime, run_dataset_synth_runtime, DatasetEvalRuntimeRequest,
        DatasetRuntimeDebug, DatasetRuntimeError, DatasetSynthCounts,
        DatasetSynthPromptRuntimeRequest, DatasetSynthRuntimeRequest,
    },
};

use crate::{
    app::DaemonHttpState,
    dto::{
        DatasetEvalRequest, DatasetEvalResponse, DatasetRuntimeDebugItem, DatasetSynthPromptItem,
        DatasetSynthPromptResponse, DatasetSynthRequest, DatasetSynthResponse, ErrorResponse,
    },
    http::{HttpBody, HttpRequest, HttpResponse},
    jobs::{JobRegistry, JobResponse},
    response::{bad_request_response, json_response, parse_json_body},
};

use super::store::path_string;

const SPEC_CONTENT_MAX_BYTES: usize = 256 * 1024;
const INPUT_CONTENT_MAX_BYTES: usize = 10 * 1024 * 1024;

pub(crate) async fn synth_dataset_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
) -> HttpResponse {
    let body = match parse_json_body::<DatasetSynthRequest>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let source = match resolve_synth_source(&body) {
        Ok(source) => source,
        Err(response) => return response,
    };
    let (split, counts) = match synth_counts(&body) {
        Ok(counts) => counts,
        Err(response) => return response,
    };

    if body.print_prompt {
        if let Err(response) = reject_prompt_provider_fields(&body) {
            return response;
        }
        let spec = match materialize_synth_spec(state.home_dir(), &source) {
            Ok(spec) => spec,
            Err(response) => return response,
        };
        let result = run_dataset_synth_prompt_runtime(DatasetSynthPromptRuntimeRequest {
            brief: source.brief.clone(),
            spec,
            split: split.clone(),
            counts,
        })
        .await;
        return match result {
            Ok(content) => json_response(
                200,
                DatasetSynthPromptResponse {
                    prompt: DatasetSynthPromptItem {
                        content,
                        split,
                        source_kind: source.kind.to_string(),
                    },
                },
            ),
            Err(error) => dataset_runtime_error_response(error, "dataset_synth_failed"),
        };
    }

    let provider = match required_string(body.provider.as_deref(), "provider") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let model = match required_string(body.model.as_deref(), "model") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let output_path = match required_absolute_path(body.output_path.as_deref(), "output_path") {
        Ok(path) => path,
        Err(response) => return response,
    };
    if let Err(response) = ensure_missing_or_empty_dir(&output_path) {
        return response;
    }
    let max_tokens = match optional_positive_u32(body.max_tokens, "max_tokens") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let temperature = match optional_f32(body.temperature, 0.0, "temperature") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let timeout_seconds = match optional_range_f32(body.timeout_seconds, 180.0, "timeout_seconds") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let retries = match optional_max_u32(body.retries, 1, 10, "retries") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let auth = match preflight_dataset_provider_auth(&provider, "dataset synth").await {
        Ok(auth) => auth,
        Err(error) => return dataset_runtime_error_response(error, "dataset_synth_failed"),
    };
    let spec = match materialize_synth_spec(state.home_dir(), &source) {
        Ok(spec) => spec,
        Err(response) => return response,
    };

    let result = run_dataset_synth_runtime(DatasetSynthRuntimeRequest {
        auth,
        model,
        output: output_path,
        brief: source.brief,
        spec,
        split,
        counts,
        max_tokens,
        temperature,
        timeout_seconds,
        retries,
    })
    .await;

    match result {
        Ok(outcome) => json_response(
            200,
            DatasetSynthResponse {
                synth: outcome.outcome,
                progress_events: outcome.progress_events,
                progress_truncated: outcome.progress_truncated,
            },
        ),
        Err(error) => dataset_runtime_error_response(error, "dataset_synth_failed"),
    }
}

pub(crate) async fn eval_dataset_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
) -> HttpResponse {
    let body = match parse_json_body::<DatasetEvalRequest>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let provider = match required_string(body.provider.as_deref(), "provider") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let model = match required_string(body.model.as_deref(), "model") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let output_path = match required_absolute_path(body.output_path.as_deref(), "output_path") {
        Ok(path) => path,
        Err(response) => return response,
    };
    if let Err(response) = ensure_missing_or_empty_dir(&output_path) {
        return response;
    }
    let input = match resolve_eval_input(state.home_dir(), &body) {
        Ok(path) => path,
        Err(response) => return response,
    };
    let split = normalize_split(
        body.split.as_deref(),
        &["train", "valid", "test", "eval_cases", "all"],
    );
    let split = match split {
        Ok(split) => split.unwrap_or_else(|| "train".to_string()),
        Err(response) => return response,
    };
    let max_records = match optional_positive_u32(body.max_records, "max_records") {
        Ok(Some(value)) => value,
        Ok(None) => 20,
        Err(response) => return response,
    };
    let criteria = match optional_nonblank(body.criteria, "criteria") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let max_tokens = match optional_positive_u32(body.max_tokens, "max_tokens") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let temperature = match optional_f32(body.temperature, 0.0, "temperature") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let timeout_seconds = match optional_range_f32(body.timeout_seconds, 180.0, "timeout_seconds") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let auth = match preflight_dataset_provider_auth(&provider, "dataset eval").await {
        Ok(auth) => auth,
        Err(error) => return dataset_runtime_error_response(error, "dataset_eval_failed"),
    };

    let result = run_dataset_eval_runtime(DatasetEvalRuntimeRequest {
        auth,
        model,
        input,
        output: output_path,
        split,
        max_records,
        criteria,
        max_tokens,
        temperature,
        timeout_seconds,
    })
    .await;

    match result {
        Ok(evaluation) => json_response(200, DatasetEvalResponse { evaluation }),
        Err(error) => dataset_runtime_error_response(error, "dataset_eval_failed"),
    }
}

pub(crate) fn synth_dataset_job_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
) -> HttpResponse {
    let body = match parse_json_body::<DatasetSynthRequest>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    if body.print_prompt {
        return bad_request_response("print_prompt is synchronous; use /v1/datasets/synth");
    }
    if let Err(response) = validate_synth_job_request(&body) {
        return response;
    }
    let label = body
        .brief
        .as_deref()
        .map(|brief| format!("synth {}", brief.trim()))
        .or_else(|| {
            body.spec_path
                .as_deref()
                .map(|path| format!("synth {path}"))
        })
        .unwrap_or_else(|| "synth dataset".to_string());
    let registry = state.jobs().clone();
    let job = registry.create("dataset_synth", label, "datasets", ["datasets".to_string()]);
    let job_id = job.job_id.clone();
    let state = state.clone();
    let request = request.clone();

    tokio::spawn(async move {
        registry.start(&job_id, "running dataset synth");
        let response = synth_dataset_response(&state, &request).await;
        finish_dataset_job(&registry, &job_id, response, "synth");
    });

    json_response(202, JobResponse { job })
}

pub(crate) fn eval_dataset_job_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
) -> HttpResponse {
    let body = match parse_json_body::<DatasetEvalRequest>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    if let Err(response) = validate_eval_job_request(state.home_dir(), &body) {
        return response;
    }
    let label = body
        .dataset_ref
        .as_deref()
        .map(|reference| format!("eval {reference}"))
        .or_else(|| {
            body.input_path
                .as_deref()
                .map(|path| format!("eval {path}"))
        })
        .unwrap_or_else(|| "eval dataset".to_string());
    let registry = state.jobs().clone();
    let job = registry.create("dataset_eval", label, "datasets", ["datasets".to_string()]);
    let job_id = job.job_id.clone();
    let state = state.clone();
    let request = request.clone();

    tokio::spawn(async move {
        registry.start(&job_id, "running dataset eval");
        let response = eval_dataset_response(&state, &request).await;
        finish_dataset_job(&registry, &job_id, response, "eval");
    });

    json_response(202, JobResponse { job })
}

struct SynthSource {
    kind: &'static str,
    brief: Option<String>,
    spec_content: Option<String>,
    spec_path: Option<PathBuf>,
}

fn resolve_synth_source(body: &DatasetSynthRequest) -> Result<SynthSource, HttpResponse> {
    let brief = optional_nonblank(body.brief.clone(), "brief")?;
    let spec_content = optional_content(body.spec_content.clone(), "spec_content")?;
    let spec_path = optional_nonblank(body.spec_path.clone(), "spec_path")?;
    let selected = [brief.is_some(), spec_content.is_some(), spec_path.is_some()]
        .into_iter()
        .filter(|selected| *selected)
        .count();
    if selected != 1 {
        return Err(bad_request_response(
            "exactly one of `brief`, `spec_content`, or `spec_path` is required",
        ));
    }
    if let Some(brief) = brief {
        return Ok(SynthSource {
            kind: "brief",
            brief: Some(brief),
            spec_content: None,
            spec_path: None,
        });
    }
    if let Some(content) = spec_content {
        if content.len() > SPEC_CONTENT_MAX_BYTES {
            return Err(bad_request_response(format!(
                "`spec_content` must be at most {SPEC_CONTENT_MAX_BYTES} bytes"
            )));
        }
        return Ok(SynthSource {
            kind: "spec_content",
            brief: None,
            spec_content: Some(content),
            spec_path: None,
        });
    }
    let spec_path = required_absolute_path(spec_path.as_deref(), "spec_path")?;
    let spec_path = canonical_input_path(&spec_path)?;
    Ok(SynthSource {
        kind: "spec_path",
        brief: None,
        spec_content: None,
        spec_path: Some(spec_path),
    })
}

fn validate_synth_job_request(body: &DatasetSynthRequest) -> Result<(), HttpResponse> {
    let source = resolve_synth_source(body)?;
    let _ = synth_counts(body)?;
    let _ = required_string(body.provider.as_deref(), "provider")?;
    let _ = required_string(body.model.as_deref(), "model")?;
    let output_path = required_absolute_path(body.output_path.as_deref(), "output_path")?;
    ensure_missing_or_empty_dir(&output_path)?;
    let _ = optional_positive_u32(body.max_tokens, "max_tokens")?;
    let _ = optional_f32(body.temperature, 0.0, "temperature")?;
    let _ = optional_range_f32(body.timeout_seconds, 180.0, "timeout_seconds")?;
    let _ = optional_max_u32(body.retries, 1, 10, "retries")?;
    if let Some(spec_path) = &source.spec_path {
        let _ = canonical_input_path(spec_path)?;
    }
    Ok(())
}

fn materialize_synth_spec(
    home: &Path,
    source: &SynthSource,
) -> Result<Option<PathBuf>, HttpResponse> {
    if let Some(content) = &source.spec_content {
        return stage_content(home, "synth-spec", "spec.md", content)
            .map(Some)
            .map_err(dataset_runtime_io_error_response);
    }
    Ok(source.spec_path.clone())
}

fn resolve_eval_input(home: &Path, body: &DatasetEvalRequest) -> Result<PathBuf, HttpResponse> {
    let dataset_ref = optional_nonblank(body.dataset_ref.clone(), "dataset_ref")?;
    let input_content = optional_content(body.input_content.clone(), "input_content")?;
    let input_path = optional_nonblank(body.input_path.clone(), "input_path")?;
    let selected = [
        dataset_ref.is_some(),
        input_content.is_some(),
        input_path.is_some(),
    ]
    .into_iter()
    .filter(|selected| *selected)
    .count();
    if selected != 1 {
        return Err(bad_request_response(
            "exactly one of `dataset_ref`, `input_content`, or `input_path` is required",
        ));
    }
    if input_content.is_none() && body.input_format.is_some() {
        return Err(bad_request_response(
            "`input_format` is only accepted with `input_content`",
        ));
    }
    if let Some(reference) = dataset_ref {
        if reference.contains('/') {
            return Err(bad_request_response(
                "`dataset_ref` must be a managed ref, not a path",
            ));
        }
        let manager = DatasetManager::new_with_home(Some(home)).map_err(dataset_error_response)?;
        let inspection = manager
            .inspect(&reference)
            .map_err(dataset_error_response)?;
        return Ok(inspection.source_path);
    }
    if let Some(content) = input_content {
        let format = body
            .input_format
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("jsonl");
        if format != "jsonl" {
            return Err(bad_request_response("`input_format` must be `jsonl`"));
        }
        if content.len() > INPUT_CONTENT_MAX_BYTES {
            return Err(bad_request_response(format!(
                "`input_content` must be at most {INPUT_CONTENT_MAX_BYTES} bytes"
            )));
        }
        return stage_content(home, "eval-input", "input.jsonl", &content)
            .map_err(dataset_runtime_io_error_response);
    }
    let input_path = required_absolute_path(input_path.as_deref(), "input_path")?;
    canonical_input_path(&input_path)
}

fn validate_eval_job_request(home: &Path, body: &DatasetEvalRequest) -> Result<(), HttpResponse> {
    let _ = required_string(body.provider.as_deref(), "provider")?;
    let _ = required_string(body.model.as_deref(), "model")?;
    let output_path = required_absolute_path(body.output_path.as_deref(), "output_path")?;
    ensure_missing_or_empty_dir(&output_path)?;

    let dataset_ref = optional_nonblank(body.dataset_ref.clone(), "dataset_ref")?;
    let input_content = optional_content(body.input_content.clone(), "input_content")?;
    let input_path = optional_nonblank(body.input_path.clone(), "input_path")?;
    let selected = [
        dataset_ref.is_some(),
        input_content.is_some(),
        input_path.is_some(),
    ]
    .into_iter()
    .filter(|selected| *selected)
    .count();
    if selected != 1 {
        return Err(bad_request_response(
            "exactly one of `dataset_ref`, `input_content`, or `input_path` is required",
        ));
    }
    if let Some(reference) = dataset_ref {
        if reference.contains('/') {
            return Err(bad_request_response(
                "`dataset_ref` must be a managed ref, not a path",
            ));
        }
        let manager = DatasetManager::new_with_home(Some(home)).map_err(dataset_error_response)?;
        let _ = manager
            .inspect(&reference)
            .map_err(dataset_error_response)?;
    }
    if let Some(content) = input_content {
        let format = body
            .input_format
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("jsonl");
        if format != "jsonl" {
            return Err(bad_request_response("`input_format` must be `jsonl`"));
        }
        if content.len() > INPUT_CONTENT_MAX_BYTES {
            return Err(bad_request_response(format!(
                "`input_content` must be at most {INPUT_CONTENT_MAX_BYTES} bytes"
            )));
        }
    } else if body.input_format.is_some() {
        return Err(bad_request_response(
            "`input_format` is only accepted with `input_content`",
        ));
    }
    if let Some(path) = input_path {
        let path = required_absolute_path(Some(&path), "input_path")?;
        let _ = canonical_input_path(&path)?;
    }

    let _ = normalize_split(
        body.split.as_deref(),
        &["train", "valid", "test", "eval_cases", "all"],
    )?;
    let _ = optional_positive_u32(body.max_records, "max_records")?;
    let _ = optional_nonblank(body.criteria.clone(), "criteria")?;
    let _ = optional_positive_u32(body.max_tokens, "max_tokens")?;
    let _ = optional_f32(body.temperature, 0.0, "temperature")?;
    let _ = optional_range_f32(body.timeout_seconds, 180.0, "timeout_seconds")?;
    Ok(())
}

fn synth_counts(body: &DatasetSynthRequest) -> Result<(String, DatasetSynthCounts), HttpResponse> {
    let split_counts = [
        body.train_count,
        body.valid_count,
        body.test_count,
        body.eval_count,
    ];
    let has_split_counts = split_counts.iter().any(Option::is_some);
    if has_split_counts {
        if body.count.is_some() || body.split.is_some() {
            return Err(bad_request_response(
                "`count` and `split` cannot be combined with split-specific counts",
            ));
        }
        if split_counts.iter().flatten().all(|count| *count == 0) {
            return Err(bad_request_response(
                "at least one split-specific count must be greater than zero",
            ));
        }
        return Ok((
            "train".to_string(),
            DatasetSynthCounts {
                count: None,
                train_count: nonzero(body.train_count),
                valid_count: nonzero(body.valid_count),
                test_count: nonzero(body.test_count),
                eval_count: nonzero(body.eval_count),
            },
        ));
    }

    let split = match normalize_split(
        body.split.as_deref(),
        &["train", "valid", "test", "eval_cases"],
    )? {
        Some(split) => split,
        None => {
            return Err(bad_request_response(
                "`split` is required when using single split `count`",
            ))
        }
    };
    let count = body
        .count
        .ok_or_else(|| bad_request_response("`count` is required for single split synthesis"))?;
    if count == 0 {
        return Err(bad_request_response("`count` must be greater than zero"));
    }
    Ok((
        split,
        DatasetSynthCounts {
            count: Some(count),
            ..DatasetSynthCounts::default()
        },
    ))
}

fn nonzero(value: Option<u32>) -> Option<u32> {
    value.filter(|value| *value > 0)
}

fn reject_prompt_provider_fields(body: &DatasetSynthRequest) -> Result<(), HttpResponse> {
    for (name, present) in [
        ("provider", body.provider.is_some()),
        ("model", body.model.is_some()),
        ("output_path", body.output_path.is_some()),
        ("max_tokens", body.max_tokens.is_some()),
        ("temperature", body.temperature.is_some()),
        ("timeout_seconds", body.timeout_seconds.is_some()),
        ("retries", body.retries.is_some()),
    ] {
        if present {
            return Err(bad_request_response(format!(
                "`{name}` is not accepted when `print_prompt` is true"
            )));
        }
    }
    Ok(())
}

fn optional_nonblank(
    value: Option<String>,
    field: &'static str,
) -> Result<Option<String>, HttpResponse> {
    match value {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Err(bad_request_response(format!("`{field}` must not be blank")))
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        None => Ok(None),
    }
}

fn optional_content(
    value: Option<String>,
    field: &'static str,
) -> Result<Option<String>, HttpResponse> {
    match value {
        Some(value) => {
            if value.trim().is_empty() {
                Err(bad_request_response(format!("`{field}` must not be blank")))
            } else {
                Ok(Some(value))
            }
        }
        None => Ok(None),
    }
}

fn required_string(value: Option<&str>, field: &'static str) -> Result<String, HttpResponse> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| bad_request_response(format!("`{field}` is required")))
}

fn optional_positive_u32(
    value: Option<u32>,
    field: &'static str,
) -> Result<Option<u32>, HttpResponse> {
    match value {
        Some(0) => Err(bad_request_response(format!(
            "`{field}` must be greater than zero"
        ))),
        Some(value) => Ok(Some(value)),
        None => Ok(None),
    }
}

fn optional_max_u32(
    value: Option<u32>,
    default: u32,
    max: u32,
    field: &'static str,
) -> Result<u32, HttpResponse> {
    let value = value.unwrap_or(default);
    if value > max {
        return Err(bad_request_response(format!(
            "`{field}` must be at most {max}"
        )));
    }
    Ok(value)
}

fn optional_f32(
    value: Option<f32>,
    default: f32,
    field: &'static str,
) -> Result<f32, HttpResponse> {
    let value = value.unwrap_or(default);
    if !value.is_finite() {
        return Err(bad_request_response(format!("`{field}` must be finite")));
    }
    Ok(value)
}

fn optional_range_f32(
    value: Option<f32>,
    default: f32,
    field: &'static str,
) -> Result<f32, HttpResponse> {
    let value = optional_f32(value, default, field)?;
    if !(1.0..=3600.0).contains(&value) {
        return Err(bad_request_response(format!(
            "`{field}` must be between 1 and 3600"
        )));
    }
    Ok(value)
}

fn normalize_split(value: Option<&str>, allowed: &[&str]) -> Result<Option<String>, HttpResponse> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if allowed.contains(&value) {
        Ok(Some(value.to_string()))
    } else {
        Err(bad_request_response(format!(
            "`split` must be one of {}",
            allowed.join(", ")
        )))
    }
}

fn required_absolute_path(
    value: Option<&str>,
    field: &'static str,
) -> Result<PathBuf, HttpResponse> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| bad_request_response(format!("`{field}` is required")))?;
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(bad_request_response(format!(
            "`{field}` must be an absolute path on the daemon host filesystem"
        )));
    }
    Ok(path)
}

fn canonical_input_path(path: &Path) -> Result<PathBuf, HttpResponse> {
    path.canonicalize().map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => path_not_found_response(path),
        _ => json_response(
            500,
            ErrorResponse {
                error: "dataset_runtime_failed",
                message: format!("failed to canonicalize `{}`", path.display()),
            },
        ),
    })
}

fn ensure_missing_or_empty_dir(path: &Path) -> Result<(), HttpResponse> {
    if !path.exists() {
        return Ok(());
    }
    if !path.is_dir() {
        return Err(output_exists_response(path));
    }
    let mut entries = fs::read_dir(path).map_err(|_| {
        json_response(
            500,
            ErrorResponse {
                error: "dataset_runtime_failed",
                message: format!("failed to read output directory `{}`", path.display()),
            },
        )
    })?;
    if entries.next().is_some() {
        return Err(output_exists_response(path));
    }
    Ok(())
}

fn output_exists_response(path: &Path) -> HttpResponse {
    json_response(
        409,
        ErrorResponse {
            error: "output_exists",
            message: format!(
                "output path `{}` already exists and is not an empty directory",
                path.display()
            ),
        },
    )
}

fn finish_dataset_job(
    registry: &JobRegistry,
    job_id: &str,
    response: HttpResponse,
    response_key: &'static str,
) {
    let status = response.status_code;
    let HttpBody::Buffered(body) = response.body else {
        registry.fail(
            job_id,
            "dataset job returned a non-buffered daemon response",
        );
        return;
    };
    let value = serde_json::from_slice::<serde_json::Value>(&body).ok();
    if (200..300).contains(&status) {
        let artifact_path = value
            .as_ref()
            .and_then(|value| value.get(response_key))
            .and_then(|value| value.get("output_dir"))
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned);
        let progress_events = value
            .as_ref()
            .and_then(|value| value.get("progress_events"))
            .and_then(serde_json::Value::as_array)
            .map(|events| format!("{} progress event(s)", events.len()));
        registry.succeed(
            job_id,
            None,
            None,
            artifact_path,
            progress_events.unwrap_or_else(|| format!("dataset {response_key} completed")),
        );
    } else {
        let message = value
            .as_ref()
            .and_then(|value| {
                value
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .or_else(|| value.get("error").and_then(serde_json::Value::as_str))
            })
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("dataset {response_key} returned HTTP {status}"));
        registry.fail(job_id, message);
    }
}

fn stage_content(home: &Path, prefix: &str, file_name: &str, content: &str) -> io::Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let stage_dir = home
        .join("runtime")
        .join("dataset-http")
        .join(format!("{prefix}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&stage_dir)?;
    let path = stage_dir.join(file_name);
    fs::write(&path, content)?;
    Ok(path)
}

fn dataset_runtime_error_response(error: DatasetRuntimeError, code: &'static str) -> HttpResponse {
    match &error {
        DatasetRuntimeError::UnsupportedProvider(_) => {
            return json_response(
                400,
                ErrorResponse {
                    error: "bad_request",
                    message: error.to_string(),
                },
            )
        }
        DatasetRuntimeError::ProviderAuthMissing { .. }
        | DatasetRuntimeError::ProviderAuthInvalid { .. }
        | DatasetRuntimeError::ProviderAuthUnknown { .. } => {
            return json_response(
                409,
                ErrorResponse {
                    error: "provider_auth_failed",
                    message: "provider auth failed for dataset cloud tool".to_string(),
                },
            )
        }
        DatasetRuntimeError::HelperExit { .. } | DatasetRuntimeError::InvalidJson { .. } => {
            return json_response(502, dataset_cloud_error_body(code, &error))
        }
        DatasetRuntimeError::RuntimeAssets(_)
        | DatasetRuntimeError::MissingPythonInterpreter { .. }
        | DatasetRuntimeError::Auth(_)
        | DatasetRuntimeError::Spawn { .. }
        | DatasetRuntimeError::Wait { .. }
        | DatasetRuntimeError::StdoutRead { .. }
        | DatasetRuntimeError::StderrRead { .. } => {}
    }
    json_response(
        500,
        ErrorResponse {
            error: "dataset_runtime_failed",
            message: "dataset runtime failed".to_string(),
        },
    )
}

#[derive(Serialize)]
struct DatasetCloudErrorBody {
    error: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    debug: Option<DatasetRuntimeDebugItem>,
}

fn dataset_cloud_error_body(
    code: &'static str,
    error: &DatasetRuntimeError,
) -> DatasetCloudErrorBody {
    DatasetCloudErrorBody {
        error: code,
        message: format!("{code}; inspect debug artifacts when available"),
        debug: error.debug().map(debug_item),
    }
}

fn debug_item(debug: &DatasetRuntimeDebug) -> DatasetRuntimeDebugItem {
    DatasetRuntimeDebugItem {
        output_path: debug.output_path.as_deref().map(path_string),
        debug_dir: debug.debug_dir.as_deref().map(path_string),
        prompt_path: debug.prompt_path.as_deref().map(path_string),
        provider_output_path: debug.provider_output_path.as_deref().map(path_string),
        error_path: debug.error_path.as_deref().map(path_string),
    }
}

fn dataset_runtime_io_error_response(error: io::Error) -> HttpResponse {
    json_response(
        500,
        ErrorResponse {
            error: "dataset_runtime_failed",
            message: format!("dataset runtime staging failed: {error}"),
        },
    )
}

fn dataset_error_response(error: DatasetError) -> HttpResponse {
    match error {
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
                error: "dataset_runtime_failed",
                message: format!("failed to resolve dataset input: {other}"),
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
