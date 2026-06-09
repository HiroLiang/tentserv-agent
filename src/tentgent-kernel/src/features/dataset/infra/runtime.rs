use std::{fs, path::Path};

use serde_json::{json, Value};

use crate::features::{
    auth::domain::Provider,
    cloud::{
        domain::{CloudChatMessage, CloudChatRequest},
        infra::ReqwestCloudModelClient,
    },
    dataset::{
        domain::{
            DatasetEvalRequest, DatasetEvalSplit, DatasetPromptSource, DatasetRuntimeDebug,
            DatasetSplitKind, DatasetSynthCounts, DatasetSynthPromptRequest,
            DatasetSynthRuntimeOutput,
        },
        ports::{
            DatasetEvalRuntimeClient, DatasetEvalRuntimeRequest, DatasetPortFuture,
            DatasetSynthPromptRuntimeRequest, DatasetSynthRuntimeClient,
            DatasetSynthRuntimeRequest,
        },
        templates::render_dataset_generation_template,
    },
};
use crate::foundation::error::{KernelError, KernelResult};

use super::error::{dataset_runtime_error, path_error};

const DEFAULT_SYNTH_MAX_TOKENS: u32 = 4096;
const DEFAULT_EVAL_MAX_TOKENS: u32 = 4096;

/// Provider-backed dataset synthesis implemented directly in Rust.
#[derive(Debug, Clone, Copy, Default)]
pub struct CloudDatasetSynthRuntimeClient;

impl CloudDatasetSynthRuntimeClient {
    pub fn new() -> Self {
        Self
    }
}

impl DatasetSynthRuntimeClient for CloudDatasetSynthRuntimeClient {
    fn render_synth_prompt(
        &self,
        request: DatasetSynthPromptRuntimeRequest,
    ) -> DatasetPortFuture<'_, String> {
        Box::pin(async move { Ok(render_synth_prompt(&request.request)?) })
    }

    fn synthesize_dataset(
        &self,
        request: DatasetSynthRuntimeRequest,
    ) -> DatasetPortFuture<'_, DatasetSynthRuntimeOutput> {
        Box::pin(async move {
            fs::create_dir_all(&request.request.output_dir).map_err(|err| {
                path_error(
                    "create generated dataset output directory",
                    &request.request.output_dir,
                    err,
                )
            })?;
            let client = ReqwestCloudModelClient::new().map_err(dataset_cloud_error)?;
            let jobs = synth_jobs(request.request.split, &request.request.counts);
            let mut files = Vec::new();
            let mut events = Vec::new();

            for (split, count) in jobs {
                let prompt = render_synth_prompt_for_split(&request.request, split, count)?;
                events.push(json!({
                    "event": "request_started",
                    "provider": request.request.provider.as_str(),
                    "model": request.request.provider_model,
                    "split": split.as_str(),
                    "count": count,
                }));
                let response = client
                    .complete_chat(
                        CloudChatRequest {
                            provider: provider_auth_provider(request.request.provider),
                            model: request.request.provider_model.clone(),
                            messages: vec![CloudChatMessage::text("user", prompt)],
                            max_tokens: request
                                .request
                                .max_tokens
                                .or(Some(DEFAULT_SYNTH_MAX_TOKENS)),
                            temperature: Some(request.request.temperature),
                            stream: false,
                            response_modalities: None,
                            audio: None,
                        },
                        request.auth.secret.secret(),
                    )
                    .await
                    .map_err(dataset_cloud_error)?;
                let text = strip_markdown_fences(&response.text);
                let file_name = split.file_name();
                let path = request.request.output_dir.join(file_name);
                fs::write(&path, text)
                    .map_err(|err| path_error("write generated dataset split", &path, err))?;
                files.push(json!({
                    "split": split.as_str(),
                    "path": path.display().to_string(),
                    "records_requested": count,
                    "finish_reason": response.finish_reason,
                }));
                events.push(json!({
                    "event": "file_written",
                    "split": split.as_str(),
                    "path": path.display().to_string(),
                }));
            }

            Ok(DatasetSynthRuntimeOutput {
                outcome: json!({
                    "provider": request.request.provider.as_str(),
                    "model": request.request.provider_model,
                    "output_dir": request.request.output_dir.display().to_string(),
                    "files": files,
                }),
                progress_events: events,
                progress_truncated: false,
            })
        })
    }
}

