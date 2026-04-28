use std::{
    collections::HashSet,
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use serde_json::Value;

use super::error::DatasetError;

pub const CANONICAL_CHAT_SCHEMA: &str = "tentgent.chat.v1";

const TRAIN_FILE: &str = "train.jsonl";
const VALID_FILE: &str = "valid.jsonl";
const LEGACY_VAL_FILE: &str = "val.jsonl";
const TEST_FILE: &str = "test.jsonl";
const EVAL_CASES_FILE: &str = "eval_cases.jsonl";

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

#[derive(Debug, Clone)]
pub struct DatasetValidationOutcome {
    pub path: PathBuf,
    pub target_kind: DatasetValidationTargetKind,
    pub tuning_ready: bool,
    pub splits: Vec<DatasetValidationSplit>,
    pub warnings: Vec<String>,
    pub errors: Vec<DatasetValidationIssue>,
}

impl DatasetValidationOutcome {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn record_count(&self) -> usize {
        self.splits.iter().map(|split| split.records).sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatasetValidationTargetKind {
    File,
    Directory,
}

impl DatasetValidationTargetKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DatasetValidationSplit {
    pub name: String,
    pub path: PathBuf,
    pub records: usize,
    pub errors: usize,
}

#[derive(Debug, Clone)]
pub struct DatasetValidationIssue {
    pub path: PathBuf,
    pub line: usize,
    pub message: String,
}

pub fn validate_dataset_path(
    path: impl AsRef<Path>,
) -> Result<DatasetValidationOutcome, DatasetError> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(DatasetError::MissingPath(path.to_path_buf()));
    }

    if path.is_file() {
        validate_dataset_file(path)
    } else if path.is_dir() {
        validate_dataset_dir(path)
    } else {
        Err(DatasetError::UnsupportedPath(path.to_path_buf()))
    }
}

fn validate_dataset_file(path: &Path) -> Result<DatasetValidationOutcome, DatasetError> {
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

fn validate_dataset_dir(path: &Path) -> Result<DatasetValidationOutcome, DatasetError> {
    let mut warnings = Vec::new();
    let mut split_files = Vec::new();

    push_split_if_present(
        &mut split_files,
        path,
        "train",
        TRAIN_FILE,
        SplitValidationKind::Training,
    );
    push_split_if_present(
        &mut split_files,
        path,
        "valid",
        VALID_FILE,
        SplitValidationKind::Training,
    );
    push_split_if_present(
        &mut split_files,
        path,
        "test",
        TEST_FILE,
        SplitValidationKind::Training,
    );
    push_split_if_present(
        &mut split_files,
        path,
        "eval",
        EVAL_CASES_FILE,
        SplitValidationKind::Eval,
    );

    let legacy_val = path.join(LEGACY_VAL_FILE);
    if legacy_val.is_file() {
        if path.join(VALID_FILE).is_file() {
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
                legacy_val,
                SplitValidationKind::Training,
            ));
        }
    }

    if split_files.is_empty() {
        return Err(DatasetError::UnsupportedLayout {
            reason: "expected at least one root split file: train.jsonl, valid.jsonl, test.jsonl, or eval_cases.jsonl".to_string(),
        });
    }

    if !path.join(TRAIN_FILE).is_file() {
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
        tuning_ready: path.join(TRAIN_FILE).is_file(),
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

fn warn_for_extra_root_jsonl(root: &Path, warnings: &mut Vec<String>) -> Result<(), DatasetError> {
    let known = [
        TRAIN_FILE,
        VALID_FILE,
        LEGACY_VAL_FILE,
        TEST_FILE,
        EVAL_CASES_FILE,
    ];
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
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

fn ensure_jsonl_path(path: &Path) -> Result<(), DatasetError> {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
    {
        return Ok(());
    }

    Err(DatasetError::UnsupportedLayout {
        reason: format!("expected a .jsonl file, got `{}`", path.display()),
    })
}

fn validate_jsonl_file(
    path: &Path,
    kind: SplitValidationKind,
) -> Result<(usize, Vec<DatasetValidationIssue>), DatasetError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut records = 0usize;
    let mut errors = Vec::new();

    for (index, line) in reader.lines().enumerate() {
        let line_number = index + 1;
        let line = line?;
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
                format!("schema must be `{CANONICAL_CHAT_SCHEMA}`, got `{other}`"),
            )),
            None => errors.push(issue(path, line, "`schema` must be a string when present")),
        }
    }

    let Some(messages) = record.get("messages") else {
        errors.push(issue(path, line, "`messages` is required"));
        return;
    };
    let Some(messages) = messages.as_array() else {
        errors.push(issue(path, line, "`messages` must be an array"));
        return;
    };
    if messages.is_empty() {
        errors.push(issue(path, line, "`messages` must not be empty"));
        return;
    }

    validate_messages(path, line, messages, kind, errors);
    validate_tools(path, line, record.get("tools"), errors);
    if kind == SplitValidationKind::Eval {
        validate_eval_expectations(path, line, record, errors);
    }
    if let Some(metadata) = record.get("metadata") {
        if !metadata.is_object() {
            errors.push(issue(
                path,
                line,
                "`metadata` must be an object when present",
            ));
        }
    }
}

