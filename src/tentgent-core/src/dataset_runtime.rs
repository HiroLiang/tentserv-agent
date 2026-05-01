use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
};

use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
};

use crate::{
    auth::{AuthError, AuthManager, KeySource, KeyValidationState, Provider},
    runtime_assets::{PythonRuntime, PythonRuntimeSource, RuntimeAssetError},
};

const DAEMON_TOKEN_ENV_VAR: &str = "TENTGENT_DAEMON_TOKEN";
const MAX_PROGRESS_EVENTS: usize = 200;
const MAX_STDERR_LINES: usize = 200;

#[derive(Clone, Debug)]
pub struct DatasetRuntimeAuth {
    provider: Provider,
    source: KeySource,
    normalized_provider: &'static str,
    secret: String,
}

impl DatasetRuntimeAuth {
    pub fn provider(&self) -> Provider {
        self.provider
    }

    pub fn source(&self) -> KeySource {
        self.source
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DatasetSynthCounts {
    pub count: Option<u32>,
    pub train_count: Option<u32>,
    pub valid_count: Option<u32>,
    pub test_count: Option<u32>,
    pub eval_count: Option<u32>,
}

impl DatasetSynthCounts {
    pub fn expected_jobs(&self) -> u64 {
        let split_jobs = [
            self.train_count,
            self.valid_count,
            self.test_count,
            self.eval_count,
        ]
        .into_iter()
        .flatten()
        .count();
        if split_jobs == 0 {
            1
        } else {
            split_jobs as u64
        }
    }
}

#[derive(Clone, Debug)]
pub struct DatasetSynthRuntimeRequest {
    pub auth: DatasetRuntimeAuth,
    pub model: String,
    pub output: PathBuf,
    pub brief: Option<String>,
    pub spec: Option<PathBuf>,
    pub split: String,
    pub counts: DatasetSynthCounts,
    pub max_tokens: Option<u32>,
    pub temperature: f32,
    pub timeout_seconds: f32,
    pub retries: u32,
}

#[derive(Clone, Debug)]
pub struct DatasetSynthPromptRuntimeRequest {
    pub brief: Option<String>,
    pub spec: Option<PathBuf>,
    pub split: String,
    pub counts: DatasetSynthCounts,
}

#[derive(Clone, Debug)]
pub struct DatasetEvalRuntimeRequest {
    pub auth: DatasetRuntimeAuth,
    pub model: String,
    pub input: PathBuf,
    pub output: PathBuf,
    pub split: String,
    pub max_records: u32,
    pub criteria: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: f32,
    pub timeout_seconds: f32,
}

#[derive(Clone, Debug)]
pub struct DatasetSynthRuntimeOutput {
    pub outcome: Value,
    pub progress_events: Vec<Value>,
    pub progress_truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatasetRuntimeDebug {
    pub output_path: Option<PathBuf>,
    pub debug_dir: Option<PathBuf>,
    pub prompt_path: Option<PathBuf>,
    pub provider_output_path: Option<PathBuf>,
    pub error_path: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum DatasetRuntimeError {
    #[error("failed to resolve Python runtime assets: {0}")]
    RuntimeAssets(#[from] RuntimeAssetError),
    #[error("{label} is missing at `{path}`; {hint}")]
    MissingPythonInterpreter {
        label: &'static str,
        path: PathBuf,
        hint: &'static str,
    },
    #[error("failed to access provider auth: {0}")]
    Auth(#[from] AuthError),
    #[error("unsupported dataset provider `{0}`")]
    UnsupportedProvider(String),
    #[error("{provider} key is missing for {purpose}; run `tentgent auth {cli_name} set` or set `{env_var}` before launch")]
    ProviderAuthMissing {
        provider: String,
        cli_name: &'static str,
        env_var: &'static str,
        purpose: &'static str,
    },
    #[error("{provider} key from {key_source} is invalid for {purpose}: {reason}")]
    ProviderAuthInvalid {
        provider: String,
        key_source: KeySource,
        purpose: &'static str,
        reason: String,
    },
    #[error("{provider} key from {key_source} could not be verified for {purpose}: {reason}")]
    ProviderAuthUnknown {
        provider: String,
        key_source: KeySource,
        purpose: &'static str,
        reason: String,
    },
    #[error("failed to spawn {tool}: {source}")]
    Spawn {
        tool: &'static str,
        source: std::io::Error,
    },
    #[error("failed to wait for {tool}: {source}")]
    Wait {
        tool: &'static str,
        source: std::io::Error,
    },
    #[error("failed to read {tool} stdout: {source}")]
    StdoutRead {
        tool: &'static str,
        source: std::io::Error,
    },
    #[error("failed to read {tool} stderr: {source}")]
    StderrRead {
        tool: &'static str,
        source: std::io::Error,
    },
    #[error("{tool} exited with status {status}: {stderr}")]
    HelperExit {
        tool: &'static str,
        status: ExitStatus,
        stderr: String,
        debug: Option<DatasetRuntimeDebug>,
    },
    #[error("{tool} returned invalid JSON: {message}")]
    InvalidJson { tool: &'static str, message: String },
}

impl DatasetRuntimeError {
    pub fn is_provider_auth_error(&self) -> bool {
        matches!(
            self,
            Self::ProviderAuthMissing { .. }
                | Self::ProviderAuthInvalid { .. }
                | Self::ProviderAuthUnknown { .. }
        )
    }

    pub fn debug(&self) -> Option<&DatasetRuntimeDebug> {
        match self {
            Self::HelperExit { debug, .. } => debug.as_ref(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeCommandParts {
    args: Vec<String>,
    env: Vec<(String, String)>,
    env_remove: Vec<String>,
}

pub async fn preflight_dataset_provider_auth(
    provider_name: &str,
    purpose: &'static str,
) -> Result<DatasetRuntimeAuth, DatasetRuntimeError> {
    let (provider, normalized_provider) = auth_provider_for_dataset(provider_name)?;
    let auth = AuthManager::new()?;
    let Some((source, secret)) = auth.effective_secret(provider)? else {
        return Err(provider_auth_missing(provider, purpose));
    };

    match auth.validate_secret(provider, &secret).await {
        KeyValidationState::Verified => Ok(DatasetRuntimeAuth {
            provider,
            source,
            normalized_provider,
            secret,
        }),
        KeyValidationState::Invalid { reason } => Err(DatasetRuntimeError::ProviderAuthInvalid {
            provider: provider.display_name().to_string(),
            key_source: source,
            purpose,
            reason,
        }),
        KeyValidationState::Unknown { reason } => Err(DatasetRuntimeError::ProviderAuthUnknown {
            provider: provider.display_name().to_string(),
            key_source: source,
            purpose,
            reason,
        }),
        KeyValidationState::NotChecked => Err(DatasetRuntimeError::ProviderAuthUnknown {
            provider: provider.display_name().to_string(),
            key_source: source,
            purpose,
            reason: "provider key validation was not checked".to_string(),
        }),
        KeyValidationState::Missing => Err(provider_auth_missing(provider, purpose)),
    }
}

pub async fn run_dataset_synth_prompt_runtime(
    request: DatasetSynthPromptRuntimeRequest,
) -> Result<String, DatasetRuntimeError> {
    let python_runtime = resolve_python_runtime()?;
    let python = require_python_interpreter(&python_runtime, "python dataset synth runtime")?;
    let parts = dataset_synth_prompt_command_parts(&request)?;
    let output = command(&python_runtime, &python, parts)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|source| DatasetRuntimeError::Wait {
            tool: "dataset synth prompt runtime",
            source,
        })?;

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        return Err(DatasetRuntimeError::HelperExit {
            tool: "dataset synth prompt runtime",
            status: output.status,
            stderr,
            debug: None,
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub async fn run_dataset_synth_runtime(
    request: DatasetSynthRuntimeRequest,
) -> Result<DatasetSynthRuntimeOutput, DatasetRuntimeError> {
    let python_runtime = resolve_python_runtime()?;
    let python = require_python_interpreter(&python_runtime, "python dataset synth runtime")?;
    let output_path = request.output.clone();
    let parts = dataset_synth_command_parts(&request)?;
    let mut child = command(&python_runtime, &python, parts)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| DatasetRuntimeError::Spawn {
            tool: "dataset synth runtime",
            source,
        })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| DatasetRuntimeError::InvalidJson {
            tool: "dataset synth runtime",
            message: "failed to capture stdout".to_string(),
        })?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| DatasetRuntimeError::InvalidJson {
            tool: "dataset synth runtime",
            message: "failed to capture stderr".to_string(),
        })?;

    let stdout_task = tokio::spawn(async move {
        let mut stdout_text = String::new();
        let mut reader = BufReader::new(stdout);
        reader.read_to_string(&mut stdout_text).await?;
        Ok::<_, std::io::Error>(stdout_text)
    });
    let stderr_task = tokio::spawn(async move {
        let mut stderr_lines = Vec::new();
        let mut progress_events = VecDeque::new();
        let mut progress_truncated = false;
        let mut lines = BufReader::new(stderr).lines();
        while let Some(line) = lines.next_line().await? {
            if let Ok(event) = serde_json::from_str::<Value>(&line) {
                if event.get("type").and_then(Value::as_str) == Some("progress") {
                    if progress_events.len() == MAX_PROGRESS_EVENTS {
                        progress_events.pop_front();
                        progress_truncated = true;
                    }
                    progress_events.push_back(event);
                    continue;
                }
            }
            if stderr_lines.len() == MAX_STDERR_LINES {
                stderr_lines.remove(0);
            }
            stderr_lines.push(line);
        }
        Ok::<_, std::io::Error>((
            stderr_lines,
            progress_events.into_iter().collect::<Vec<_>>(),
            progress_truncated,
        ))
    });

    let status = child
        .wait()
        .await
        .map_err(|source| DatasetRuntimeError::Wait {
            tool: "dataset synth runtime",
            source,
        })?;
    let stdout = stdout_task
        .await
        .map_err(|err| DatasetRuntimeError::InvalidJson {
            tool: "dataset synth runtime",
            message: format!("stdout reader panicked: {err}"),
        })?
        .map_err(|source| DatasetRuntimeError::StdoutRead {
            tool: "dataset synth runtime",
            source,
        })?;
    let (stderr_lines, progress_events, progress_truncated) = stderr_task
        .await
        .map_err(|err| DatasetRuntimeError::InvalidJson {
            tool: "dataset synth runtime",
            message: format!("stderr reader panicked: {err}"),
        })?
        .map_err(|source| DatasetRuntimeError::StderrRead {
            tool: "dataset synth runtime",
            source,
        })?;

    if !status.success() {
        return Err(DatasetRuntimeError::HelperExit {
            tool: "dataset synth runtime",
            status,
            stderr: stderr_lines.join("\n"),
            debug: debug_from_stderr(&stderr_lines, Some(&output_path)),
        });
    }

    let stdout = stdout.trim();
    let outcome =
        serde_json::from_str::<Value>(stdout).map_err(|err| DatasetRuntimeError::InvalidJson {
            tool: "dataset synth runtime",
            message: format!("{err}"),
        })?;
    Ok(DatasetSynthRuntimeOutput {
        outcome,
        progress_events,
        progress_truncated,
    })
}

pub async fn run_dataset_eval_runtime(
    request: DatasetEvalRuntimeRequest,
) -> Result<Value, DatasetRuntimeError> {
    let python_runtime = resolve_python_runtime()?;
    let python = require_python_interpreter(&python_runtime, "python dataset eval runtime")?;
    let parts = dataset_eval_command_parts(&request)?;
    let output = command(&python_runtime, &python, parts)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|source| DatasetRuntimeError::Wait {
            tool: "dataset eval runtime",
            source,
        })?;

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        return Err(DatasetRuntimeError::HelperExit {
            tool: "dataset eval runtime",
            status: output.status,
            stderr,
            debug: Some(DatasetRuntimeDebug {
                output_path: Some(request.output),
                debug_dir: None,
                prompt_path: None,
                provider_output_path: None,
                error_path: None,
            }),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    serde_json::from_str::<Value>(&stdout).map_err(|err| DatasetRuntimeError::InvalidJson {
        tool: "dataset eval runtime",
        message: format!("{err}"),
    })
}

fn command(runtime: &PythonRuntime, python: &Path, parts: RuntimeCommandParts) -> Command {
    let mut process = Command::new(python);
    process
        .current_dir(runtime.project_dir())
        .env("PYTHONPATH", runtime.python_src_dir())
        .kill_on_drop(true);
    for name in parts.env_remove {
        process.env_remove(name);
    }
    for (name, value) in parts.env {
        process.env(name, value);
    }
    process.args(parts.args);
    process
}

fn dataset_synth_prompt_command_parts(
    request: &DatasetSynthPromptRuntimeRequest,
) -> Result<RuntimeCommandParts, DatasetRuntimeError> {
    let mut args = vec![
        "-m".to_string(),
        "tentgent_daemon.cli.dataset_synth".to_string(),
        "--print-prompt".to_string(),
        "--split".to_string(),
        request.split.clone(),
    ];
    append_dataset_synth_count_args(&mut args, &request.counts);
    append_prompt_source_args(&mut args, request.brief.as_deref(), request.spec.as_deref());
    Ok(RuntimeCommandParts {
        args,
        env: Vec::new(),
        env_remove: vec![DAEMON_TOKEN_ENV_VAR.to_string()],
    })
}

fn dataset_synth_command_parts(
    request: &DatasetSynthRuntimeRequest,
) -> Result<RuntimeCommandParts, DatasetRuntimeError> {
    let mut args = vec![
        "-m".to_string(),
        "tentgent_daemon.cli.dataset_synth".to_string(),
        "--provider".to_string(),
        request.auth.normalized_provider.to_string(),
        "--model".to_string(),
        request.model.clone(),
        "--output".to_string(),
        request.output.display().to_string(),
        "--split".to_string(),
        request.split.clone(),
        "--temperature".to_string(),
        request.temperature.to_string(),
        "--timeout-seconds".to_string(),
        request.timeout_seconds.to_string(),
        "--retries".to_string(),
        request.retries.to_string(),
        "--progress-json".to_string(),
    ];
    append_dataset_synth_count_args(&mut args, &request.counts);
    append_prompt_source_args(&mut args, request.brief.as_deref(), request.spec.as_deref());
    if let Some(max_tokens) = request.max_tokens {
        args.extend(["--max-tokens".to_string(), max_tokens.to_string()]);
    }
    Ok(RuntimeCommandParts {
        args,
        env: vec![(
            request.auth.provider.env_var().to_string(),
            request.auth.secret.clone(),
        )],
        env_remove: vec![DAEMON_TOKEN_ENV_VAR.to_string()],
    })
}

fn dataset_eval_command_parts(
    request: &DatasetEvalRuntimeRequest,
) -> Result<RuntimeCommandParts, DatasetRuntimeError> {
    let mut args = vec![
        "-m".to_string(),
        "tentgent_daemon.cli.dataset_eval".to_string(),
        "--provider".to_string(),
        request.auth.normalized_provider.to_string(),
        "--model".to_string(),
        request.model.clone(),
        "--input".to_string(),
        request.input.display().to_string(),
        "--output".to_string(),
        request.output.display().to_string(),
        "--split".to_string(),
        request.split.clone(),
        "--max-records".to_string(),
        request.max_records.to_string(),
        "--temperature".to_string(),
        request.temperature.to_string(),
        "--timeout-seconds".to_string(),
        request.timeout_seconds.to_string(),
    ];
    if let Some(criteria) = &request.criteria {
        args.extend(["--criteria".to_string(), criteria.clone()]);
    }
    if let Some(max_tokens) = request.max_tokens {
        args.extend(["--max-tokens".to_string(), max_tokens.to_string()]);
    }
    Ok(RuntimeCommandParts {
        args,
        env: vec![(
            request.auth.provider.env_var().to_string(),
            request.auth.secret.clone(),
        )],
        env_remove: vec![DAEMON_TOKEN_ENV_VAR.to_string()],
    })
}

fn append_dataset_synth_count_args(args: &mut Vec<String>, counts: &DatasetSynthCounts) {
    if let Some(count) = counts.count {
        args.extend(["--count".to_string(), count.to_string()]);
    }
    if let Some(count) = counts.train_count {
        args.extend(["--train-count".to_string(), count.to_string()]);
    }
    if let Some(count) = counts.valid_count {
        args.extend(["--valid-count".to_string(), count.to_string()]);
    }
    if let Some(count) = counts.test_count {
        args.extend(["--test-count".to_string(), count.to_string()]);
    }
    if let Some(count) = counts.eval_count {
        args.extend(["--eval-count".to_string(), count.to_string()]);
    }
}

fn append_prompt_source_args(args: &mut Vec<String>, brief: Option<&str>, spec: Option<&Path>) {
    if let Some(brief) = brief {
        args.extend(["--brief".to_string(), brief.to_string()]);
    }
    if let Some(spec) = spec {
        args.extend(["--spec".to_string(), spec.display().to_string()]);
    }
}

fn debug_from_stderr(lines: &[String], output_path: Option<&Path>) -> Option<DatasetRuntimeDebug> {
    let debug_dir = lines.iter().rev().find_map(|line| {
        line.strip_prefix("provider debug written to ")
            .map(|path| PathBuf::from(path.trim()))
    });
    let debug_dir = debug_dir?;
    Some(DatasetRuntimeDebug {
        output_path: output_path.map(Path::to_path_buf),
        prompt_path: Some(debug_dir.join("prompt.md")),
        provider_output_path: Some(debug_dir.join("provider-output.raw.txt")),
        error_path: Some(debug_dir.join("error.txt")),
        debug_dir: Some(debug_dir),
    })
}

fn auth_provider_for_dataset(
    provider_name: &str,
) -> Result<(Provider, &'static str), DatasetRuntimeError> {
    match provider_name.trim() {
        "openai" => Ok((Provider::OpenAI, "openai")),
        "anthropic" | "claude" => Ok((Provider::Anthropic, "anthropic")),
        other => Err(DatasetRuntimeError::UnsupportedProvider(other.to_string())),
    }
}

fn provider_auth_missing(provider: Provider, purpose: &'static str) -> DatasetRuntimeError {
    DatasetRuntimeError::ProviderAuthMissing {
        provider: provider.display_name().to_string(),
        cli_name: provider.cli_name(),
        env_var: provider.env_var(),
        purpose,
    }
}

fn resolve_python_runtime() -> Result<PythonRuntime, DatasetRuntimeError> {
    Ok(PythonRuntime::resolve()?)
}

fn require_python_interpreter(
    runtime: &PythonRuntime,
    label: &'static str,
) -> Result<PathBuf, DatasetRuntimeError> {
    let python = runtime.python_bin();
    if python.exists() {
        return Ok(python);
    }

    Err(DatasetRuntimeError::MissingPythonInterpreter {
        label,
        path: python,
        hint: missing_runtime_hint(runtime),
    })
}

fn missing_runtime_hint(runtime: &PythonRuntime) -> &'static str {
    match runtime.source() {
        PythonRuntimeSource::InstalledPrefix => {
            "run the installer Python bootstrap, then run `tentgent doctor` to verify the managed runtime"
        }
        PythonRuntimeSource::DevelopmentSource | PythonRuntimeSource::EnvironmentOverride => {
            "run `tentgent doctor --fix` during development or `tentgent status` to inspect runtime asset paths"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth(provider: Provider) -> DatasetRuntimeAuth {
        DatasetRuntimeAuth {
            provider,
            source: KeySource::Env,
            normalized_provider: match provider {
                Provider::OpenAI => "openai",
                Provider::Anthropic => "anthropic",
                Provider::HuggingFace => "huggingface",
            },
            secret: "secret".to_string(),
        }
    }

    #[test]
    fn synth_prompt_args_have_no_provider_env_and_remove_daemon_token() {
        let parts = dataset_synth_prompt_command_parts(&DatasetSynthPromptRuntimeRequest {
            brief: Some("make records".to_string()),
            spec: None,
            split: "train".to_string(),
            counts: DatasetSynthCounts {
                count: Some(2),
                ..DatasetSynthCounts::default()
            },
        })
        .expect("parts");

        assert!(parts.env.is_empty());
        assert_eq!(parts.env_remove, vec![DAEMON_TOKEN_ENV_VAR.to_string()]);
        assert!(parts.args.contains(&"--print-prompt".to_string()));
        assert!(parts.args.ends_with(&[
            "--count".to_string(),
            "2".to_string(),
            "--brief".to_string(),
            "make records".to_string()
        ]));
    }

    #[test]
    fn synth_args_preserve_provider_env_and_progress_shape() {
        let parts = dataset_synth_command_parts(&DatasetSynthRuntimeRequest {
            auth: auth(Provider::OpenAI),
            model: "gpt-4.1-mini".to_string(),
            output: PathBuf::from("/tmp/out"),
            brief: None,
            spec: Some(PathBuf::from("/tmp/spec.md")),
            split: "train".to_string(),
            counts: DatasetSynthCounts {
                train_count: Some(2),
                valid_count: Some(1),
                ..DatasetSynthCounts::default()
            },
            max_tokens: Some(1000),
            temperature: 0.2,
            timeout_seconds: 300.0,
            retries: 2,
        })
        .expect("parts");

        assert_eq!(
            parts.env,
            vec![("OPENAI_API_KEY".to_string(), "secret".to_string())]
        );
        assert_eq!(parts.env_remove, vec![DAEMON_TOKEN_ENV_VAR.to_string()]);
        assert!(parts.args.contains(&"--progress-json".to_string()));
        assert!(parts
            .args
            .windows(2)
            .any(|pair| pair == ["--provider", "openai"]));
        assert!(parts
            .args
            .windows(2)
            .any(|pair| pair == ["--spec", "/tmp/spec.md"]));
        assert!(parts
            .args
            .windows(2)
            .any(|pair| pair == ["--train-count", "2"]));
        assert!(parts
            .args
            .windows(2)
            .any(|pair| pair == ["--valid-count", "1"]));
    }

    #[test]
    fn eval_args_preserve_cli_shape_and_remove_daemon_token() {
        let parts = dataset_eval_command_parts(&DatasetEvalRuntimeRequest {
            auth: auth(Provider::Anthropic),
            model: "claude".to_string(),
            input: PathBuf::from("/tmp/input"),
            output: PathBuf::from("/tmp/report"),
            split: "all".to_string(),
            max_records: 3,
            criteria: Some("check style".to_string()),
            max_tokens: Some(500),
            temperature: 0.0,
            timeout_seconds: 180.0,
        })
        .expect("parts");

        assert_eq!(
            parts.env,
            vec![("ANTHROPIC_API_KEY".to_string(), "secret".to_string())]
        );
        assert_eq!(parts.env_remove, vec![DAEMON_TOKEN_ENV_VAR.to_string()]);
        assert!(parts
            .args
            .windows(2)
            .any(|pair| pair == ["--input", "/tmp/input"]));
        assert!(parts
            .args
            .windows(2)
            .any(|pair| pair == ["--criteria", "check style"]));
    }

    #[test]
    fn debug_paths_are_derived_without_raw_provider_output() {
        let debug = debug_from_stderr(
            &["provider debug written to /tmp/out/_debug/train".to_string()],
            Some(Path::new("/tmp/out")),
        )
        .expect("debug");

        assert_eq!(debug.output_path, Some(PathBuf::from("/tmp/out")));
        assert_eq!(
            debug.debug_dir,
            Some(PathBuf::from("/tmp/out/_debug/train"))
        );
        assert_eq!(
            debug.provider_output_path,
            Some(PathBuf::from(
                "/tmp/out/_debug/train/provider-output.raw.txt"
            ))
        );
    }
}
