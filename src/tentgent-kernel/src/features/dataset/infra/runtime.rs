use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;

use serde_json::Value;

use crate::features::dataset::domain::{
    DatasetEvalRequest, DatasetPromptSource, DatasetRuntimeDebug, DatasetSynthPromptRequest,
    DatasetSynthRuntimeOutput,
};
use crate::features::dataset::ports::{
    DatasetEvalRuntimeClient, DatasetEvalRuntimeRequest, DatasetPortFuture,
    DatasetSynthPromptRuntimeRequest, DatasetSynthRuntimeClient, DatasetSynthRuntimeRequest,
};
use crate::features::runtime::domain::{PythonRuntimeLayout, RuntimeEntrypoint};
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::foundation::error::KernelResult;

use super::error::dataset_runtime_error;

const DAEMON_TOKEN_ENV_VAR: &str = "TENTGENT_DAEMON_TOKEN";
const MAX_PROGRESS_EVENTS: usize = 200;
const MAX_STDERR_LINES: usize = 200;

/// Executes provider-backed dataset synthesis through `tentgent-dataset-synth`.
pub struct PythonDatasetSynthRuntimeClient<'a> {
    executable_resolver: &'a dyn RuntimeExecutableResolver,
}

impl<'a> PythonDatasetSynthRuntimeClient<'a> {
    pub fn new(executable_resolver: &'a dyn RuntimeExecutableResolver) -> Self {
        Self {
            executable_resolver,
        }
    }

    fn render_synth_prompt_blocking(
        &self,
        request: DatasetSynthPromptRuntimeRequest,
    ) -> KernelResult<String> {
        let entrypoint = self
            .executable_resolver
            .entrypoint_path(&request.runtime, RuntimeEntrypoint::DatasetSynth)?;
        let output = command_for_synth_prompt(entrypoint, &request.runtime, request.request)
            .output()
            .map_err(|err| {
                dataset_runtime_error(format!("failed to run dataset synth prompt runtime: {err}"))
            })?;

        if !output.status.success() {
            return Err(dataset_runtime_error(format_process_failure(
                "dataset synth prompt runtime exited",
                output.status.code(),
                &output.stderr,
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn synthesize_dataset_blocking(
        &self,
        request: DatasetSynthRuntimeRequest,
    ) -> KernelResult<DatasetSynthRuntimeOutput> {
        let entrypoint = self
            .executable_resolver
            .entrypoint_path(&request.runtime, RuntimeEntrypoint::DatasetSynth)?;
        let mut child = command_for_synth(entrypoint, &request)
            .spawn()
            .map_err(|err| {
                dataset_runtime_error(format!("failed to start dataset synth runtime: {err}"))
            })?;
        let mut stdout = child.stdout.take().ok_or_else(|| {
            dataset_runtime_error("failed to capture dataset synth runtime stdout")
        })?;
        let mut stderr = child.stderr.take().ok_or_else(|| {
            dataset_runtime_error("failed to capture dataset synth runtime stderr")
        })?;

        let stdout_task = thread::spawn(move || {
            let mut stdout_text = String::new();
            stdout.read_to_string(&mut stdout_text).map(|_| stdout_text)
        });
        let stderr_task = thread::spawn(move || read_synth_stderr(&mut stderr));

        let status = child.wait().map_err(|err| {
            dataset_runtime_error(format!("failed to wait for dataset synth runtime: {err}"))
        })?;
        let stdout = stdout_task
            .join()
            .map_err(|_| dataset_runtime_error("dataset synth stdout reader panicked"))?
            .map_err(|err| {
                dataset_runtime_error(format!("failed to read dataset synth stdout: {err}"))
            })?;
        let stderr = stderr_task
            .join()
            .map_err(|_| dataset_runtime_error("dataset synth stderr reader panicked"))?
            .map_err(|err| {
                dataset_runtime_error(format!("failed to read dataset synth stderr: {err}"))
            })?;

        if !status.success() {
            return Err(dataset_runtime_error(format_process_failure(
                "dataset synth runtime exited",
                status.code(),
                stderr.lines.join("\n").as_bytes(),
            )));
        }

        let outcome = serde_json::from_str::<Value>(stdout.trim()).map_err(|err| {
            dataset_runtime_error(format!(
                "dataset synth runtime returned invalid JSON: {err}"
            ))
        })?;
        Ok(DatasetSynthRuntimeOutput {
            outcome,
            progress_events: stderr.progress_events,
            progress_truncated: stderr.progress_truncated,
        })
    }
}

impl DatasetSynthRuntimeClient for PythonDatasetSynthRuntimeClient<'_> {
    fn render_synth_prompt<'a>(
        &'a self,
        request: DatasetSynthPromptRuntimeRequest,
    ) -> DatasetPortFuture<'a, String> {
        Box::pin(async move { self.render_synth_prompt_blocking(request) })
    }

    fn synthesize_dataset<'a>(
        &'a self,
        request: DatasetSynthRuntimeRequest,
    ) -> DatasetPortFuture<'a, DatasetSynthRuntimeOutput> {
        Box::pin(async move { self.synthesize_dataset_blocking(request) })
    }
}

