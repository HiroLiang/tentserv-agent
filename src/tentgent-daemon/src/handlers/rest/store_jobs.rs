use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use tentgent_kernel::features::{
    auth::{
        domain::{AuthEnvLoadPolicy, Provider},
        usecases::AuthSecretResolutionRequest,
    },
    dataset::domain::{
        DatasetEvalSplit, DatasetPromptSource, DatasetProvider, DatasetRefSelector,
        DatasetSplitKind, DatasetSynthCounts,
    },
    model::domain::ModelRefSelector,
};

use crate::{
    runtime::{JobOutputLine, JobProgressPatch, JobProgressUpdate, JobStream},
    transport::rest::error::RestError,
};

pub const SPEC_CONTENT_MAX_BYTES: usize = 256 * 1024;
pub const INPUT_CONTENT_MAX_BYTES: usize = 10 * 1024 * 1024;

pub struct ParsedSynthSource {
    pub prompt_source: DatasetPromptSource,
    pub label: String,
}

pub enum ParsedEvalInput {
    LocalPath(PathBuf),
    ManagedDataset(DatasetRefSelector),
}

pub fn canonical_import_path(value: &str) -> Result<PathBuf, RestError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(RestError::bad_request(
            "bad_request",
            "path must not be blank",
        ));
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        return Err(RestError::bad_request(
            "bad_request",
            "path must be an absolute path on the daemon host filesystem",
        ));
    }
    path.canonicalize().map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => RestError::not_found(
            "path_not_found",
            format!("path `{trimmed}` was not found on the daemon host"),
        ),
        _ => RestError::internal(
            "store_mutation_failed",
            format!("failed to canonicalize path `{trimmed}`: {error}"),
        ),
    })
}

pub fn normalize_repo_id(value: &str) -> Result<String, RestError> {
    let repo_id = value.trim();
    if repo_id.is_empty() {
        return Err(RestError::bad_request(
            "bad_request",
            "repo_id must not be blank",
        ));
    }
    if repo_id.contains("://")
        || repo_id.contains("/tree/")
        || repo_id.starts_with('/')
        || repo_id.contains('\\')
    {
        return Err(RestError::bad_request(
            "bad_request",
            "repo_id must be a Hugging Face repo id such as `owner/name`, not a URL or path",
        ));
    }

    let segments = repo_id.split('/').collect::<Vec<_>>();
    if segments.len() != 2 || segments.iter().any(|segment| invalid_repo_segment(segment)) {
        return Err(RestError::bad_request(
            "bad_request",
            "repo_id must be a Hugging Face repo id such as `owner/name`",
        ));
    }

    Ok(repo_id.to_string())
}

pub fn normalize_revision(value: Option<String>) -> Result<Option<String>, RestError> {
    match value {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Err(RestError::bad_request(
                    "bad_request",
                    "revision must not be blank when provided",
                ))
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        None => Ok(None),
    }
}

pub fn normalize_optional_model_ref(
    value: Option<String>,
    field: &'static str,
) -> Result<Option<ModelRefSelector>, RestError> {
    let Some(value) = optional_nonblank(value, field)? else {
        return Ok(None);
    };
    ModelRefSelector::parse(&value)
        .map(Some)
        .map_err(|err| RestError::bad_request("bad_request", format!("invalid `{field}`: {err}")))
}

pub fn hf_progress_update(
    description: String,
    position: u64,
    total: Option<u64>,
    unit: &str,
    finished: bool,
) -> JobProgressUpdate {
    let stage = if description.trim().is_empty() {
        if finished {
            "download complete".to_string()
        } else {
            "downloading".to_string()
        }
    } else {
        description
    };
    let unit = unit.to_ascii_lowercase();
    let is_bytes = matches!(unit.as_str(), "b" | "byte" | "bytes");
    JobProgressUpdate {
        stage: Some(stage.clone()),
        progress: JobProgressPatch {
            bytes_done: is_bytes.then_some(position),
            bytes_total: is_bytes.then_some(total).flatten(),
            files_done: (!is_bytes).then_some(position),
            files_total: (!is_bytes).then_some(total).flatten(),
            ..JobProgressPatch::default()
        },
        output: vec![JobOutputLine::new(JobStream::Event, stage)],
        warning_summary: None,
    }
}

