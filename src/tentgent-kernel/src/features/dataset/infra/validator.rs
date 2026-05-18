use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::features::dataset::domain::{
    DatasetValidationIssue, DatasetValidationOutcome, DatasetValidationSplit,
    DatasetValidationTargetKind, CANONICAL_CHAT_SCHEMA, EVAL_CASES_SPLIT_FILENAME,
    LEGACY_VALID_SPLIT_FILENAME, TEST_SPLIT_FILENAME, TRAIN_SPLIT_FILENAME, VALID_SPLIT_FILENAME,
};
use crate::features::dataset::ports::DatasetValidator;
use crate::foundation::error::KernelResult;

use super::error::{dataset_store_error, path_error};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplitValidationKind {
    Training,
    Eval,
}

impl SplitValidationKind {
    const fn requires_final_assistant(self) -> bool {
        matches!(self, Self::Training)
    }
}

/// Validates local dataset JSONL and package directories.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdDatasetValidator;

impl DatasetValidator for StdDatasetValidator {
    fn validate_dataset_path(&self, path: &Path) -> KernelResult<DatasetValidationOutcome> {
        if !path.exists() {
            return Err(dataset_store_error(format!(
                "dataset path does not exist: `{}`",
                path.display()
            )));
        }

        if path.is_file() {
            validate_dataset_file(path)
        } else if path.is_dir() {
            validate_dataset_dir(path)
        } else {
            Err(dataset_store_error(format!(
                "path is not a supported dataset file or directory: `{}`",
                path.display()
            )))
        }
    }
}

fn validate_dataset_file(path: &Path) -> KernelResult<DatasetValidationOutcome> {
    ensure_jsonl_path(path)?;
    let (records, errors) = validate_jsonl_file(path, SplitValidationKind::Training)?;
    let split_errors = errors.len();
    Ok(DatasetValidationOutcome {
        path: path.to_path_buf(),
        target_kind: DatasetValidationTargetKind::File,
        tuning_ready: true,
        splits: vec![DatasetValidationSplit {
            name: "train".to_string(),
            path: path.to_path_buf(),
            records,
            errors: split_errors,
        }],
        warnings: vec![
            "single JSONL validation treats the file as a train split; use a directory with train.jsonl for canonical packages".to_string(),
        ],
        errors,
    })
}

fn validate_dataset_dir(path: &Path) -> KernelResult<DatasetValidationOutcome> {
    let mut warnings = Vec::new();
    let mut split_files = Vec::new();

    push_split_if_present(
        &mut split_files,
        path,
        "train",
        TRAIN_SPLIT_FILENAME,
        SplitValidationKind::Training,
    );
    push_split_if_present(
        &mut split_files,
        path,
        "valid",
        VALID_SPLIT_FILENAME,
        SplitValidationKind::Training,
    );
    push_split_if_present(
        &mut split_files,
        path,
        "test",
        TEST_SPLIT_FILENAME,
        SplitValidationKind::Training,
    );
    push_split_if_present(
        &mut split_files,
        path,
        "eval",
        EVAL_CASES_SPLIT_FILENAME,
        SplitValidationKind::Eval,
    );

    let legacy_valid = path.join(LEGACY_VALID_SPLIT_FILENAME);
    if legacy_valid.is_file() {
        if path.join(VALID_SPLIT_FILENAME).is_file() {
            warnings.push(
                "`val.jsonl` was found but `valid.jsonl` is the canonical validation split"
                    .to_string(),
            );
        } else {
            warnings.push(
                "`val.jsonl` was found; treating it as validation, but `valid.jsonl` is preferred"
                    .to_string(),
            );
            split_files.push((
                "valid".to_string(),
                legacy_valid,
                SplitValidationKind::Training,
            ));
        }
    }

    if split_files.is_empty() {
        return Err(dataset_store_error(
            "unsupported dataset layout: expected at least one root split file: train.jsonl, valid.jsonl, test.jsonl, or eval_cases.jsonl",
        ));
    }

    if !path.join(TRAIN_SPLIT_FILENAME).is_file() {
        warnings.push(
            "no root `train.jsonl` detected; this dataset can be validated but is not ready for tuning".to_string(),
        );
    }

    warn_for_extra_root_jsonl(path, &mut warnings)?;

    let mut splits = Vec::new();
    let mut errors = Vec::new();
    for (name, split_path, kind) in split_files {
        let (records, mut split_errors) = validate_jsonl_file(&split_path, kind)?;
        let error_count = split_errors.len();
        splits.push(DatasetValidationSplit {
            name,
            path: split_path,
            records,
            errors: error_count,
        });
        errors.append(&mut split_errors);
    }

    Ok(DatasetValidationOutcome {
        path: path.to_path_buf(),
        target_kind: DatasetValidationTargetKind::Directory,
        tuning_ready: path.join(TRAIN_SPLIT_FILENAME).is_file(),
        splits,
        warnings,
        errors,
    })
}