fn is_legacy_eval_case(record: &serde_json::Map<String, Value>) -> bool {
    record.contains_key("case_id")
        && record.contains_key("user_prompt")
        && record.contains_key("expected_behaviors")
}

fn validate_legacy_eval_case(
    path: &Path,
    line: usize,
    record: &serde_json::Map<String, Value>,
    errors: &mut Vec<DatasetValidationIssue>,
) {
    validate_optional_string(path, line, record, "case_id", true, errors);
    validate_optional_string(path, line, record, "user_prompt", true, errors);
    validate_optional_string(path, line, record, "input_language", false, errors);
    validate_optional_string_array(path, line, record, "tools_available", false, errors);
    validate_optional_string_array(path, line, record, "expected_behaviors", true, errors);
}

fn validate_eval_expectations(
    path: &Path,
    line: usize,
    record: &serde_json::Map<String, Value>,
    errors: &mut Vec<DatasetValidationIssue>,
) {
    if let Some(expected_behavior) = record.get("expected_behavior") {
        if !expected_behavior.is_object() {
            errors.push(issue(
                path,
                line,
                "`expected_behavior` must be an object when present",
            ));
        }
    }
    validate_optional_string_array(path, line, record, "expected_behaviors", false, errors);
}

fn validate_optional_string(
    path: &Path,
    line: usize,
    record: &serde_json::Map<String, Value>,
    key: &str,
    required: bool,
    errors: &mut Vec<DatasetValidationIssue>,
) {
    match record.get(key).and_then(Value::as_str) {
        Some(value) if !value.trim().is_empty() => {}
        Some(_) => errors.push(issue(path, line, format!("`{key}` must not be empty"))),
        None if required => errors.push(issue(path, line, format!("`{key}` is required"))),
        None => {}
    }
}

fn validate_optional_string_array(
    path: &Path,
    line: usize,
    record: &serde_json::Map<String, Value>,
    key: &str,
    required: bool,
    errors: &mut Vec<DatasetValidationIssue>,
) {
    let Some(value) = record.get(key) else {
        if required {
            errors.push(issue(path, line, format!("`{key}` is required")));
        }
        return;
    };
    let Some(values) = value.as_array() else {
        errors.push(issue(path, line, format!("`{key}` must be an array")));
        return;
    };
    if required && values.is_empty() {
        errors.push(issue(path, line, format!("`{key}` must not be empty")));
    }
    for (index, value) in values.iter().enumerate() {
        match value.as_str() {
            Some(value) if !value.trim().is_empty() => {}
            Some(_) => errors.push(issue(
                path,
                line,
                format!("`{key}`[{index}] must not be empty"),
            )),
            None => errors.push(issue(
                path,
                line,
                format!("`{key}`[{index}] must be a string"),
            )),
        }
    }
}