pub fn dataset_auth_request(provider: DatasetProvider) -> AuthSecretResolutionRequest {
    let provider = match provider {
        DatasetProvider::OpenAI => Provider::OpenAI,
        DatasetProvider::Anthropic => Provider::Anthropic,
    };
    AuthSecretResolutionRequest::for_secret_use(provider, AuthEnvLoadPolicy::CwdDotenvOverride)
}

pub fn required_dataset_provider(value: Option<&str>) -> Result<DatasetProvider, RestError> {
    match required_string(value, "provider")?.as_str() {
        "openai" => Ok(DatasetProvider::OpenAI),
        "anthropic" => Ok(DatasetProvider::Anthropic),
        _ => Err(RestError::bad_request(
            "bad_request",
            "`provider` must be one of openai, anthropic",
        )),
    }
}

pub fn required_absolute_path(
    value: Option<&str>,
    field: &'static str,
) -> Result<PathBuf, RestError> {
    let value = required_string(value, field)?;
    let path = PathBuf::from(&value);
    if !path.is_absolute() {
        return Err(RestError::bad_request(
            "bad_request",
            format!("`{field}` must be an absolute path on the daemon host filesystem"),
        ));
    }
    Ok(path)
}

pub fn ensure_missing_or_empty_dir(path: &Path) -> Result<(), RestError> {
    if !path.exists() {
        return Ok(());
    }
    if !path.is_dir() {
        return Err(output_exists_error(path));
    }
    let mut entries = fs::read_dir(path).map_err(|error| {
        RestError::internal(
            "dataset_runtime_failed",
            format!(
                "failed to read output directory `{}`: {error}",
                path.display()
            ),
        )
    })?;
    if entries.next().is_some() {
        return Err(output_exists_error(path));
    }
    Ok(())
}

pub fn parse_synth_source(
    home: &Path,
    brief: Option<String>,
    spec_content: Option<String>,
    spec_path: Option<String>,
) -> Result<ParsedSynthSource, RestError> {
    let brief = optional_nonblank(brief, "brief")?;
    let spec_content = optional_content(spec_content, "spec_content")?;
    let spec_path = optional_nonblank(spec_path, "spec_path")?;
    let selected = [brief.is_some(), spec_content.is_some(), spec_path.is_some()]
        .into_iter()
        .filter(|selected| *selected)
        .count();
    if selected != 1 {
        return Err(RestError::bad_request(
            "bad_request",
            "exactly one of `brief`, `spec_content`, or `spec_path` is required",
        ));
    }
    if let Some(brief) = brief {
        return Ok(ParsedSynthSource {
            label: format!("synth {brief}"),
            prompt_source: DatasetPromptSource::Brief(brief),
        });
    }
    if let Some(content) = spec_content {
        if content.len() > SPEC_CONTENT_MAX_BYTES {
            return Err(RestError::bad_request(
                "bad_request",
                format!("`spec_content` must be at most {SPEC_CONTENT_MAX_BYTES} bytes"),
            ));
        }
        let path = stage_content(home, "synth-spec", "spec.md", &content)?;
        return Ok(ParsedSynthSource {
            label: format!("synth {}", path.display()),
            prompt_source: DatasetPromptSource::SpecPath(path),
        });
    }
    let path = required_absolute_path(spec_path.as_deref(), "spec_path")?;
    let path = canonical_input_path(&path)?;
    Ok(ParsedSynthSource {
        label: format!("synth {}", path.display()),
        prompt_source: DatasetPromptSource::SpecPath(path),
    })
}

