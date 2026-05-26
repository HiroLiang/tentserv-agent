use std::fs::OpenOptions;
use std::net::{TcpListener, ToSocketAddrs};
use std::process::{Child, Command, ExitStatus, Stdio};

use crate::features::auth::domain::{AuthSecretMaterial, Provider};
use crate::features::runtime::domain::PythonRuntimeLayout;
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::features::server::domain::{
    CloudProvider, ServerInspection, ServerRuntimeKind, ServerSpec,
};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayout;

use super::error::server_runtime_error;

const DAEMON_TOKEN_ENV_VAR: &str = "TENTGENT_DAEMON_TOKEN";
const AUTO_SERVER_PORT_SCAN_LIMIT: u16 = 100;

/// Builds and launches local model and cloud server runtime entrypoints.
pub struct ServerRuntimeLauncher<'a> {
    _executable_resolver: &'a dyn RuntimeExecutableResolver,
}

impl<'a> ServerRuntimeLauncher<'a> {
    pub fn new(executable_resolver: &'a dyn RuntimeExecutableResolver) -> Self {
        Self {
            _executable_resolver: executable_resolver,
        }
    }

    pub fn spawn_foreground(
        &self,
        request: ServerRuntimeLaunchRequest,
    ) -> KernelResult<SpawnedForegroundServer> {
        let bound_port = allocate_bind_port_for_spec(&request.inspection.spec)?;
        let mut command = self.command_for_request(&request, bound_port)?;
        command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        let child = command.spawn().map_err(|err| {
            server_runtime_error(format!("failed to spawn server runtime: {err}"))
        })?;
        let pid = child.id();

        Ok(SpawnedForegroundServer {
            pid,
            bound_port,
            child,
        })
    }

    pub fn spawn_background(
        &self,
        request: ServerRuntimeLaunchRequest,
    ) -> KernelResult<SpawnedBackgroundServer> {
        let bound_port = allocate_bind_port_for_spec(&request.inspection.spec)?;
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

        let mut command = self.command_for_request(&request, bound_port)?;
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
        Ok(SpawnedBackgroundServer {
            pid: child.id(),
            bound_port,
        })
    }

    fn command_for_request(
        &self,
        request: &ServerRuntimeLaunchRequest,
        bound_port: u16,
    ) -> KernelResult<Command> {
        let entrypoint = match request.inspection.spec.runtime_kind {
            ServerRuntimeKind::Local => std::env::current_exe().map_err(|err| {
                server_runtime_error(format!("failed to resolve current Rust executable: {err}"))
            })?,
            ServerRuntimeKind::Cloud => std::env::current_exe().map_err(|err| {
                server_runtime_error(format!("failed to resolve current Rust executable: {err}"))
            })?,
        };
        let parts = server_runtime_command_parts(
            &request.inspection.spec,
            &request.layout.home_dir,
            request.auth.as_ref(),
            bound_port,
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
    pub bound_port: u16,
    child: Child,
}

impl SpawnedForegroundServer {
    pub fn wait(&mut self) -> KernelResult<ExitStatus> {
        self.child.wait().map_err(|err| {
            server_runtime_error(format!("failed to wait for server runtime: {err}"))
        })
    }
}

pub struct SpawnedBackgroundServer {
    pub pid: u32,
    pub bound_port: u16,
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
    bound_port: u16,
) -> KernelResult<ServerRuntimeCommandParts> {
    let mut env = Vec::new();
    let env_remove = vec![DAEMON_TOKEN_ENV_VAR.to_string()];

    let args = match spec.runtime_kind {
        ServerRuntimeKind::Local => local_model_runtime_command_args(spec, home_dir, bound_port)?,
        ServerRuntimeKind::Cloud => {
            cloud_server_runtime_command_args(spec, home_dir, auth, &mut env, bound_port)?
        }
    };

    Ok(ServerRuntimeCommandParts {
        args,
        env,
        env_remove,
    })
}

fn local_model_runtime_command_args(
    spec: &ServerSpec,
    home_dir: &std::path::Path,
    bound_port: u16,
) -> KernelResult<Vec<String>> {
    let model_ref = spec.local_model_ref().ok_or_else(|| {
        server_runtime_error(format!(
            "local server spec `{}` is missing model_ref",
            spec.short_ref
        ))
    })?;
    let mut args = vec![
        "__local-server-runtime".to_string(),
        "--server-ref".to_string(),
        spec.server_ref.to_string(),
        "--capability".to_string(),
        spec.capability.as_str().to_string(),
        "--host".to_string(),
        spec.host.clone(),
        "--port".to_string(),
        bound_port.to_string(),
        "--home".to_string(),
        home_dir.display().to_string(),
        "--model-ref".to_string(),
        model_ref.to_string(),
    ];
    if spec.lazy_load {
        args.push("--lazy-load".to_string());
    }
    if let Some(idle_seconds) = spec.idle_seconds {
        args.extend(["--idle-seconds".to_string(), idle_seconds.to_string()]);
    }
    Ok(args)
}

fn cloud_server_runtime_command_args(
    spec: &ServerSpec,
    home_dir: &std::path::Path,
    auth: Option<&AuthSecretMaterial>,
    env: &mut Vec<(String, String)>,
    bound_port: u16,
) -> KernelResult<Vec<String>> {
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

    let mut args = vec![
        "__cloud-server-runtime".to_string(),
        "--server-ref".to_string(),
        spec.server_ref.to_string(),
        "--host".to_string(),
        spec.host.clone(),
        "--port".to_string(),
        bound_port.to_string(),
        "--home".to_string(),
        home_dir.display().to_string(),
        "--provider".to_string(),
        provider.as_str().to_string(),
        "--provider-model".to_string(),
        provider_model.to_string(),
    ];
    if spec.lazy_load {
        args.push("--lazy-load".to_string());
    }
    if let Some(idle_seconds) = spec.idle_seconds {
        args.extend(["--idle-seconds".to_string(), idle_seconds.to_string()]);
    }
    Ok(args)
}

fn auth_provider_for_cloud(provider: CloudProvider) -> Provider {
    match provider {
        CloudProvider::OpenAI => Provider::OpenAI,
        CloudProvider::Anthropic => Provider::Anthropic,
        CloudProvider::Gemini => Provider::Gemini,
    }
}

pub(super) fn allocate_bind_port_for_spec(spec: &ServerSpec) -> KernelResult<u16> {
    if !spec.port_auto {
        ensure_bind_available(&spec.host, spec.port)?;
        return Ok(spec.port);
    }

    let start = spec.port;
    let max = u32::from(u16::MAX);
    let end =
        (u32::from(start) + u32::from(AUTO_SERVER_PORT_SCAN_LIMIT).saturating_sub(1)).min(max);
    let mut last_error = None;
    for port in u32::from(start)..=end {
        let port = port as u16;
        match ensure_bind_available(&spec.host, port) {
            Ok(()) => return Ok(port),
            Err(err) => last_error = Some(err.to_string()),
        }
    }

    Err(server_runtime_error(format!(
        "no available server bind port on {} in auto range {}..={}{}",
        spec.host,
        start,
        end,
        last_error
            .map(|err| format!("; last error: {err}"))
            .unwrap_or_default()
    )))
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
