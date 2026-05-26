use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{ErrorKind, Read, Write},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use reqwest::StatusCode;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    features::{
        model::domain::ModelCapability,
        runtime::{
            domain::{PythonRuntimeLayout, RuntimeEntrypoint},
            ports::RuntimeExecutableResolver,
        },
    },
    foundation::{
        error::{KernelError, KernelResult},
        layout::RuntimeLayout,
        net::http_url_from_host_port,
    },
};

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 8780;
const PORT_SCAN_LIMIT: u16 = 100;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);
const STARTUP_POLL_INTERVAL: Duration = Duration::from_millis(150);
const HEALTH_POLL_INTERVAL: Duration = Duration::from_secs(30);
const LOCK_WAIT_TIMEOUT: Duration = Duration::from_secs(120);
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(100);
const IDLE_KEEP_ALIVE_SECONDS: &str = "300";
const MODEL_IDLE_TIMEOUT_SECONDS: &str = "-1";
const DAEMON_DIRNAME: &str = "model-runtime-daemons";
const DAEMON_METADATA_FILENAME: &str = "daemon.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelRuntimeCapability {
    #[serde(rename = "audio-speech")]
    AudioSpeech,
    #[serde(rename = "audio-transcription")]
    AudioTranscription,
    #[serde(rename = "chat")]
    Chat,
    #[serde(rename = "embedding")]
    Embedding,
    #[serde(rename = "image-generation")]
    ImageGeneration,
    #[serde(rename = "lora-tuning")]
    LoraTuning,
    #[serde(rename = "rerank")]
    Rerank,
    #[serde(rename = "video-understanding")]
    VideoUnderstanding,
    #[serde(rename = "vision-chat")]
    VisionChat,
}

impl ModelRuntimeCapability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AudioSpeech => "audio-speech",
            Self::AudioTranscription => "audio-transcription",
            Self::Chat => "chat",
            Self::Embedding => "embedding",
            Self::ImageGeneration => "image-generation",
            Self::LoraTuning => "lora-tuning",
            Self::Rerank => "rerank",
            Self::VideoUnderstanding => "video-understanding",
            Self::VisionChat => "vision-chat",
        }
    }

    pub const fn from_model_capability(capability: ModelCapability) -> Self {
        match capability {
            ModelCapability::AudioSpeech => Self::AudioSpeech,
            ModelCapability::AudioTranscription => Self::AudioTranscription,
            ModelCapability::Chat => Self::Chat,
            ModelCapability::Embedding => Self::Embedding,
            ModelCapability::ImageGeneration => Self::ImageGeneration,
            ModelCapability::Rerank => Self::Rerank,
            ModelCapability::VideoUnderstanding => Self::VideoUnderstanding,
            ModelCapability::VisionChat => Self::VisionChat,
        }
    }
}