/// Provider-backed dataset evaluation implemented directly in Rust.
#[derive(Debug, Clone, Copy, Default)]
pub struct CloudDatasetEvalRuntimeClient;

impl CloudDatasetEvalRuntimeClient {
    pub fn new() -> Self {
        Self
    }
}

impl DatasetEvalRuntimeClient for CloudDatasetEvalRuntimeClient {
    fn evaluate_dataset(&self, request: DatasetEvalRuntimeRequest) -> DatasetPortFuture<'_, Value> {
        Box::pin(async move {
            fs::create_dir_all(&request.request.output_dir).map_err(|err| {
                path_error(
                    "create dataset evaluation output directory",
                    &request.request.output_dir,
                    err,
                )
            })?;
            let prompt = render_eval_prompt(&request.request)?;
            let prompt_path = request.request.output_dir.join("prompt.md");
            fs::write(&prompt_path, &prompt)
                .map_err(|err| path_error("write dataset evaluation prompt", &prompt_path, err))?;
            let client = ReqwestCloudModelClient::new().map_err(dataset_cloud_error)?;
            let response = client
                .complete_chat(
                    CloudChatRequest {
                        provider: provider_auth_provider(request.request.provider),
                        model: request.request.provider_model.clone(),
                        messages: vec![CloudChatMessage::text("user", prompt)],
                        max_tokens: request.request.max_tokens.or(Some(DEFAULT_EVAL_MAX_TOKENS)),
                        temperature: Some(request.request.temperature),
                        stream: false,
                        response_modalities: None,
                        audio: None,
                    },
                    request.auth.secret.secret(),
                )
                .await
                .map_err(dataset_cloud_error)?;
            let report_path = request.request.output_dir.join("report.json");
            let report = json!({
                "provider": request.request.provider.as_str(),
                "model": request.request.provider_model,
                "input": request.request.input.display().to_string(),
                "split": request.request.split.as_str(),
                "finish_reason": response.finish_reason,
                "report_text": response.text,
                "prompt_path": prompt_path.display().to_string(),
            });
            fs::write(
                &report_path,
                serde_json::to_vec_pretty(&report).map_err(|err| {
                    dataset_runtime_error(format!("serialize eval report failed: {err}"))
                })?,
            )
            .map_err(|err| path_error("write dataset evaluation report", &report_path, err))?;

            Ok(json!({
                "provider": report["provider"],
                "model": report["model"],
                "input": report["input"],
                "split": report["split"],
                "report_path": report_path.display().to_string(),
                "prompt_path": prompt_path.display().to_string(),
                "report": report,
            }))
        })
    }

    fn runtime_debug(&self, error_detail: &str) -> Option<DatasetRuntimeDebug> {
        runtime_debug_from_detail(error_detail)
    }
}

fn render_synth_prompt(request: &DatasetSynthPromptRequest) -> KernelResult<String> {
    let source = prompt_source_text(&request.prompt_source)?;
    let mut prompt = render_dataset_generation_template(
        &crate::features::dataset::domain::DatasetTemplateRequest::new(Some(source.clone()), None),
    )
    .body;
    prompt.push_str("\n\nGeneration request:\n");
    prompt.push_str(&format!("- Source brief/spec: {source}\n"));
    prompt.push_str(&format!(
        "- Split plan: {}\n",
        synth_counts_label(request.split, &request.counts)
    ));
    prompt.push_str(
        "- Return only JSONL for the requested split unless split-specific counts are listed.\n",
    );
    Ok(prompt)
}