fn validate_messages(
    path: &Path,
    line: usize,
    messages: &[Value],
    kind: SplitValidationKind,
    errors: &mut Vec<DatasetValidationIssue>,
) {
    let mut seen_tool_call_ids = HashSet::new();
    let mut last_role = None::<String>;

    for (index, message) in messages.iter().enumerate() {
        let Some(message) = message.as_object() else {
            errors.push(issue(
                path,
                line,
                format!("messages[{index}] must be an object"),
            ));
            continue;
        };

        let role = message.get("role").and_then(Value::as_str);
        let Some(role) = role else {
            errors.push(issue(
                path,
                line,
                format!("messages[{index}].role is required"),
            ));
            continue;
        };
        last_role = Some(role.to_string());

        match role {
            "system" | "user" => validate_required_text_content(path, line, index, message, errors),
            "assistant" => validate_assistant_message(
                path,
                line,
                index,
                message,
                errors,
                &mut seen_tool_call_ids,
            ),
            "tool" => {
                validate_tool_message(path, line, index, message, errors, &seen_tool_call_ids)
            }
            _ => errors.push(issue(
                path,
                line,
                format!("messages[{index}].role `{role}` is not supported"),
            )),
        }
    }

    if kind.requires_final_assistant() && last_role.as_deref() != Some("assistant") {
        errors.push(issue(
            path,
            line,
            "record should end with a final assistant message",
        ));
    }
}

fn validate_required_text_content(
    path: &Path,
    line: usize,
    index: usize,
    message: &serde_json::Map<String, Value>,
    errors: &mut Vec<DatasetValidationIssue>,
) {
    match message.get("content").and_then(Value::as_str) {
        Some(content) if !content.trim().is_empty() => {}
        Some(_) => errors.push(issue(
            path,
            line,
            format!("messages[{index}].content must not be empty"),
        )),
        None => errors.push(issue(
            path,
            line,
            format!("messages[{index}].content must be a string"),
        )),
    }
}

fn validate_assistant_message(
    path: &Path,
    line: usize,
    index: usize,
    message: &serde_json::Map<String, Value>,
    errors: &mut Vec<DatasetValidationIssue>,
    seen_tool_call_ids: &mut HashSet<String>,
) {
    let content = message.get("content").and_then(Value::as_str);
    let tool_calls = message.get("tool_calls").and_then(Value::as_array);
    if content.is_none() {
        errors.push(issue(
            path,
            line,
            format!("messages[{index}].content must be a string"),
        ));
    }
    let has_tool_calls = tool_calls.is_some_and(|calls| !calls.is_empty());
    if content.is_some_and(|content| content.trim().is_empty()) && !has_tool_calls {
        errors.push(issue(
            path,
            line,
            format!("messages[{index}].content may be empty only when tool_calls is non-empty"),
        ));
    }

    if let Some(tool_calls) = tool_calls {
        for (call_index, call) in tool_calls.iter().enumerate() {
            validate_tool_call(
                path,
                line,
                index,
                call_index,
                call,
                errors,
                seen_tool_call_ids,
            );
        }
    }
}

fn validate_tool_call(
    path: &Path,
    line: usize,
    message_index: usize,
    call_index: usize,
    call: &Value,
    errors: &mut Vec<DatasetValidationIssue>,
    seen_tool_call_ids: &mut HashSet<String>,
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
            if !seen_tool_call_ids.insert(id.to_string()) {
                errors.push(issue(
                    path,
                    line,
                    format!("tool_call id `{id}` is duplicated"),
                ));
            }
        }
        Some(_) => errors.push(issue(path, line, "tool_call id must not be empty")),
        None => errors.push(issue(path, line, "tool_call id is required")),
    }
    if let Some(function) = call.get("function") {
        validate_openai_style_tool_call(path, line, function, errors);
    } else {
        match call.get("name").and_then(Value::as_str) {
            Some(name) if !name.trim().is_empty() => {}
            Some(_) => errors.push(issue(path, line, "tool_call name must not be empty")),
            None => errors.push(issue(path, line, "tool_call name is required")),
        }
        if !call.get("arguments").is_some_and(Value::is_object) {
            errors.push(issue(path, line, "tool_call arguments must be an object"));
        }
    }
}

