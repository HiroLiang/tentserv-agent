use std::{fs, path::Path};

use super::error::DatasetError;

pub const DEFAULT_TEMPLATE_TASK: &str = "chat";
pub const DEFAULT_TEMPLATE_LANGUAGE: &str = "en";
pub const DATASET_TEMPLATE_VERSION: &str = "tentgent.dataset.synth.v1";

#[derive(Debug, Clone)]
pub struct DatasetTemplateRequest {
    pub task: String,
    pub language: String,
}

impl DatasetTemplateRequest {
    pub fn new(task: Option<String>, language: Option<String>) -> Self {
        Self {
            task: normalize_hint(task, DEFAULT_TEMPLATE_TASK),
            language: normalize_hint(language, DEFAULT_TEMPLATE_LANGUAGE),
        }
    }
}

pub fn render_dataset_template(request: &DatasetTemplateRequest) -> String {
    format!(
        r#"# Tentgent Dataset Generation Template

Template version: `{template_version}`
Canonical schema: `tentgent.chat.v1`
Task/domain hint: `{task}`
Language/content hint: `{language}`

You are generating tuning data for Tentgent.

Return only JSONL. Do not wrap the output in Markdown fences. Each line must be one complete JSON object.

Required output rules:

- Use `schema: "tentgent.chat.v1"` on every record.
- Use `messages` as the only training conversation body.
- Supported message roles are `system`, `user`, `assistant`, and `tool`.
- Each record should end with a final `assistant` answer.
- Use `tools` only to describe tools available to that record.
- Use assistant `tool_calls` for tool requests.
- Use `tool` messages for tool results.
- Keep `metadata` factual and non-training-critical.
- Do not output MLX, PEFT, ChatML, OpenAI-specific, or Anthropic-specific rendered prompt text.
- Keep generated content in language/content style `{language}` unless the task requires quoting another language.
- Prefer realistic, diverse, non-duplicated examples for task/domain `{task}`.

Minimal valid JSONL example:

{{"schema":"tentgent.chat.v1","id":"example-001","messages":[{{"role":"system","content":"You are a concise assistant."}},{{"role":"user","content":"Say hello in one short sentence."}},{{"role":"assistant","content":"Hello! How can I help?"}}],"metadata":{{"task":"{task}","language":"{language}","source":"synthetic"}}}}

When generating many rows:

- Use stable, unique `id` values.
- Keep one conversation per line.
- Avoid blank lines.
- Avoid trailing commas.
- Make sure every line parses as JSON independently.
- Before finalizing, internally check that the output would pass `tentgent dataset validate`.
"#,
        template_version = DATASET_TEMPLATE_VERSION,
        task = request.task,
        language = request.language,
    )
}

pub fn write_dataset_template(path: &Path, body: &str) -> Result<(), DatasetError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(path, body)?;
    Ok(())
}

fn normalize_hint(value: Option<String>, default: &str) -> String {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}
