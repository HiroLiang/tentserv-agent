use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use tentgent_core::{
    auth::{AuthError, AuthManager, KeySource, KeyValidationState, Provider},
    runtime_assets::{PythonRuntime, PythonRuntimeSource, RuntimeAssetError},
    server::{
        CloudProvider, LaunchMode, ServerError, ServerInspection, ServerManager,
        ServerPrepareOutcome, ServerSpec,
    },
};
use tokio::process::Command;

use crate::security::DAEMON_TOKEN_ENV_VAR;

#[derive(Clone)]
pub struct CloudRuntimeAuth {
    provider: Provider,
    source: KeySource,
    secret: String,
}

impl CloudRuntimeAuth {
    pub fn provider(&self) -> Provider {
        self.provider
    }

    pub fn source(&self) -> KeySource {
        self.source
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServerRuntimeError {
    #[error("failed to resolve Python runtime assets: {0}")]
    RuntimeAssets(#[from] RuntimeAssetError),
    #[error("{label} is missing at `{path}`; {hint}")]
    MissingPythonInterpreter {
        label: &'static str,
        path: PathBuf,
        hint: &'static str,
    },
    #[error(transparent)]
    Server(#[from] ServerError),
    #[error("failed to access provider auth: {0}")]
    Auth(#[from] AuthError),
    #[error("{provider} key is missing for cloud server `{short_ref}`; run `tentgent auth {cli_name} set` or set `{env_var}` before launch")]
    ProviderAuthMissing {
        provider: String,
        cli_name: &'static str,
        env_var: &'static str,
        short_ref: String,
    },
    #[error(
        "{provider} key from {key_source} is invalid for cloud server `{short_ref}`: {reason}"
    )]
    ProviderAuthInvalid {
        provider: String,
        key_source: KeySource,
        short_ref: String,
        reason: String,
    },
    #[error("{provider} key from {key_source} could not be verified for cloud server `{short_ref}`: {reason}")]
    ProviderAuthUnknown {
        provider: String,
        key_source: KeySource,
        short_ref: String,
        reason: String,
    },
    #[error("failed to spawn server runtime: {0}")]
    Spawn(std::io::Error),
    #[error("failed to wait for server runtime: {0}")]
    Wait(std::io::Error),
    #[error("failed to launch background server runtime: {detail}")]
    BackgroundLaunch { detail: String },
    #[error("failed to parse background server pid: {0}")]
    PidParse(#[from] std::num::ParseIntError),
    #[error("failed to obtain server process pid")]
    MissingPid,
    #[error("server runtime exited with status {status}")]
    ForegroundExit { status: std::process::ExitStatus },
}

impl ServerRuntimeError {
    pub fn is_provider_auth_error(&self) -> bool {
        matches!(
            self,
            Self::ProviderAuthMissing { .. }
                | Self::ProviderAuthInvalid { .. }
                | Self::ProviderAuthUnknown { .. }
        )
    }

    pub fn as_server_error(&self) -> Option<&ServerError> {
        match self {
            Self::Server(error) => Some(error),
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

pub async fn resolve_server_runtime_auth(
    spec: &ServerSpec,
) -> Result<Option<CloudRuntimeAuth>, ServerRuntimeError> {
    if spec.is_cloud() {
        Ok(Some(preflight_cloud_runtime_auth(spec).await?))
    } else {
        ensure_local_runtime_launchable(spec)?;
        Ok(None)
    }
}

pub async fn launch_foreground_server_runtime(
    manager: &ServerManager,
    outcome: &ServerPrepareOutcome,
    cloud_auth: Option<&CloudRuntimeAuth>,
) -> Result<(), ServerRuntimeError> {
    let python_runtime = resolve_python_runtime()?;
    let python_interpreter =
        require_python_interpreter(&python_runtime, "python server interpreter")?;
    let mut process = server_process_command(
        &python_runtime,
        &python_interpreter,
        &outcome.spec,
        &outcome.home_dir,
        cloud_auth,
    )?;
    process
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);

    let mut child = process.spawn().map_err(ServerRuntimeError::Spawn)?;
    let pid = child.id().ok_or(ServerRuntimeError::MissingPid)?;
    manager.record_process_start(&outcome.spec.server_ref, pid, LaunchMode::Foreground)?;

    let status = child.wait().await.map_err(ServerRuntimeError::Wait)?;
    manager.clear_process_if_matches(&outcome.spec.server_ref, Some(pid))?;
    if !status.success() {
        return Err(ServerRuntimeError::ForegroundExit { status });
    }

    Ok(())
}

pub async fn launch_background_server_runtime(
    manager: &ServerManager,
    inspection: &ServerInspection,
    cloud_auth: Option<&CloudRuntimeAuth>,
) -> Result<ServerInspection, ServerRuntimeError> {
    let python_runtime = resolve_python_runtime()?;
    let python_interpreter =
        require_python_interpreter(&python_runtime, "python server interpreter")?;
    let mut process = Command::new("sh");
    process
        .current_dir(python_runtime.project_dir())
        .env("TENTGENT_STDOUT_LOG", &inspection.stdout_log_path)
        .env("TENTGENT_STDERR_LOG", &inspection.stderr_log_path)
        .arg("-c")
        .arg(
            "nohup \"$@\" >>\"$TENTGENT_STDOUT_LOG\" 2>>\"$TENTGENT_STDERR_LOG\" < /dev/null & echo $!",
        )
        .arg("sh")
        .arg(python_interpreter)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false);

    append_server_runtime_args(
        &mut process,
        &inspection.spec,
        &inspection.home_dir,
        cloud_auth,
    )?;

    let output = process.output().await.map_err(ServerRuntimeError::Wait)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            format!("status {}", output.status)
        } else {
            stderr
        };
        return Err(ServerRuntimeError::BackgroundLaunch { detail });
    }

    let pid = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()?;
    let inspection =
        manager.record_process_start(&inspection.spec.server_ref, pid, LaunchMode::Background)?;
    Ok(inspection)
}

fn server_process_command(
    python_runtime: &PythonRuntime,
    python_interpreter: &Path,
    spec: &ServerSpec,
    home_dir: &Path,
    cloud_auth: Option<&CloudRuntimeAuth>,
) -> Result<Command, ServerRuntimeError> {
    let mut process = Command::new(python_interpreter);
    process.current_dir(python_runtime.project_dir());
    append_server_runtime_args(&mut process, spec, home_dir, cloud_auth)?;
    Ok(process)
}

fn append_server_runtime_args(
    process: &mut Command,
    spec: &ServerSpec,
    home_dir: &Path,
    cloud_auth: Option<&CloudRuntimeAuth>,
) -> Result<(), ServerRuntimeError> {
    let parts = server_runtime_command_parts(spec, home_dir, cloud_auth)?;
    for name in parts.env_remove {
        process.env_remove(name);
    }
    for (name, value) in parts.env {
        process.env(name, value);
    }
    process.args(parts.args);
    Ok(())
}

fn server_runtime_command_parts(
    spec: &ServerSpec,
    home_dir: &Path,
    cloud_auth: Option<&CloudRuntimeAuth>,
) -> Result<RuntimeCommandParts, ServerRuntimeError> {
    let mut args = vec![
        "-m".to_string(),
        "tentgent_daemon.cli.server".to_string(),
        "--server-ref".to_string(),
        spec.server_ref.clone(),
        "--runtime-kind".to_string(),
        spec.runtime_kind.as_str().to_string(),
        "--host".to_string(),
        spec.host.clone(),
        "--port".to_string(),
        spec.port.to_string(),
        "--home".to_string(),
        home_dir.display().to_string(),
    ];
    let mut env = Vec::new();
    let env_remove = vec![DAEMON_TOKEN_ENV_VAR.to_string()];

    if spec.is_cloud() {
        let provider = spec.provider.ok_or_else(|| ServerError::ProcessControl {
            message: format!(
                "cloud server spec `{}` is missing provider metadata",
                spec.short_ref
            ),
        })?;
        let provider_model =
            spec.provider_model
                .as_deref()
                .ok_or_else(|| ServerError::ProcessControl {
                    message: format!(
                        "cloud server spec `{}` is missing provider_model metadata",
                        spec.short_ref
                    ),
                })?;
        let cloud_auth = cloud_auth.ok_or_else(|| ServerError::ProcessControl {
            message: format!(
                "cloud server spec `{}` is missing launch-time provider auth",
                spec.short_ref
            ),
        })?;
        env.push((
            cloud_auth.provider.env_var().to_string(),
            cloud_auth.secret.clone(),
        ));
        args.extend([
            "--provider".to_string(),
            provider.as_str().to_string(),
            "--provider-model".to_string(),
            provider_model.to_string(),
        ]);
    } else {
        args.extend([
            "--model-ref".to_string(),
            ensure_local_runtime_launchable(spec)?.to_string(),
        ]);
    }

    if spec.lazy_load {
        args.push("--lazy-load".to_string());
    }

    if let Some(idle_seconds) = spec.idle_seconds {
        args.extend(["--idle-seconds".to_string(), idle_seconds.to_string()]);
    }

    Ok(RuntimeCommandParts {
        args,
        env,
        env_remove,
    })
}

fn ensure_local_runtime_launchable(spec: &ServerSpec) -> Result<&str, ServerRuntimeError> {
    spec.local_model_ref().ok_or_else(|| {
        if spec.is_cloud() {
            ServerError::ProcessControl {
                message: format!(
                    "cloud server spec `{}` cannot be launched through the local model path",
                    spec.short_ref
                ),
            }
        } else {
            ServerError::MissingLocalModelRef(spec.short_ref.clone())
        }
        .into()
    })
}

async fn preflight_cloud_runtime_auth(
    spec: &ServerSpec,
) -> Result<CloudRuntimeAuth, ServerRuntimeError> {
    let cloud_provider = spec.provider.ok_or_else(|| ServerError::ProcessControl {
        message: format!(
            "cloud server spec `{}` is missing provider metadata",
            spec.short_ref
        ),
    })?;
    let provider = auth_provider_for_cloud(cloud_provider);
    let auth = AuthManager::new()?;
    let Some((source, secret)) = auth.effective_secret(provider)? else {
        return Err(provider_auth_missing(provider, &spec.short_ref));
    };

    match auth.validate_secret(provider, &secret).await {
        KeyValidationState::Verified => Ok(CloudRuntimeAuth {
            provider,
            source,
            secret,
        }),
        KeyValidationState::Invalid { reason } => Err(ServerRuntimeError::ProviderAuthInvalid {
            provider: provider.display_name().to_string(),
            key_source: source,
            short_ref: spec.short_ref.clone(),
            reason,
        }),
        KeyValidationState::Unknown { reason } => Err(ServerRuntimeError::ProviderAuthUnknown {
            provider: provider.display_name().to_string(),
            key_source: source,
            short_ref: spec.short_ref.clone(),
            reason,
        }),
        KeyValidationState::Missing => Err(provider_auth_missing(provider, &spec.short_ref)),
    }
}

fn provider_auth_missing(provider: Provider, short_ref: &str) -> ServerRuntimeError {
    ServerRuntimeError::ProviderAuthMissing {
        provider: provider.display_name().to_string(),
        cli_name: provider.cli_name(),
        env_var: provider.env_var(),
        short_ref: short_ref.to_string(),
    }
}

fn auth_provider_for_cloud(provider: CloudProvider) -> Provider {
    match provider {
        CloudProvider::OpenAI => Provider::OpenAI,
        CloudProvider::Anthropic => Provider::Anthropic,
    }
}

fn resolve_python_runtime() -> Result<PythonRuntime, ServerRuntimeError> {
    Ok(PythonRuntime::resolve()?)
}

fn require_python_interpreter(
    runtime: &PythonRuntime,
    label: &'static str,
) -> Result<PathBuf, ServerRuntimeError> {
    let python = runtime.python_bin();
    if python.exists() {
        return Ok(python);
    }

    Err(ServerRuntimeError::MissingPythonInterpreter {
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
    use std::path::PathBuf;

    use tentgent_core::server::{CloudProvider, ServerRuntimeKind};

    use super::*;

    #[test]
    fn local_runtime_args_preserve_python_server_shape() {
        let spec = ServerSpec {
            server_ref: "server-ref".to_string(),
            short_ref: "server-ref".to_string(),
            runtime_kind: ServerRuntimeKind::Local,
            model_ref: Some("model-ref".to_string()),
            provider: None,
            provider_model: None,
            host: "127.0.0.1".to_string(),
            port: 8780,
            lazy_load: true,
            idle_seconds: Some(30),
            created_at: "2026-05-01T00:00:00Z".to_string(),
        };

        let parts = server_runtime_command_parts(&spec, &PathBuf::from("/tmp/tentgent-home"), None)
            .expect("parts");

        assert_eq!(
            parts.args,
            vec![
                "-m",
                "tentgent_daemon.cli.server",
                "--server-ref",
                "server-ref",
                "--runtime-kind",
                "local",
                "--host",
                "127.0.0.1",
                "--port",
                "8780",
                "--home",
                "/tmp/tentgent-home",
                "--model-ref",
                "model-ref",
                "--lazy-load",
                "--idle-seconds",
                "30"
            ]
        );
        assert!(parts.env.is_empty());
        assert_eq!(parts.env_remove, vec![DAEMON_TOKEN_ENV_VAR.to_string()]);
    }

    #[test]
    fn cloud_runtime_args_include_provider_auth_env() {
        let spec = ServerSpec {
            server_ref: "server-ref".to_string(),
            short_ref: "server-ref".to_string(),
            runtime_kind: ServerRuntimeKind::Cloud,
            model_ref: None,
            provider: Some(CloudProvider::OpenAI),
            provider_model: Some("gpt-4.1-mini".to_string()),
            host: "127.0.0.1".to_string(),
            port: 8781,
            lazy_load: false,
            idle_seconds: None,
            created_at: "2026-05-01T00:00:00Z".to_string(),
        };
        let auth = CloudRuntimeAuth {
            provider: Provider::OpenAI,
            source: KeySource::Env,
            secret: "secret".to_string(),
        };

        let parts =
            server_runtime_command_parts(&spec, &PathBuf::from("/tmp/tentgent-home"), Some(&auth))
                .expect("parts");

        assert_eq!(
            parts.env,
            vec![("OPENAI_API_KEY".to_string(), "secret".to_string())]
        );
        assert_eq!(parts.env_remove, vec![DAEMON_TOKEN_ENV_VAR.to_string()]);
        assert!(parts.args.ends_with(&[
            "--provider".to_string(),
            "openai".to_string(),
            "--provider-model".to_string(),
            "gpt-4.1-mini".to_string()
        ]));
    }
}