impl std::fmt::Display for ModelRuntimeCapability {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRuntimeDaemonEndpoint {
    pub base_url: String,
    pub host: String,
    pub port: u16,
    pub pid: u32,
    pub capability: ModelRuntimeCapability,
    pub model_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRuntimeDaemonLaunchPolicy {
    pub idle_keep_alive_seconds: String,
    pub model_idle_timeout_seconds: String,
}

impl ModelRuntimeDaemonLaunchPolicy {
    pub fn with_idle_keep_alive_seconds(seconds: u64) -> Self {
        Self {
            idle_keep_alive_seconds: seconds.to_string(),
            model_idle_timeout_seconds: MODEL_IDLE_TIMEOUT_SECONDS.to_string(),
        }
    }
}

impl Default for ModelRuntimeDaemonLaunchPolicy {
    fn default() -> Self {
        Self {
            idle_keep_alive_seconds: IDLE_KEEP_ALIVE_SECONDS.to_string(),
            model_idle_timeout_seconds: MODEL_IDLE_TIMEOUT_SECONDS.to_string(),
        }
    }
}

impl ModelRuntimeDaemonEndpoint {
    pub fn url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }
}

#[derive(Clone)]
pub struct ModelRuntimeDaemonSupervisor {
    inner: Arc<ModelRuntimeDaemonSupervisorInner>,
}

struct ModelRuntimeDaemonSupervisorInner {
    client: reqwest::Client,
    endpoints: Mutex<HashMap<String, ModelRuntimeDaemonEndpoint>>,
    poller_started: Mutex<bool>,
}

impl ModelRuntimeDaemonSupervisor {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ModelRuntimeDaemonSupervisorInner {
                client: reqwest::Client::new(),
                endpoints: Mutex::new(HashMap::new()),
                poller_started: Mutex::new(false),
            }),
        }
    }

    pub async fn ensure_model_bound(
        &self,
        layout: &RuntimeLayout,
        runtime: &PythonRuntimeLayout,
        executable_resolver: &dyn RuntimeExecutableResolver,
        capability: ModelRuntimeCapability,
        model_ref: &str,
    ) -> KernelResult<ModelRuntimeDaemonEndpoint> {
        self.ensure(
            layout,
            runtime,
            executable_resolver,
            capability,
            Some(model_ref),
            &ModelRuntimeDaemonLaunchPolicy::default(),
        )
        .await
    }

    pub async fn ensure_model_bound_with_policy(
        &self,
        layout: &RuntimeLayout,
        runtime: &PythonRuntimeLayout,
        executable_resolver: &dyn RuntimeExecutableResolver,
        capability: ModelRuntimeCapability,
        model_ref: &str,
        policy: &ModelRuntimeDaemonLaunchPolicy,
    ) -> KernelResult<ModelRuntimeDaemonEndpoint> {
        self.ensure(
            layout,
            runtime,
            executable_resolver,
            capability,
            Some(model_ref),
            policy,
        )
        .await
    }

    pub async fn ensure_unbound(
        &self,
        layout: &RuntimeLayout,
        runtime: &PythonRuntimeLayout,
        executable_resolver: &dyn RuntimeExecutableResolver,
        capability: ModelRuntimeCapability,
    ) -> KernelResult<ModelRuntimeDaemonEndpoint> {
        self.ensure(
            layout,
            runtime,
            executable_resolver,
            capability,
            None,
            &ModelRuntimeDaemonLaunchPolicy::default(),
        )
        .await
    }

    async fn ensure(
        &self,
        layout: &RuntimeLayout,
        runtime: &PythonRuntimeLayout,
        executable_resolver: &dyn RuntimeExecutableResolver,
        capability: ModelRuntimeCapability,
        model_ref: Option<&str>,
        policy: &ModelRuntimeDaemonLaunchPolicy,
    ) -> KernelResult<ModelRuntimeDaemonEndpoint> {
        let key = daemon_key(capability, model_ref);
        if let Some(endpoint) = self.cached_healthy_endpoint(&key).await? {
            return Ok(endpoint);
        }

        let metadata_path = daemon_metadata_path(layout, &key);
        if let Some(endpoint) = self
            .stored_healthy_endpoint(&metadata_path, capability, model_ref)
            .await?
        {
            self.remember_endpoint(key, endpoint.clone())?;
            self.ensure_poller();
            return Ok(endpoint);
        }

        let lock_path = daemon_lock_path(layout, &key);
        let mut started = std::time::Instant::now();
        loop {
            match ModelRuntimeDaemonLock::try_acquire(&lock_path) {
                Ok(_lock) => {
                    if let Some(endpoint) = self
                        .stored_healthy_endpoint(&metadata_path, capability, model_ref)
                        .await?
                    {
                        self.remember_endpoint(key, endpoint.clone())?;
                        self.ensure_poller();
                        return Ok(endpoint);
                    }

                    let endpoint = self
                        .spawn_daemon(
                            layout,
                            runtime,
                            executable_resolver,
                            capability,
                            model_ref,
                            &metadata_path,
                            policy,
                        )
                        .await?;
                    self.remember_endpoint(key, endpoint.clone())?;
                    self.ensure_poller();
                    return Ok(endpoint);
                }
                Err(err) if err.kind() == ErrorKind::AlreadyExists => {
                    if let Some(endpoint) = self
                        .stored_healthy_endpoint(&metadata_path, capability, model_ref)
                        .await?
                    {
                        self.remember_endpoint(key, endpoint.clone())?;
                        self.ensure_poller();
                        return Ok(endpoint);
                    }
                    if started.elapsed() > LOCK_WAIT_TIMEOUT {
                        if fs::remove_file(&lock_path).is_ok() {
                            started = std::time::Instant::now();
                        }
                    }
                    tokio::time::sleep(LOCK_POLL_INTERVAL).await;
                }
                Err(err) => {
                    return Err(runtime_error(format!(
                        "acquire model runtime daemon lock `{}` failed: {err}",
                        lock_path.display()
                    )))
                }
            }
        }
    }

    pub async fn post_json<Payload, Output, ErrorFn>(
        &self,
        endpoint: &ModelRuntimeDaemonEndpoint,
        path: &str,
        payload: &Payload,
        error: ErrorFn,
    ) -> KernelResult<Output>
    where
        Payload: Serialize + ?Sized,
        Output: DeserializeOwned,
        ErrorFn: Fn(String) -> KernelError,
    {
        let response = self
            .inner
            .client
            .post(endpoint.url(path))
            .json(payload)
            .send()
            .await
            .map_err(|err| error(format!("model runtime HTTP request failed: {err}")))?;
        let response = ensure_success(response, &error).await?;
        response
            .json::<Output>()
            .await
            .map_err(|err| error(format!("failed to decode model runtime response: {err}")))
    }

    pub async fn post_response<Payload, ErrorFn>(
        &self,
        endpoint: &ModelRuntimeDaemonEndpoint,
        path: &str,
        payload: &Payload,
        error: ErrorFn,
    ) -> KernelResult<reqwest::Response>
    where
        Payload: Serialize + ?Sized,
        ErrorFn: Fn(String) -> KernelError,
    {
        let response = self
            .inner
            .client
            .post(endpoint.url(path))
            .json(payload)
            .send()
            .await
            .map_err(|err| error(format!("model runtime HTTP request failed: {err}")))?;
        ensure_success(response, &error).await
    }

    async fn cached_healthy_endpoint(
        &self,
        key: &str,
    ) -> KernelResult<Option<ModelRuntimeDaemonEndpoint>> {
        let endpoint = self
            .inner
            .endpoints
            .lock()
            .map_err(|_| runtime_error("model runtime supervisor lock poisoned"))?
            .get(key)
            .cloned();
        let Some(endpoint) = endpoint else {
            return Ok(None);
        };
        if self.health_matches(&endpoint).await? {
            Ok(Some(endpoint))
        } else {
            Ok(None)
        }
    }

    async fn stored_healthy_endpoint(
        &self,
        metadata_path: &Path,
        capability: ModelRuntimeCapability,
        model_ref: Option<&str>,
    ) -> KernelResult<Option<ModelRuntimeDaemonEndpoint>> {
        let Some(metadata) = read_metadata_if_exists(metadata_path)? else {
            return Ok(None);
        };
        if metadata.capability != capability || metadata.model_ref.as_deref() != model_ref {
            return Ok(None);
        }
        let endpoint = metadata.endpoint();
        if self.health_matches(&endpoint).await? {
            Ok(Some(endpoint))
        } else {
            let _ = fs::remove_file(metadata_path);
            Ok(None)
        }
    }

    async fn health_matches(&self, endpoint: &ModelRuntimeDaemonEndpoint) -> KernelResult<bool> {
        let response = match self.inner.client.get(endpoint.url("/healthz")).send().await {
            Ok(response) => response,
            Err(_) => return Ok(false),
        };
        if response.status() != StatusCode::OK {
            return Ok(false);
        }
        let payload = match response.json::<HealthPayload>().await {
            Ok(payload) => payload,
            Err(_) => return Ok(false),
        };
        Ok(payload.status == "ok"
            && payload.pid == endpoint.pid
            && payload.runtime.capability == endpoint.capability.as_str()
            && payload.runtime.model_ref.as_deref() == endpoint.model_ref.as_deref())
    }

    async fn spawn_daemon(
        &self,
        layout: &RuntimeLayout,
        runtime: &PythonRuntimeLayout,
        executable_resolver: &dyn RuntimeExecutableResolver,
        capability: ModelRuntimeCapability,
        model_ref: Option<&str>,
        metadata_path: &Path,
        policy: &ModelRuntimeDaemonLaunchPolicy,
    ) -> KernelResult<ModelRuntimeDaemonEndpoint> {
        let port = allocate_bind_port(DEFAULT_HOST, DEFAULT_PORT)?;
        let entrypoint =
            executable_resolver.entrypoint_path(runtime, RuntimeEntrypoint::ModelRuntimeDaemon)?;
        let log_dir = layout.logs_dir.join(DAEMON_DIRNAME);
        fs::create_dir_all(&log_dir).map_err(|err| {
            runtime_error(format!(
                "create model runtime log directory `{}` failed: {err}",
                log_dir.display()
            ))
        })?;
        let log_name = daemon_key(capability, model_ref);
        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join(format!("{log_name}.stdout.log")))
            .map_err(|err| runtime_error(format!("open model runtime stdout log failed: {err}")))?;
        let stderr = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join(format!("{log_name}.stderr.log")))
            .map_err(|err| runtime_error(format!("open model runtime stderr log failed: {err}")))?;

        let mut command = Command::new(entrypoint);
        command
            .current_dir(&runtime.project_dir)
            .env("TENTGENT_HOME", &layout.home_dir)
            .arg("--host")
            .arg(DEFAULT_HOST)
            .arg("--port")
            .arg(port.to_string())
            .arg("--home")
            .arg(&layout.home_dir)
            .arg("--capability")
            .arg(capability.as_str())
            .arg("--idle-keep-alive-seconds")
            .arg(&policy.idle_keep_alive_seconds)
            .arg("--model-idle-timeout-seconds")
            .arg(&policy.model_idle_timeout_seconds)
            .arg("--lazy-load")
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        if let Some(model_ref) = model_ref {
            command.arg("--model-ref").arg(model_ref);
        }

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }

        let child = command
            .spawn()
            .map_err(|err| runtime_error(format!("failed to spawn model runtime daemon: {err}")))?;
        let endpoint = ModelRuntimeDaemonEndpoint {
            base_url: http_url_from_host_port(DEFAULT_HOST, port),
            host: DEFAULT_HOST.to_string(),
            port,
            pid: child.id(),
            capability,
            model_ref: model_ref.map(ToOwned::to_owned),
        };
        write_metadata(
            metadata_path,
            &ModelRuntimeDaemonMetadata::from_endpoint(&endpoint)?,
        )?;
        self.wait_until_healthy(&endpoint).await?;
        Ok(endpoint)
    }

    async fn wait_until_healthy(&self, endpoint: &ModelRuntimeDaemonEndpoint) -> KernelResult<()> {
        let started = std::time::Instant::now();
        let mut last_error = None;
        while started.elapsed() <= STARTUP_TIMEOUT {
            match self.health_matches(endpoint).await {
                Ok(true) => return Ok(()),
                Ok(false) => {
                    last_error = Some("healthz did not match expected runtime".to_string())
                }
                Err(err) => last_error = Some(err.to_string()),
            }
            tokio::time::sleep(STARTUP_POLL_INTERVAL).await;
        }
        Err(runtime_error(format!(
            "model runtime daemon did not become healthy on {} within {}s{}",
            endpoint.base_url,
            STARTUP_TIMEOUT.as_secs(),
            last_error
                .map(|err| format!("; last error: {err}"))
                .unwrap_or_default()
        )))
    }

    fn remember_endpoint(
        &self,
        key: String,
        endpoint: ModelRuntimeDaemonEndpoint,
    ) -> KernelResult<()> {
        self.inner
            .endpoints
            .lock()
            .map_err(|_| runtime_error("model runtime supervisor lock poisoned"))?
            .insert(key, endpoint);
        Ok(())
    }

    fn ensure_poller(&self) {
        let Ok(mut started) = self.inner.poller_started.lock() else {
            return;
        };
        if *started {
            return;
        }
        *started = true;
        let inner = Arc::clone(&self.inner);
        thread::spawn(move || loop {
            thread::sleep(HEALTH_POLL_INTERVAL);
            let endpoints = match inner.endpoints.lock() {
                Ok(endpoints) => endpoints.values().cloned().collect::<Vec<_>>(),
                Err(_) => return,
            };
            for endpoint in endpoints {
                let _ = blocking_healthz(&endpoint.host, endpoint.port);
            }
        });
    }
}