/// Executes provider-backed dataset evaluation through `tentgent-dataset-eval`.
pub struct PythonDatasetEvalRuntimeClient<'a> {
    executable_resolver: &'a dyn RuntimeExecutableResolver,
}

impl<'a> PythonDatasetEvalRuntimeClient<'a> {
    pub fn new(executable_resolver: &'a dyn RuntimeExecutableResolver) -> Self {
        Self {
            executable_resolver,
        }
    }

    fn evaluate_dataset_blocking(&self, request: DatasetEvalRuntimeRequest) -> KernelResult<Value> {
        let entrypoint = self
            .executable_resolver
            .entrypoint_path(&request.runtime, RuntimeEntrypoint::DatasetEval)?;
        let output = command_for_eval(entrypoint, &request)
            .output()
            .map_err(|err| {
                dataset_runtime_error(format!("failed to run dataset eval runtime: {err}"))
            })?;

        if !output.status.success() {
            return Err(dataset_runtime_error(format_process_failure(
                "dataset eval runtime exited",
                output.status.code(),
                &output.stderr,
            )));
        }

        serde_json::from_slice::<Value>(&output.stdout).map_err(|err| {
            dataset_runtime_error(format!("dataset eval runtime returned invalid JSON: {err}"))
        })
    }
}

impl DatasetEvalRuntimeClient for PythonDatasetEvalRuntimeClient<'_> {
    fn evaluate_dataset<'a>(
        &'a self,
        request: DatasetEvalRuntimeRequest,
    ) -> DatasetPortFuture<'a, Value> {
        Box::pin(async move { self.evaluate_dataset_blocking(request) })
    }

    fn runtime_debug(&self, error_detail: &str) -> Option<DatasetRuntimeDebug> {
        debug_from_stderr(
            error_detail
                .lines()
                .map(str::to_string)
                .collect::<Vec<_>>()
                .as_slice(),
            None,
        )
    }
}

fn command_for_synth_prompt(
    entrypoint: std::path::PathBuf,
    runtime: &PythonRuntimeLayout,
    request: DatasetSynthPromptRequest,
) -> Command {
    let mut command = Command::new(entrypoint);
    command
        .current_dir(&runtime.project_dir)
        .env_remove(DAEMON_TOKEN_ENV_VAR)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("--print-prompt")
        .arg("--split")
        .arg(request.split.as_str());
    append_synth_count_args(&mut command, &request.counts);
    append_prompt_source_args(&mut command, &request.prompt_source);
    command
}