fn push_split_if_present(
    splits: &mut Vec<(String, PathBuf, SplitValidationKind)>,
    root: &Path,
    name: &str,
    file: &str,
    kind: SplitValidationKind,
) {
    let path = root.join(file);
    if path.is_file() {
        splits.push((name.to_string(), path, kind));
    }
}

fn warn_for_extra_root_jsonl(root: &Path, warnings: &mut Vec<String>) -> KernelResult<()> {
    let known = [
        TRAIN_SPLIT_FILENAME,
        VALID_SPLIT_FILENAME,
        LEGACY_VALID_SPLIT_FILENAME,
        TEST_SPLIT_FILENAME,
        EVAL_CASES_SPLIT_FILENAME,
    ];
    for entry in
        std::fs::read_dir(root).map_err(|err| path_error("read dataset directory", root, err))?
    {
        let entry = entry.map_err(|err| {
            dataset_store_error(format!(
                "read entry in dataset directory `{}` failed: {err}",
                root.display()
            ))
        })?;
        if !entry
            .file_type()
            .map_err(|err| path_error("read dataset entry type", &entry.path(), err))?
            .is_file()
        {
            continue;
        }
        let path = entry.path();
        let is_jsonl = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"));
        if !is_jsonl {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if !known.contains(&file_name.as_str()) {
            warnings.push(format!(
                "root JSONL file `{file_name}` is not a canonical split and was not schema-validated"
            ));
        }
    }

    Ok(())
}

fn ensure_jsonl_path(path: &Path) -> KernelResult<()> {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
    {
        return Ok(());
    }

    Err(dataset_store_error(format!(
        "unsupported dataset layout: expected a .jsonl file, got `{}`",
        path.display()
    )))
}

fn validate_jsonl_file(
    path: &Path,
    kind: SplitValidationKind,
) -> KernelResult<(usize, Vec<DatasetValidationIssue>)> {
    let file = File::open(path).map_err(|err| path_error("open dataset JSONL", path, err))?;
    let reader = BufReader::new(file);
    let mut records = 0usize;
    let mut errors = Vec::new();

    for (index, line) in reader.lines().enumerate() {
        let line_number = index + 1;
        let line = line.map_err(|err| path_error("read dataset JSONL", path, err))?;
        if line.trim().is_empty() {
            continue;
        }
        records += 1;
        match serde_json::from_str::<Value>(&line) {
            Ok(value) => validate_record(path, line_number, &value, kind, &mut errors),
            Err(err) => errors.push(issue(path, line_number, format!("invalid JSON: {err}"))),
        }
    }

    if records == 0 {
        errors.push(issue(path, 1, "JSONL file contains no records"));
    }

    Ok((records, errors))
}

fn validate_record(
    path: &Path,
    line: usize,
    value: &Value,
    kind: SplitValidationKind,
    errors: &mut Vec<DatasetValidationIssue>,
) {
    let Some(record) = value.as_object() else {
        errors.push(issue(path, line, "record must be a JSON object"));
        return;
    };

    if kind == SplitValidationKind::Eval && is_legacy_eval_case(record) {
        validate_legacy_eval_case(path, line, record, errors);
        return;
    }

    if let Some(schema) = record.get("schema") {
        match schema.as_str() {
            Some(CANONICAL_CHAT_SCHEMA) => {}
            Some(other) => errors.push(issue(
                path,
                line,
                format!("unsupported schema `{other}`; expected `{CANONICAL_CHAT_SCHEMA}`"),
            )),
            None => errors.push(issue(path, line, "`schema` must be a string")),
        }
    } else {
        errors.push(issue(path, line, "missing `schema`"));
    }

    validate_messages(path, line, record, kind, errors);
    validate_tools(path, line, record, errors);

    for forbidden in ["prompt", "completion", "input", "output", "answer"] {
        if record.contains_key(forbidden) {
            errors.push(issue(
                path,
                line,
                format!("top-level `{forbidden}` is not part of tentgent.chat.v1"),
            ));
        }
    }
}

fn validate_messages(
    path: &Path,
    line: usize,
    record: &Map<String, Value>,
    kind: SplitValidationKind,
    errors: &mut Vec<DatasetValidationIssue>,
) {
    let Some(messages) = record.get("messages").and_then(Value::as_array) else {
        errors.push(issue(path, line, "`messages` must be a non-empty array"));
        return;
    };

    if messages.is_empty() {
        errors.push(issue(path, line, "`messages` must be a non-empty array"));
        return;
    }

    let mut tool_call_ids = HashSet::new();
    for (index, message) in messages.iter().enumerate() {
        let Some(message) = message.as_object() else {
            errors.push(issue(
                path,
                line,
                format!("messages[{index}] must be an object"),
            ));
            continue;
        };

        match message.get("role").and_then(Value::as_str) {
            Some("system" | "user" | "assistant" | "tool") => {}
            Some(other) => errors.push(issue(
                path,
                line,
                format!("messages[{index}].role `{other}` is not supported"),
            )),
            None => errors.push(issue(
                path,
                line,
                format!("messages[{index}].role must be a string"),
            )),
        }

        if !message.contains_key("content") {
            errors.push(issue(
                path,
                line,
                format!("messages[{index}] is missing `content`"),
            ));
        }

        if let Some(calls) = message.get("tool_calls") {
            let Some(calls) = calls.as_array() else {
                errors.push(issue(
                    path,
                    line,
                    format!("messages[{index}].tool_calls must be an array"),
                ));
                continue;
            };
            for (call_index, call) in calls.iter().enumerate() {
                validate_tool_call(
                    path,
                    line,
                    index,
                    call_index,
                    call,
                    &mut tool_call_ids,
                    errors,
                );
            }
        }

        if message.get("role").and_then(Value::as_str) == Some("tool")
            && message
                .get("tool_call_id")
                .and_then(Value::as_str)
                .is_none()
        {
            errors.push(issue(
                path,
                line,
                format!("messages[{index}] with role tool must include `tool_call_id`"),
            ));
        }
    }

    if kind.requires_final_assistant() {
        match messages.last().and_then(Value::as_object) {
            Some(last) if last.get("role").and_then(Value::as_str) == Some("assistant") => {}
            _ => errors.push(issue(
                path,
                line,
                "training records must end with a final assistant message",
            )),
        }
    }
}

fn validate_tool_call(
    path: &Path,
    line: usize,
    message_index: usize,
    call_index: usize,
    call: &Value,
    tool_call_ids: &mut HashSet<String>,
    errors: &mut Vec<DatasetValidationIssue>,
) {
    let Some(call) = call.as_object() else {
        errors.push(issue(
            path,
            line,
            format!("messages[{message_index}].tool_calls[{call_index}] must be an object"),
        ));
        return;
    };

    match call.get("id").and_then(Value::as_str) {
        Some(id) if !id.trim().is_empty() => {
            tool_call_ids.insert(id.to_string());
        }
        _ => errors.push(issue(
            path,
            line,
            format!(
                "messages[{message_index}].tool_calls[{call_index}].id must be a non-empty string"
            ),
        )),
    }
    if call.get("name").and_then(Value::as_str).is_none() {
        errors.push(issue(
            path,
            line,
            format!("messages[{message_index}].tool_calls[{call_index}].name must be a string"),
        ));
    }
    if !call.contains_key("arguments") {
        errors.push(issue(
            path,
            line,
            format!("messages[{message_index}].tool_calls[{call_index}] is missing `arguments`"),
        ));
    }
}

fn validate_tools(
    path: &Path,
    line: usize,
    record: &Map<String, Value>,
    errors: &mut Vec<DatasetValidationIssue>,
) {
    let Some(tools) = record.get("tools") else {
        return;
    };
    let Some(tools) = tools.as_array() else {
        errors.push(issue(path, line, "`tools` must be an array when present"));
        return;
    };

    for (index, tool) in tools.iter().enumerate() {
        let Some(tool) = tool.as_object() else {
            errors.push(issue(
                path,
                line,
                format!("tools[{index}] must be an object"),
            ));
            continue;
        };
        if tool.get("name").and_then(Value::as_str).is_none() {
            errors.push(issue(
                path,
                line,
                format!("tools[{index}].name must be a string"),
            ));
        }
        if !tool.contains_key("parameters") {
            errors.push(issue(
                path,
                line,
                format!("tools[{index}] is missing `parameters`"),
            ));
        }
    }
}

fn is_legacy_eval_case(record: &Map<String, Value>) -> bool {
    record.contains_key("input") || record.contains_key("expected")
}

fn validate_legacy_eval_case(
    path: &Path,
    line: usize,
    record: &Map<String, Value>,
    errors: &mut Vec<DatasetValidationIssue>,
) {
    if record.get("input").and_then(Value::as_str).is_none() {
        errors.push(issue(
            path,
            line,
            "legacy eval case must include string `input`",
        ));
    }
}

fn issue(path: &Path, line: usize, message: impl Into<String>) -> DatasetValidationIssue {
    DatasetValidationIssue {
        path: path.to_path_buf(),
        line,
        message: message.into(),
    }
}