fn validate_openai_style_tool_call(
    path: &Path,
    line: usize,
    function: &Value,
    errors: &mut Vec<DatasetValidationIssue>,
) {
    let Some(function) = function.as_object() else {
        errors.push(issue(path, line, "tool_call function must be an object"));
        return;
    };

    match function.get("name").and_then(Value::as_str) {
        Some(name) if !name.trim().is_empty() => {}
        Some(_) => errors.push(issue(
            path,
            line,
            "tool_call function.name must not be empty",
        )),
        None => errors.push(issue(path, line, "tool_call function.name is required")),
    }

    let Some(arguments) = function.get("arguments") else {
        errors.push(issue(
            path,
            line,
            "tool_call function.arguments is required",
        ));
        return;
    };

    if arguments.is_object() {
        return;
    }

    let Some(arguments) = arguments.as_str() else {
        errors.push(issue(
            path,
            line,
            "tool_call function.arguments must be an object or JSON object string",
        ));
        return;
    };
    if arguments.trim().is_empty() {
        errors.push(issue(
            path,
            line,
            "tool_call function.arguments must not be empty",
        ));
        return;
    }
    match serde_json::from_str::<Value>(arguments) {
        Ok(value) if value.is_object() => {}
        Ok(_) => errors.push(issue(
            path,
            line,
            "tool_call function.arguments string must decode to a JSON object",
        )),
        Err(err) => errors.push(issue(
            path,
            line,
            format!("tool_call function.arguments is not valid JSON: {err}"),
        )),
    }
}

fn validate_tool_message(
    path: &Path,
    line: usize,
    index: usize,
    message: &serde_json::Map<String, Value>,
    errors: &mut Vec<DatasetValidationIssue>,
    seen_tool_call_ids: &HashSet<String>,
) {
    let tool_call_id = message.get("tool_call_id").and_then(Value::as_str);
    match tool_call_id {
        Some(id) if !id.trim().is_empty() => {
            if !seen_tool_call_ids.contains(id) {
                errors.push(issue(
                    path,
                    line,
                    format!("messages[{index}].tool_call_id `{id}` does not match a prior assistant tool_call"),
                ));
            }
        }
        Some(_) => errors.push(issue(
            path,
            line,
            format!("messages[{index}].tool_call_id must not be empty"),
        )),
        None => errors.push(issue(
            path,
            line,
            format!("messages[{index}].tool_call_id is required"),
        )),
    }

    match message.get("name").and_then(Value::as_str) {
        Some(name) if !name.trim().is_empty() => {}
        Some(_) => errors.push(issue(
            path,
            line,
            format!("messages[{index}].name must not be empty"),
        )),
        None => errors.push(issue(
            path,
            line,
            format!("messages[{index}].name is required"),
        )),
    }

    if !message.contains_key("content") {
        errors.push(issue(
            path,
            line,
            format!("messages[{index}].content is required"),
        ));
    }
}

fn validate_tools(
    path: &Path,
    line: usize,
    tools: Option<&Value>,
    errors: &mut Vec<DatasetValidationIssue>,
) {
    let Some(tools) = tools else {
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
        match tool.get("name").and_then(Value::as_str) {
            Some(name) if !name.trim().is_empty() => {}
            Some(_) => errors.push(issue(
                path,
                line,
                format!("tools[{index}].name must not be empty"),
            )),
            None => errors.push(issue(
                path,
                line,
                format!("tools[{index}].name is required"),
            )),
        }
        if !tool.get("parameters").is_some_and(Value::is_object) {
            errors.push(issue(
                path,
                line,
                format!("tools[{index}].parameters must be an object"),
            ));
        }
    }
}