struct ModelRuntimeDaemonLock {
    path: PathBuf,
}

impl ModelRuntimeDaemonLock {
    fn try_acquire(path: &Path) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        writeln!(file, "pid = {}", std::process::id())?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for ModelRuntimeDaemonLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

impl Default for ModelRuntimeDaemonSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn http_error_detail(response: reqwest::Response) -> String {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if body.trim().is_empty() {
        return format!("model runtime HTTP request failed with status {status}");
    }
    match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(value) => match value.get("detail") {
            Some(serde_json::Value::String(detail)) => {
                format!("model runtime HTTP {status}: {detail}")
            }
            Some(detail) => format!("model runtime HTTP {status}: {detail}"),
            None => format!("model runtime HTTP {status}: {value}"),
        },
        Err(_) => format!("model runtime HTTP {status}: {body}"),
    }
}

async fn ensure_success<ErrorFn>(
    response: reqwest::Response,
    error: &ErrorFn,
) -> KernelResult<reqwest::Response>
where
    ErrorFn: Fn(String) -> KernelError,
{
    if response.status().is_success() {
        return Ok(response);
    }
    Err(error(http_error_detail(response).await))
}

fn allocate_bind_port(host: &str, start: u16) -> KernelResult<u16> {
    let max = u32::from(u16::MAX);
    let end = (u32::from(start) + u32::from(PORT_SCAN_LIMIT).saturating_sub(1)).min(max);
    let mut last_error = None;
    for port in u32::from(start)..=end {
        let port = port as u16;
        match ensure_bind_available(host, port) {
            Ok(()) => return Ok(port),
            Err(err) => last_error = Some(err.to_string()),
        }
    }
    Err(runtime_error(format!(
        "no available model runtime bind port on {host} in auto range {start}..={end}{}",
        last_error
            .map(|err| format!("; last error: {err}"))
            .unwrap_or_default()
    )))
}