pub fn synth_counts(
    split: Option<String>,
    count: Option<u32>,
    train_count: Option<u32>,
    valid_count: Option<u32>,
    test_count: Option<u32>,
    eval_count: Option<u32>,
) -> Result<(DatasetSplitKind, DatasetSynthCounts), RestError> {
    let split_counts = [train_count, valid_count, test_count, eval_count];
    let has_split_counts = split_counts.iter().any(Option::is_some);
    if has_split_counts {
        if count.is_some() || split.is_some() {
            return Err(RestError::bad_request(
                "bad_request",
                "`count` and `split` cannot be combined with split-specific counts",
            ));
        }
        if split_counts.iter().flatten().all(|count| *count == 0) {
            return Err(RestError::bad_request(
                "bad_request",
                "at least one split-specific count must be greater than zero",
            ));
        }
        return Ok((
            DatasetSplitKind::Train,
            DatasetSynthCounts {
                count: None,
                train_count: nonzero(train_count),
                valid_count: nonzero(valid_count),
                test_count: nonzero(test_count),
                eval_count: nonzero(eval_count),
            },
        ));
    }

    let split = split
        .as_deref()
        .map(parse_dataset_split)
        .transpose()?
        .ok_or_else(|| {
            RestError::bad_request(
                "bad_request",
                "`split` is required when using single split `count`",
            )
        })?;
    let count =
        count.ok_or_else(|| RestError::bad_request("bad_request", "`count` is required"))?;
    if count == 0 {
        return Err(RestError::bad_request(
            "bad_request",
            "`count` must be greater than zero",
        ));
    }
    Ok((
        split,
        DatasetSynthCounts {
            count: Some(count),
            ..DatasetSynthCounts::default()
        },
    ))
}

pub fn parse_eval_input(
    home: &Path,
    dataset_ref: Option<String>,
    input_content: Option<String>,
    input_format: Option<String>,
    input_path: Option<String>,
) -> Result<(ParsedEvalInput, String), RestError> {
    let dataset_ref = optional_nonblank(dataset_ref, "dataset_ref")?;
    let input_content = optional_content(input_content, "input_content")?;
    let input_path = optional_nonblank(input_path, "input_path")?;
    let selected = [
        dataset_ref.is_some(),
        input_content.is_some(),
        input_path.is_some(),
    ]
    .into_iter()
    .filter(|selected| *selected)
    .count();
    if selected != 1 {
        return Err(RestError::bad_request(
            "bad_request",
            "exactly one of `dataset_ref`, `input_content`, or `input_path` is required",
        ));
    }
    if input_content.is_none() && input_format.is_some() {
        return Err(RestError::bad_request(
            "bad_request",
            "`input_format` is only accepted with `input_content`",
        ));
    }
    if let Some(reference) = dataset_ref {
        if reference.contains('/') {
            return Err(RestError::bad_request(
                "bad_request",
                "`dataset_ref` must be a managed ref, not a path",
            ));
        }
        let selector = DatasetRefSelector::parse(&reference).map_err(|err| {
            RestError::bad_request("bad_request", format!("invalid `dataset_ref`: {err}"))
        })?;
        return Ok((
            ParsedEvalInput::ManagedDataset(selector),
            format!("eval {reference}"),
        ));
    }
    if let Some(content) = input_content {
        let format = input_format
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("jsonl");
        if format != "jsonl" {
            return Err(RestError::bad_request(
                "bad_request",
                "`input_format` must be `jsonl`",
            ));
        }
        if content.len() > INPUT_CONTENT_MAX_BYTES {
            return Err(RestError::bad_request(
                "bad_request",
                format!("`input_content` must be at most {INPUT_CONTENT_MAX_BYTES} bytes"),
            ));
        }
        let path = stage_content(home, "eval-input", "input.jsonl", &content)?;
        return Ok((
            ParsedEvalInput::LocalPath(path.clone()),
            format!("eval {}", path.display()),
        ));
    }
    let path = required_absolute_path(input_path.as_deref(), "input_path")?;
    let path = canonical_input_path(&path)?;
    Ok((
        ParsedEvalInput::LocalPath(path.clone()),
        format!("eval {}", path.display()),
    ))
}

pub fn parse_eval_split(value: Option<String>) -> Result<DatasetEvalSplit, RestError> {
    match value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("train")
    {
        "train" => Ok(DatasetEvalSplit::Train),
        "valid" => Ok(DatasetEvalSplit::Valid),
        "test" => Ok(DatasetEvalSplit::Test),
        "eval_cases" => Ok(DatasetEvalSplit::EvalCases),
        "all" => Ok(DatasetEvalSplit::All),
        _ => Err(RestError::bad_request(
            "bad_request",
            "`split` must be one of train, valid, test, eval_cases, all",
        )),
    }
}