fn render_synth_prompt_for_split(
    request: &crate::features::dataset::domain::DatasetSynthRequest,
    split: DatasetSplitKind,
    count: u32,
) -> KernelResult<String> {
    let prompt = render_synth_prompt(&DatasetSynthPromptRequest {
        prompt_source: request.prompt_source.clone(),
        split,
        counts: DatasetSynthCounts {
            count: Some(count),
            ..DatasetSynthCounts::default()
        },
    })?;
    Ok(format!(
        "{prompt}\n\nNow generate exactly {count} JSONL records for `{}`. Use filename semantics `{}`.",
        split.as_str(),
        split.file_name()
    ))
}

fn render_eval_prompt(request: &DatasetEvalRequest) -> KernelResult<String> {
    let sample = dataset_sample(&request.input, request.split, request.max_records)?;
    let criteria = request.criteria.as_deref().unwrap_or(
        "Check schema validity, instruction quality, diversity, safety, and tuning readiness.",
    );
    Ok(format!(
        "Evaluate this Tentgent dataset.\n\nInput: {}\nSplit: {}\nMax records: {}\nCriteria: {}\n\nReturn concise JSON with summary, issues, and recommendations.\n\nDataset sample:\n{}",
        request.input.display(),
        request.split.as_str(),
        request.max_records,
        criteria,
        sample
    ))
}

fn prompt_source_text(source: &DatasetPromptSource) -> KernelResult<String> {
    match source {
        DatasetPromptSource::Brief(value) => Ok(value.clone()),
        DatasetPromptSource::SpecPath(path) => fs::read_to_string(path)
            .map_err(|err| path_error("read dataset synthesis spec", path, err)),
    }
}

fn synth_jobs(
    split: DatasetSplitKind,
    counts: &DatasetSynthCounts,
) -> Vec<(DatasetSplitKind, u32)> {
    let explicit = [
        (DatasetSplitKind::Train, counts.train_count),
        (DatasetSplitKind::Valid, counts.valid_count),
        (DatasetSplitKind::Test, counts.test_count),
        (DatasetSplitKind::EvalCases, counts.eval_count),
    ]
    .into_iter()
    .filter_map(|(split, count)| count.map(|count| (split, count)))
    .collect::<Vec<_>>();
    if explicit.is_empty() {
        vec![(split, counts.count.unwrap_or(10))]
    } else {
        explicit
    }
}