fn issue(path: &Path, line: usize, message: impl Into<String>) -> DatasetValidationIssue {
    DatasetValidationIssue {
        path: path.to_path_buf(),
        line,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    #[test]
    fn validates_canonical_chat_file() {
        let root = unique_root("valid");
        fs::create_dir_all(&root).expect("root");
        let file = root.join("train.jsonl");
        fs::write(
            &file,
            r#"{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Hi"},{"role":"assistant","content":"Hello"}]}"#,
        )
        .expect("write");

        let outcome = validate_dataset_path(&file).expect("validate");

        assert!(outcome.is_valid());
        assert_eq!(outcome.record_count(), 1);
        assert_eq!(outcome.splits[0].records, 1);
    }

    #[test]
    fn reports_line_level_schema_errors() {
        let root = unique_root("invalid");
        fs::create_dir_all(&root).expect("root");
        let file = root.join("train.jsonl");
        fs::write(
            &file,
            concat!(
                r#"{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Hi"}]}"#,
                "\n",
                r#"{"schema":"wrong","messages":[]}"#,
                "\n",
            ),
        )
        .expect("write");

        let outcome = validate_dataset_path(&file).expect("validate");

        assert!(!outcome.is_valid());
        assert_eq!(outcome.record_count(), 2);
        assert!(outcome.errors.iter().any(|error| error.line == 1));
        assert!(outcome
            .errors
            .iter()
            .any(|error| error.message.contains("schema must be")));
    }

    #[test]
    fn validates_directory_splits() {
        let root = unique_root("dir");
        fs::create_dir_all(&root).expect("root");
        fs::write(
            root.join("train.jsonl"),
            r#"{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Hi"},{"role":"assistant","content":"Hello"}]}"#,
        )
        .expect("write");
        fs::write(
            root.join("valid.jsonl"),
            r#"{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Bye"},{"role":"assistant","content":"Goodbye"}]}"#,
        )
        .expect("write");

        let outcome = validate_dataset_path(&root).expect("validate");

        assert!(outcome.is_valid());
        assert!(outcome.tuning_ready);
        assert_eq!(outcome.splits.len(), 2);
        assert_eq!(outcome.record_count(), 2);
    }

    #[test]
    fn accepts_openai_style_tool_calls() {
        let root = unique_root("openai_tool");
        fs::create_dir_all(&root).expect("root");
        fs::write(
            root.join("train.jsonl"),
            r#"{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Fetch profile."},{"role":"assistant","content":"","tool_calls":[{"id":"call_1","type":"function","function":{"name":"get_profile","arguments":"{\"field\":\"role\"}"}}]},{"role":"tool","tool_call_id":"call_1","name":"get_profile","content":"{\"role\":\"AI Engineer\"}"},{"role":"assistant","content":"AI Engineer."}]}"#,
        )
        .expect("write");

        let outcome = validate_dataset_path(&root).expect("validate");

        assert!(outcome.is_valid());
        assert_eq!(outcome.record_count(), 1);
    }

    #[test]
    fn accepts_eval_cases_without_final_assistant() {
        let root = unique_root("eval_prompt");
        fs::create_dir_all(&root).expect("root");
        fs::write(
            root.join("train.jsonl"),
            r#"{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Hi"},{"role":"assistant","content":"Hello"}]}"#,
        )
        .expect("write");
        fs::write(
            root.join("eval_cases.jsonl"),
            r#"{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Say hello."}],"expected_behavior":{"answer_language":"en"}}"#,
        )
        .expect("write");

        let outcome = validate_dataset_path(&root).expect("validate");

        assert!(outcome.is_valid());
        assert_eq!(outcome.record_count(), 2);
    }

    #[test]
    fn accepts_legacy_eval_case_records() {
        let root = unique_root("legacy_eval");
        fs::create_dir_all(&root).expect("root");
        fs::write(
            root.join("train.jsonl"),
            r#"{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Hi"},{"role":"assistant","content":"Hello"}]}"#,
        )
        .expect("write");
        fs::write(
            root.join("eval_cases.jsonl"),
            r#"{"case_id":"case-1","input_language":"zh-TW","user_prompt":"請介紹 Hiro。","tools_available":["get_profile(field)"],"expected_behaviors":["uses zh-TW","does not hallucinate"]}"#,
        )
        .expect("write");

        let outcome = validate_dataset_path(&root).expect("validate");

        assert!(outcome.is_valid());
        assert_eq!(outcome.record_count(), 2);
    }

    fn unique_root(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("tentgent-dataset-validate-{label}-{nanos}"))
    }
}