pub fn optional_positive_u32(
    value: Option<u32>,
    field: &'static str,
) -> Result<Option<u32>, RestError> {
    match value {
        Some(0) => Err(RestError::bad_request(
            "bad_request",
            format!("`{field}` must be greater than zero"),
        )),
        Some(value) => Ok(Some(value)),
        None => Ok(None),
    }
}

pub fn optional_max_u32(
    value: Option<u32>,
    default: u32,
    max: u32,
    field: &'static str,
) -> Result<u32, RestError> {
    let value = value.unwrap_or(default);
    if value > max {
        return Err(RestError::bad_request(
            "bad_request",
            format!("`{field}` must be at most {max}"),
        ));
    }
    Ok(value)
}

pub fn optional_f32(
    value: Option<f32>,
    default: f32,
    field: &'static str,
) -> Result<f32, RestError> {
    let value = value.unwrap_or(default);
    if !value.is_finite() {
        return Err(RestError::bad_request(
            "bad_request",
            format!("`{field}` must be finite"),
        ));
    }
    Ok(value)
}

pub fn optional_range_f32(
    value: Option<f32>,
    default: f32,
    field: &'static str,
) -> Result<f32, RestError> {
    let value = optional_f32(value, default, field)?;
    if !(1.0..=3600.0).contains(&value) {
        return Err(RestError::bad_request(
            "bad_request",
            format!("`{field}` must be between 1 and 3600"),
        ));
    }
    Ok(value)
}

pub fn optional_nonblank(
    value: Option<String>,
    field: &'static str,
) -> Result<Option<String>, RestError> {
    match value {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Err(RestError::bad_request(
                    "bad_request",
                    format!("`{field}` must not be blank"),
                ))
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        None => Ok(None),
    }
}

pub fn optional_content(
    value: Option<String>,
    field: &'static str,
) -> Result<Option<String>, RestError> {
    match value {
        Some(value) => {
            if value.trim().is_empty() {
                Err(RestError::bad_request(
                    "bad_request",
                    format!("`{field}` must not be blank"),
                ))
            } else {
                Ok(Some(value))
            }
        }
        None => Ok(None),
    }
}

pub fn required_string(value: Option<&str>, field: &'static str) -> Result<String, RestError> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| RestError::bad_request("bad_request", format!("`{field}` is required")))
}

fn parse_dataset_split(value: &str) -> Result<DatasetSplitKind, RestError> {
    match value.trim() {
        "train" => Ok(DatasetSplitKind::Train),
        "valid" => Ok(DatasetSplitKind::Valid),
        "test" => Ok(DatasetSplitKind::Test),
        "eval_cases" => Ok(DatasetSplitKind::EvalCases),
        _ => Err(RestError::bad_request(
            "bad_request",
            "`split` must be one of train, valid, test, eval_cases",
        )),
    }
}

fn invalid_repo_segment(segment: &str) -> bool {
    segment.is_empty()
        || segment == "."
        || segment == ".."
        || segment
            .chars()
            .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.'))
}

fn nonzero(value: Option<u32>) -> Option<u32> {
    value.filter(|value| *value > 0)
}

fn canonical_input_path(path: &Path) -> Result<PathBuf, RestError> {
    path.canonicalize().map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => RestError::not_found(
            "path_not_found",
            format!("path `{}` was not found on the daemon host", path.display()),
        ),
        _ => RestError::internal(
            "dataset_runtime_failed",
            format!("failed to canonicalize `{}`: {error}", path.display()),
        ),
    })
}

fn output_exists_error(path: &Path) -> RestError {
    RestError::conflict(
        "output_exists",
        format!(
            "output path `{}` already exists and is not an empty directory",
            path.display()
        ),
    )
}

fn stage_content(
    home: &Path,
    prefix: &str,
    file_name: &str,
    content: &str,
) -> Result<PathBuf, RestError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let stage_dir = home
        .join("runtime")
        .join("dataset-daemon")
        .join(format!("{prefix}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&stage_dir).map_err(|error| {
        RestError::internal(
            "dataset_runtime_failed",
            format!("failed to create dataset staging directory: {error}"),
        )
    })?;
    let path = stage_dir.join(file_name);
    fs::write(&path, content).map_err(|error| {
        RestError::internal(
            "dataset_runtime_failed",
            format!(
                "failed to write dataset staging file `{}`: {error}",
                path.display()
            ),
        )
    })?;
    Ok(path)
}