fn synth_counts_label(split: DatasetSplitKind, counts: &DatasetSynthCounts) -> String {
    synth_jobs(split, counts)
        .into_iter()
        .map(|(split, count)| format!("{}={count}", split.as_str()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn dataset_sample(path: &Path, split: DatasetEvalSplit, max_records: u32) -> KernelResult<String> {
    let path = match split {
        DatasetEvalSplit::Train => {
            path.join(crate::features::dataset::domain::TRAIN_SPLIT_FILENAME)
        }
        DatasetEvalSplit::Valid => {
            path.join(crate::features::dataset::domain::VALID_SPLIT_FILENAME)
        }
        DatasetEvalSplit::Test => path.join(crate::features::dataset::domain::TEST_SPLIT_FILENAME),
        DatasetEvalSplit::EvalCases => {
            path.join(crate::features::dataset::domain::EVAL_CASES_SPLIT_FILENAME)
        }
        DatasetEvalSplit::All => path.to_path_buf(),
    };
    if path.is_file() {
        return sample_file(&path, max_records);
    }
    let mut chunks = Vec::new();
    for file_name in [
        crate::features::dataset::domain::TRAIN_SPLIT_FILENAME,
        crate::features::dataset::domain::VALID_SPLIT_FILENAME,
        crate::features::dataset::domain::TEST_SPLIT_FILENAME,
        crate::features::dataset::domain::EVAL_CASES_SPLIT_FILENAME,
    ] {
        let file_path = path.join(file_name);
        if file_path.is_file() {
            chunks.push(format!(
                "## {file_name}\n{}",
                sample_file(&file_path, max_records)?
            ));
        }
    }
    if chunks.is_empty() {
        return Err(path_error(
            "read dataset sample",
            &path,
            std::io::Error::new(std::io::ErrorKind::NotFound, "no dataset split files found"),
        ));
    }
    Ok(chunks.join("\n\n"))
}

fn sample_file(path: &Path, max_records: u32) -> KernelResult<String> {
    let contents =
        fs::read_to_string(path).map_err(|err| path_error("read dataset file", path, err))?;
    Ok(contents
        .lines()
        .take(max_records as usize)
        .collect::<Vec<_>>()
        .join("\n"))
}

fn provider_auth_provider(provider: crate::features::dataset::domain::DatasetProvider) -> Provider {
    match provider {
        crate::features::dataset::domain::DatasetProvider::OpenAI => Provider::OpenAI,
        crate::features::dataset::domain::DatasetProvider::Anthropic => Provider::Anthropic,
        crate::features::dataset::domain::DatasetProvider::Gemini => Provider::Gemini,
    }
}

fn strip_markdown_fences(text: &str) -> String {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }
    let mut lines = trimmed.lines().collect::<Vec<_>>();
    if lines
        .first()
        .is_some_and(|line| line.trim_start().starts_with("```"))
    {
        lines.remove(0);
    }
    if lines
        .last()
        .is_some_and(|line| line.trim_start().starts_with("```"))
    {
        lines.pop();
    }
    lines.join("\n").trim().to_string()
}

fn runtime_debug_from_detail(error_detail: &str) -> Option<DatasetRuntimeDebug> {
    let value: Value = serde_json::from_str(error_detail).ok()?;
    Some(DatasetRuntimeDebug {
        output_path: value
            .get("output_path")
            .and_then(Value::as_str)
            .map(Into::into),
        debug_dir: value
            .get("debug_dir")
            .and_then(Value::as_str)
            .map(Into::into),
        prompt_path: value
            .get("prompt_path")
            .and_then(Value::as_str)
            .map(Into::into),
        provider_output_path: value
            .get("provider_output_path")
            .and_then(Value::as_str)
            .map(Into::into),
        error_path: value
            .get("error_path")
            .and_then(Value::as_str)
            .map(Into::into),
    })
}

fn dataset_cloud_error(error: KernelError) -> KernelError {
    match error {
        KernelError::RuntimeStateUnavailable(message)
        | KernelError::ChatRuntimeUnavailable(message) => dataset_runtime_error(message),
        KernelError::UnsupportedTarget(message) => dataset_runtime_error(message),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::dataset::domain::{DatasetPromptSource, DatasetSplitKind};

    #[tokio::test]
    async fn render_prompt_includes_split_counts() {
        let client = CloudDatasetSynthRuntimeClient::new();
        let prompt = client
            .render_synth_prompt(DatasetSynthPromptRuntimeRequest {
                runtime: crate::features::runtime::domain::PythonRuntimeLayout {
                    project_dir: "/tmp".into(),
                    env_dir: "/tmp/venv".into(),
                    source:
                        crate::features::runtime::domain::PythonRuntimeSource::DevelopmentSource,
                },
                request: DatasetSynthPromptRequest {
                    prompt_source: DatasetPromptSource::Brief("support chat".to_string()),
                    split: DatasetSplitKind::Train,
                    counts: DatasetSynthCounts {
                        count: Some(3),
                        ..DatasetSynthCounts::default()
                    },
                },
            })
            .await
            .expect("prompt");

        assert!(prompt.contains("support chat"));
        assert!(prompt.contains("train=3"));
    }

    #[test]
    fn strip_fences_removes_jsonl_code_block() {
        assert_eq!(
            strip_markdown_fences("```jsonl\n{\"x\":1}\n```"),
            "{\"x\":1}"
        );
    }
}