fn ensure_bind_available(host: &str, port: u16) -> KernelResult<()> {
    let target = socket_addr_text(host, port);
    let mut last_error = None;
    let mut resolved_any = false;
    for addr in target
        .to_socket_addrs()
        .map_err(|err| runtime_error(format!("resolve bind address {target} failed: {err}")))?
    {
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
            "model runtime bind address {target} is not available: {}",
            last_error
                .map(|err| err.to_string())
                .unwrap_or_else(|| "unknown bind error".to_string())
        )
    } else {
        format!("model runtime bind address {target} did not resolve to any socket address")
    };
    Err(runtime_error(detail))
}

fn socket_addr_text(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn blocking_healthz(host: &str, port: u16) -> KernelResult<()> {
    let target = socket_addr_text(host, port);
    let mut addrs = target
        .to_socket_addrs()
        .map_err(|err| runtime_error(format!("resolve healthz address {target} failed: {err}")))?;
    let Some(addr) = addrs.next() else {
        return Err(runtime_error(format!(
            "healthz address {target} did not resolve to any socket address"
        )));
    };
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))
        .map_err(|err| runtime_error(format!("connect healthz {target} failed: {err}")))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|err| runtime_error(format!("set healthz read timeout failed: {err}")))?;
    stream
        .write_all(
            format!("GET /healthz HTTP/1.1\r\nHost: {target}\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .map_err(|err| runtime_error(format!("write healthz request failed: {err}")))?;
    let mut body = String::new();
    stream
        .read_to_string(&mut body)
        .map_err(|err| runtime_error(format!("read healthz response failed: {err}")))?;
    if body.starts_with("HTTP/1.1 200") || body.starts_with("HTTP/1.0 200") {
        Ok(())
    } else {
        Err(runtime_error(format!(
            "healthz {target} returned non-success response"
        )))
    }
}

fn daemon_key(capability: ModelRuntimeCapability, model_ref: Option<&str>) -> String {
    match model_ref {
        Some(model_ref) => format!("{}-{model_ref}", capability.as_str()),
        None => format!("{}-unbound", capability.as_str()),
    }
}

fn daemon_metadata_path(layout: &RuntimeLayout, key: &str) -> PathBuf {
    layout
        .runtime_dir
        .join(DAEMON_DIRNAME)
        .join(key)
        .join(DAEMON_METADATA_FILENAME)
}

fn daemon_lock_path(layout: &RuntimeLayout, key: &str) -> PathBuf {
    layout
        .locks_dir
        .join(DAEMON_DIRNAME)
        .join(format!("{key}.lock"))
}

fn read_metadata_if_exists(path: &Path) -> KernelResult<Option<ModelRuntimeDaemonMetadata>> {
    if !path.exists() {
        return Ok(None);
    }
    let body = fs::read_to_string(path).map_err(|err| {
        runtime_error(format!(
            "read model runtime daemon metadata `{}` failed: {err}",
            path.display()
        ))
    })?;
    toml::from_str(&body).map(Some).map_err(|err| {
        runtime_error(format!(
            "parse model runtime daemon metadata `{}` failed: {err}",
            path.display()
        ))
    })
}

fn write_metadata(path: &Path, metadata: &ModelRuntimeDaemonMetadata) -> KernelResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            runtime_error(format!(
                "create model runtime daemon metadata directory `{}` failed: {err}",
                parent.display()
            ))
        })?;
    }
    let body = toml::to_string_pretty(metadata).map_err(|err| {
        runtime_error(format!(
            "serialize model runtime daemon metadata failed: {err}"
        ))
    })?;
    fs::write(path, body).map_err(|err| {
        runtime_error(format!(
            "write model runtime daemon metadata `{}` failed: {err}",
            path.display()
        ))
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModelRuntimeDaemonMetadata {
    host: String,
    port: u16,
    pid: u32,
    capability: ModelRuntimeCapability,
    model_ref: Option<String>,
    started_at: String,
}

impl ModelRuntimeDaemonMetadata {
    fn from_endpoint(endpoint: &ModelRuntimeDaemonEndpoint) -> KernelResult<Self> {
        let started_at = OffsetDateTime::now_utc().format(&Rfc3339).map_err(|err| {
            runtime_error(format!("format model runtime timestamp failed: {err}"))
        })?;
        Ok(Self {
            host: endpoint.host.clone(),
            port: endpoint.port,
            pid: endpoint.pid,
            capability: endpoint.capability,
            model_ref: endpoint.model_ref.clone(),
            started_at,
        })
    }

    fn endpoint(&self) -> ModelRuntimeDaemonEndpoint {
        ModelRuntimeDaemonEndpoint {
            base_url: http_url_from_host_port(&self.host, self.port),
            host: self.host.clone(),
            port: self.port,
            pid: self.pid,
            capability: self.capability,
            model_ref: self.model_ref.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct HealthPayload {
    status: String,
    pid: u32,
    runtime: HealthRuntimePayload,
}

#[derive(Debug, Deserialize)]
struct HealthRuntimePayload {
    capability: String,
    model_ref: Option<String>,
}

fn runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::RuntimeStateUnavailable(message.into())
}