fn command_for_synth(
    entrypoint: std::path::PathBuf,
    request: &DatasetSynthRuntimeRequest,
) -> Command {
    let mut command = Command::new(entrypoint);
    command
        .current_dir(&request.runtime.project_dir)
        .env_remove(DAEMON_TOKEN_ENV_VAR)
        .env(
            request.auth.secret.provider.env_var(),
            request.auth.secret.secret(),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("--provider")
        .arg(request.request.provider.as_str())
        .arg("--model")
        .arg(&request.request.provider_model)
        .arg("--output")
        .arg(&request.request.output_dir)
        .arg("--split")
        .arg(request.request.split.as_str())
        .arg("--temperature")
        .arg(request.request.temperature.to_string())
        .arg("--timeout-seconds")
        .arg(request.request.timeout_seconds.to_string())
        .arg("--retries")
        .arg(request.request.retries.to_string())
        .arg("--progress-json");
    append_synth_count_args(&mut command, &request.request.counts);
    append_prompt_source_args(&mut command, &request.request.prompt_source);
    if let Some(max_tokens) = request.request.max_tokens {
        command.arg("--max-tokens").arg(max_tokens.to_string());
    }
    command
}

fn command_for_eval(
    entrypoint: std::path::PathBuf,
    request: &DatasetEvalRuntimeRequest,
) -> Command {
    let DatasetEvalRequest {
        provider,
        provider_model,
        input,
        output_dir,
        split,
        max_records,
        criteria,
        max_tokens,
        temperature,
        timeout_seconds,
    } = &request.request;
    let mut command = Command::new(entrypoint);
    command
        .current_dir(&request.runtime.project_dir)
        .env_remove(DAEMON_TOKEN_ENV_VAR)
        .env(
            request.auth.secret.provider.env_var(),
            request.auth.secret.secret(),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("--provider")
        .arg(provider.as_str())
        .arg("--model")
        .arg(provider_model)
        .arg("--input")
        .arg(input)
        .arg("--output")
        .arg(output_dir)
        .arg("--split")
        .arg(split.as_str())
        .arg("--max-records")
        .arg(max_records.to_string())
        .arg("--temperature")
        .arg(temperature.to_string())
        .arg("--timeout-seconds")
        .arg(timeout_seconds.to_string());
    if let Some(criteria) = criteria {
        command.arg("--criteria").arg(criteria);
    }
    if let Some(max_tokens) = max_tokens {
        command.arg("--max-tokens").arg(max_tokens.to_string());
    }
    command
}

fn append_synth_count_args(
    command: &mut Command,
    counts: &crate::features::dataset::domain::DatasetSynthCounts,
) {
    if let Some(count) = counts.count {
        command.arg("--count").arg(count.to_string());
    }
    if let Some(count) = counts.train_count {
        command.arg("--train-count").arg(count.to_string());
    }
    if let Some(count) = counts.valid_count {
        command.arg("--valid-count").arg(count.to_string());
    }
    if let Some(count) = counts.test_count {
        command.arg("--test-count").arg(count.to_string());
    }
    if let Some(count) = counts.eval_count {
        command.arg("--eval-count").arg(count.to_string());
    }
}

fn append_prompt_source_args(command: &mut Command, source: &DatasetPromptSource) {
    match source {
        DatasetPromptSource::Brief(brief) => {
            command.arg("--brief").arg(brief);
        }
        DatasetPromptSource::SpecPath(path) => {
            command.arg("--spec").arg(path);
        }
    };
}

struct SynthStderr {
    lines: Vec<String>,
    progress_events: Vec<Value>,
    progress_truncated: bool,
}

fn read_synth_stderr(stderr: &mut impl Read) -> std::io::Result<SynthStderr> {
    let mut text = String::new();
    stderr.read_to_string(&mut text)?;
    let mut lines = Vec::new();
    let mut progress_events = Vec::new();
    let mut progress_truncated = false;

    for line in text.lines() {
        if let Ok(event) = serde_json::from_str::<Value>(line) {
            if event.get("type").and_then(Value::as_str) == Some("progress") {
                if progress_events.len() == MAX_PROGRESS_EVENTS {
                    progress_events.remove(0);
                    progress_truncated = true;
                }
                progress_events.push(event);
                continue;
            }
        }
        if lines.len() == MAX_STDERR_LINES {
            lines.remove(0);
        }
        lines.push(line.to_string());
    }

    Ok(SynthStderr {
        lines,
        progress_events,
        progress_truncated,
    })
}

fn format_process_failure(prefix: &str, code: Option<i32>, stderr: &[u8]) -> String {
    let status = code
        .map(|code| format!("with status {code}"))
        .unwrap_or_else(|| "without an exit status".to_string());
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    if stderr.is_empty() {
        format!("{prefix} {status}")
    } else {
        format!("{prefix} {status}: {stderr}")
    }
}

fn debug_from_stderr(
    lines: &[String],
    output_path: Option<&std::path::Path>,
) -> Option<DatasetRuntimeDebug> {
    let debug_dir = lines.iter().rev().find_map(|line| {
        line.strip_prefix("provider debug written to ")
            .map(|path| std::path::PathBuf::from(path.trim()))
    });
    let debug_dir = debug_dir?;
    Some(DatasetRuntimeDebug {
        output_path: output_path.map(std::path::Path::to_path_buf),
        prompt_path: Some(debug_dir.join("prompt.md")),
        provider_output_path: Some(debug_dir.join("provider-output.raw.txt")),
        error_path: Some(debug_dir.join("error.txt")),
        debug_dir: Some(debug_dir),
    })
}
