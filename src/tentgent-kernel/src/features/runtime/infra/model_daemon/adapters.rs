use std::{
    fs::{self, OpenOptions},
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    process::{Command, Stdio},
    sync::Arc,
    time::Duration,
};

use crate::{
    features::runtime::{
        domain::{PythonRuntimeLayout, RuntimeEntrypoint},
        ports::RuntimeExecutableResolver,
    },
    foundation::{
        error::{KernelError, KernelResult},
        layout::RuntimeLayout,
        net::http_url_from_host_port,
    },
};

use super::{
    health::endpoint_health_mismatch,
    metadata::{write_metadata, ModelRuntimeDaemonMetadata},
    policy::{ModelRuntimeCapability, ModelRuntimeDaemonLaunchPolicy},
    process::PendingRuntimeProcess,
    supervisor::ModelRuntimeDaemonEndpoint,
};

const DAEMON_DIRNAME: &str = "model-runtime-daemons";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);
const STARTUP_POLL_INTERVAL: Duration = Duration::from_millis(150);

pub(super) struct RuntimeSpawnRequest<'a> {
    pub layout: &'a RuntimeLayout,
    pub runtime: &'a PythonRuntimeLayout,
    pub executable_resolver: &'a dyn RuntimeExecutableResolver,
    pub capability: ModelRuntimeCapability,
    pub model_ref: Option<&'a str>,
    pub policy: &'a ModelRuntimeDaemonLaunchPolicy,
    pub process_token: &'a str,
    pub host: &'a str,
    pub port: u16,
}

pub(super) trait RuntimeProcessSpawner: Send + Sync {
    fn spawn(
        &self,
        request: RuntimeSpawnRequest<'_>,
    ) -> KernelResult<(PendingRuntimeProcess, ModelRuntimeDaemonEndpoint)>;
}

pub(super) trait RuntimeMetadataWriter: Send + Sync {
    fn write(&self, path: &Path, endpoint: &ModelRuntimeDaemonEndpoint) -> KernelResult<()>;
}

pub(super) type RuntimeStartupProbeFuture<'a> =
    Pin<Box<dyn Future<Output = KernelResult<()>> + Send + 'a>>;

pub(super) trait RuntimeStartupProbe: Send + Sync {
    fn wait_until_healthy<'a>(
        &'a self,
        client: &'a reqwest::Client,
        endpoint: &'a ModelRuntimeDaemonEndpoint,
    ) -> RuntimeStartupProbeFuture<'a>;
}

#[derive(Clone)]
pub(super) struct RuntimeLaunchAdapters {
    pub spawner: Arc<dyn RuntimeProcessSpawner>,
    pub metadata: Arc<dyn RuntimeMetadataWriter>,
    pub startup: Arc<dyn RuntimeStartupProbe>,
}

impl RuntimeLaunchAdapters {
    pub(super) fn standard() -> Self {
        Self {
            spawner: Arc::new(StdRuntimeProcessSpawner),
            metadata: Arc::new(StdRuntimeMetadataWriter),
            startup: Arc::new(StdRuntimeStartupProbe),
        }
    }
}

struct StdRuntimeProcessSpawner;

impl RuntimeProcessSpawner for StdRuntimeProcessSpawner {
    fn spawn(
        &self,
        request: RuntimeSpawnRequest<'_>,
    ) -> KernelResult<(PendingRuntimeProcess, ModelRuntimeDaemonEndpoint)> {
        let entrypoint = request
            .executable_resolver
            .entrypoint_path(request.runtime, RuntimeEntrypoint::ModelRuntimeDaemon)?;
        let log_dir = request.layout.logs_dir.join(DAEMON_DIRNAME);
        fs::create_dir_all(&log_dir).map_err(|error| {
            runtime_error(format!(
                "create model runtime log directory `{}` failed: {error}",
                log_dir.display()
            ))
        })?;
        let log_name = daemon_key(request.capability, request.model_ref);
        let stdout = open_log(log_dir.join(format!("{log_name}.stdout.log")), "stdout")?;
        let stderr = open_log(log_dir.join(format!("{log_name}.stderr.log")), "stderr")?;

        let mut command = Command::new(entrypoint);
        command
            .current_dir(&request.runtime.project_dir)
            .env("TENTGENT_HOME", &request.layout.home_dir)
            .env("TENTGENT_DATA_ROOT", &request.layout.data_root_dir)
            .env("TENTGENT_RUNTIME_PROCESS_TOKEN", request.process_token)
            .arg("--host")
            .arg(request.host)
            .arg("--port")
            .arg(request.port.to_string())
            .arg("--home")
            .arg(&request.layout.home_dir)
            .arg("--capability")
            .arg(request.capability.as_str())
            .arg("--idle-keep-alive-seconds")
            .arg(&request.policy.idle_keep_alive_seconds)
            .arg("--model-idle-timeout-seconds")
            .arg(&request.policy.model_idle_timeout_seconds)
            .arg("--lazy-load")
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        if let Some(model_ref) = request.model_ref {
            command.arg("--model-ref").arg(model_ref);
        }

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }

        let child = command.spawn().map_err(|error| {
            runtime_error(format!("failed to spawn model runtime daemon: {error}"))
        })?;
        let pending = PendingRuntimeProcess::new(child);
        let endpoint = ModelRuntimeDaemonEndpoint {
            base_url: http_url_from_host_port(request.host, request.port),
            host: request.host.to_string(),
            port: request.port,
            pid: pending.pid(),
            process_token: request.process_token.to_string(),
            capability: request.capability,
            model_ref: request.model_ref.map(ToOwned::to_owned),
            policy_mismatch: None,
        };
        Ok((pending, endpoint))
    }
}

struct StdRuntimeMetadataWriter;

impl RuntimeMetadataWriter for StdRuntimeMetadataWriter {
    fn write(&self, path: &Path, endpoint: &ModelRuntimeDaemonEndpoint) -> KernelResult<()> {
        write_metadata(path, &ModelRuntimeDaemonMetadata::from_endpoint(endpoint)?)
    }
}

struct StdRuntimeStartupProbe;

impl RuntimeStartupProbe for StdRuntimeStartupProbe {
    fn wait_until_healthy<'a>(
        &'a self,
        client: &'a reqwest::Client,
        endpoint: &'a ModelRuntimeDaemonEndpoint,
    ) -> RuntimeStartupProbeFuture<'a> {
        Box::pin(async move {
            let started = std::time::Instant::now();
            let mut last_error = None;
            while started.elapsed() <= STARTUP_TIMEOUT {
                match endpoint_health_mismatch(client, endpoint).await {
                    Ok(None) => return Ok(()),
                    Ok(Some(reason)) => last_error = Some(reason),
                    Err(error) => last_error = Some(error.to_string()),
                }
                tokio::time::sleep(STARTUP_POLL_INTERVAL).await;
            }
            Err(runtime_error(format!(
                "model runtime daemon did not become healthy on {} within {}s{}",
                endpoint.base_url,
                STARTUP_TIMEOUT.as_secs(),
                last_error
                    .map(|error| format!("; last error: {error}"))
                    .unwrap_or_default()
            )))
        })
    }
}

fn open_log(path: PathBuf, stream: &str) -> KernelResult<std::fs::File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| runtime_error(format!("open model runtime {stream} log failed: {error}")))
}

fn daemon_key(capability: ModelRuntimeCapability, model_ref: Option<&str>) -> String {
    match model_ref {
        Some(model_ref) => format!("{}-{model_ref}", capability.as_str()),
        None => format!("{}-unbound", capability.as_str()),
    }
}

fn runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::RuntimeStateUnavailable(message.into())
}
