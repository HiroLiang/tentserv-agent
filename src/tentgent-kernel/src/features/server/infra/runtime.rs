use std::fs::OpenOptions;
use std::net::{TcpListener, ToSocketAddrs};
use std::process::{Child, Command, ExitStatus, Stdio};

use crate::features::auth::domain::{AuthSecretMaterial, Provider};
use crate::features::runtime::domain::{PythonRuntimeLayout, RuntimeEntrypoint};
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::features::server::domain::{
    CloudProvider, ServerCapability, ServerInspection, ServerRuntimeKind, ServerSpec,
};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayout;

use super::error::server_runtime_error;

const DAEMON_TOKEN_ENV_VAR: &str = "TENTGENT_DAEMON_TOKEN";

/// Builds and launches the Python `tentgent-server` runtime entrypoint.
pub struct PythonServerRuntimeLauncher<'a> {
    executable_resolver: &'a dyn RuntimeExecutableResolver,
}

impl<'a> PythonServerRuntimeLauncher<'a> {
    pub fn new(executable_resolver: &'a dyn RuntimeExecutableResolver) -> Self {
        Self {
            executable_resolver,
        }
    }

    pub fn spawn_foreground(
        &self,
        request: ServerRuntimeLaunchRequest,
    ) -> KernelResult<SpawnedForegroundServer> {
        let mut command = self.command_for_request(&request)?;
        command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        let child = command.spawn().map_err(|err| {
            server_runtime_error(format!("failed to spawn server runtime: {err}"))
        })?;
        let pid = child.id();

        Ok(SpawnedForegroundServer { pid, child })
    }

    pub fn spawn_background(&self, request: ServerRuntimeLaunchRequest) -> KernelResult<u32> {
        ensure_bind_available(&request.inspection.spec.host, request.inspection.spec.port)?;
        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&request.inspection.stdout_log_path)
            .map_err(|err| {
                server_runtime_error(format!(
                    "open server stdout log `{}` failed: {err}",
                    request.inspection.stdout_log_path.display()
                ))
            })?;
        let stderr = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&request.inspection.stderr_log_path)
            .map_err(|err| {
                server_runtime_error(format!(
                    "open server stderr log `{}` failed: {err}",
                    request.inspection.stderr_log_path.display()
                ))
            })?;

        let mut command = self.command_for_request(&request)?;
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }

        let child = command.spawn().map_err(|err| {
            server_runtime_error(format!("failed to spawn server runtime: {err}"))
        })?;
        Ok(child.id())
    }

    fn command_for_request(&self, request: &ServerRuntimeLaunchRequest) -> KernelResult<Command> {
        let entrypoint = self
            .executable_resolver
            .entrypoint_path(&request.runtime, RuntimeEntrypoint::Server)?;
        let parts = server_runtime_command_parts(
            &request.inspection.spec,
            &request.layout.home_dir,
            request.auth.as_ref(),
        )?;
        let mut command = Command::new(entrypoint);
        command
            .current_dir(&request.runtime.project_dir)
            .env("TENTGENT_HOME", &request.layout.home_dir);

        for name in parts.env_remove {
            command.env_remove(name);
        }
        for (name, value) in parts.env {
            command.env(name, value);
        }
        command.args(parts.args);

        Ok(command)
    }
}

#[derive(Debug, Clone)]
pub struct ServerRuntimeLaunchRequest {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
    pub inspection: ServerInspection,
    pub auth: Option<AuthSecretMaterial>,
}

pub struct SpawnedForegroundServer {
    pub pid: u32,
    child: Child,
}

impl SpawnedForegroundServer {
    pub fn wait(&mut self) -> KernelResult<ExitStatus> {
        self.child.wait().map_err(|err| {
            server_runtime_error(format!("failed to wait for server runtime: {err}"))
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ServerRuntimeCommandParts {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub env_remove: Vec<String>,
}

pub(super) fn server_runtime_command_parts(
    spec: &ServerSpec,
    home_dir: &std::path::Path,
    auth: Option<&AuthSecretMaterial>,
) -> KernelResult<ServerRuntimeCommandParts> {
    if spec.capability != ServerCapability::Chat {
        return Err(server_runtime_error(format!(
            "server capability `{}` is not implemented yet",
            spec.capability
        )));
    }

    let mut args = vec![
        "--server-ref".to_string(),
        spec.server_ref.to_string(),
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

    match spec.runtime_kind {
        ServerRuntimeKind::Local => {
            let model_ref = spec.local_model_ref().ok_or_else(|| {
                server_runtime_error(format!(
                    "local server spec `{}` is missing model_ref",
                    spec.short_ref
                ))
            })?;
            args.extend(["--model-ref".to_string(), model_ref.to_string()]);
        }
        ServerRuntimeKind::Cloud => {
            let provider = spec.provider.ok_or_else(|| {
                server_runtime_error(format!(
                    "cloud server spec `{}` is missing provider metadata",
                    spec.short_ref
                ))
            })?;
            let provider_model = spec.provider_model.as_deref().ok_or_else(|| {
                server_runtime_error(format!(
                    "cloud server spec `{}` is missing provider_model metadata",
                    spec.short_ref
                ))
            })?;
            let auth = auth.ok_or_else(|| {
                server_runtime_error(format!(
                    "cloud server spec `{}` is missing launch-time provider auth",
                    spec.short_ref
                ))
            })?;
            let auth_provider = auth_provider_for_cloud(provider);
            if auth.provider != auth_provider {
                return Err(KernelError::ServerRuntimeUnavailable(format!(
                    "cloud server `{}` expected {} auth, got {} auth",
                    spec.short_ref,
                    auth_provider.display_name(),
                    auth.provider.display_name()
                )));
            }
            env.push((
                auth_provider.env_var().to_string(),
                auth.secret().to_string(),
            ));
            args.extend([
                "--provider".to_string(),
                provider.as_str().to_string(),
                "--provider-model".to_string(),
                provider_model.to_string(),
            ]);
        }
    }

    if spec.lazy_load {
        args.push("--lazy-load".to_string());
    }
    if let Some(idle_seconds) = spec.idle_seconds {
        args.extend(["--idle-seconds".to_string(), idle_seconds.to_string()]);
    }

    Ok(ServerRuntimeCommandParts {
        args,
        env,
        env_remove,
    })
}

fn auth_provider_for_cloud(provider: CloudProvider) -> Provider {
    match provider {
        CloudProvider::OpenAI => Provider::OpenAI,
        CloudProvider::Anthropic => Provider::Anthropic,
    }
}

fn ensure_bind_available(host: &str, port: u16) -> KernelResult<()> {
    let target = socket_addr_text(host, port);
    let mut last_error = None;
    let mut resolved_any = false;
    for addr in target.to_socket_addrs().map_err(|err| {
        server_runtime_error(format!("resolve bind address {target} failed: {err}"))
    })? {
        resolved_any = true;
        match TcpListener::bind(addr) {
            Ok(listener) => {
                drop(listener);
                return Ok(());
            }
            Err(err) => last_error = Some(err),
        }
    }

    let detail = if resolved_any {
        format!(
            "server bind address {target} is not available: {}",
            last_error
                .map(|err| err.to_string())
                .unwrap_or_else(|| "unknown bind error".to_string())
        )
    } else {
        format!("server bind address {target} did not resolve to any socket address")
    };
    Err(server_runtime_error(detail))
}

fn socket_addr_text(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}
