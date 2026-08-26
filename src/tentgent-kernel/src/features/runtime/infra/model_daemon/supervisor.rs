use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use serde::{de::DeserializeOwned, Serialize};

use crate::{
    features::{
        model::{
            domain::{ModelRef, ModelStoreLayout},
            infra::FileModelCatalogStore,
            ports::ModelCatalogStore,
        },
        runtime::{domain::PythonRuntimeLayout, ports::RuntimeExecutableResolver},
        runtime_ownership::{
            OwnershipProcessProbe, RuntimeExecutionIdentity, RuntimeGenerationAdmission,
            RuntimeGenerationEndpoint, RuntimeGenerationHealth, RuntimeGenerationHealthProbe,
            RuntimeGenerationLaunchTarget, RuntimeGenerationOwnershipUseCase,
            RuntimeGenerationRecord, RuntimeGenerationTransition, RuntimeLaunchPolicyRecord,
            StdOwnershipProcessProbe, StdRuntimeGenerationHealthProbe, StdRuntimeOwnershipUseCase,
        },
        server::{
            domain::{ServerCapability, ServerRuntimeProfileSelection},
            profile::local_server_runtime_profile_for,
            usecases::server_runtime_backend_for_format,
        },
    },
    foundation::{
        error::{KernelError, KernelResult},
        layout::RuntimeLayout,
        net::http_url_from_host_port,
    },
};

use super::{
    adapters::{RuntimeLaunchAdapters, RuntimeSpawnRequest},
    client::http_error_detail,
    health::{endpoint_health_mismatch, HealthPayload},
    launcher::{allocate_bind_port, socket_addr_text},
    metadata::read_metadata_if_exists,
    policy::{ModelRuntimeCapability, ModelRuntimeDaemonLaunchPolicy},
    process::PendingRuntimeProcess,
};

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 8780;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);
const STARTUP_POLL_INTERVAL: Duration = Duration::from_millis(150);
const HEALTH_POLL_INTERVAL: Duration = Duration::from_secs(30);
const CLOSING_BARRIER: Duration = Duration::from_secs(5);
const DAEMON_DIRNAME: &str = "model-runtime-daemons";
const DAEMON_METADATA_FILENAME: &str = "daemon.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRuntimeDaemonEndpoint {
    pub base_url: String,
    pub host: String,
    pub port: u16,
    pub pid: u32,
    pub process_token: String,
    pub capability: ModelRuntimeCapability,
    pub model_ref: Option<String>,
    pub policy_mismatch: Option<String>,
}

pub struct ModelRuntimeBinding<'a> {
    pub capability: ModelRuntimeCapability,
    pub model_ref: &'a str,
    pub runtime_profile: Option<&'a ServerRuntimeProfileSelection>,
}

struct RuntimeEnsureTarget<'a> {
    capability: ModelRuntimeCapability,
    model_ref: Option<&'a str>,
    runtime_profile: Option<&'a ServerRuntimeProfileSelection>,
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
    endpoints: Mutex<HashMap<String, TrackedRuntimeEndpoint>>,
    poller_started: Mutex<bool>,
    launch: RuntimeLaunchAdapters,
    ownership: Arc<dyn RuntimeGenerationOwnershipUseCase>,
    process_probe: Arc<dyn OwnershipProcessProbe>,
    generation_health_probe: Arc<dyn RuntimeGenerationHealthProbe>,
    model_catalog: Arc<dyn ModelCatalogStore>,
}

#[derive(Clone)]
pub struct ModelRuntimeDaemonSupervisorDependencies {
    pub ownership: Arc<dyn RuntimeGenerationOwnershipUseCase>,
    pub process_probe: Arc<dyn OwnershipProcessProbe>,
    pub generation_health_probe: Arc<dyn RuntimeGenerationHealthProbe>,
    pub model_catalog: Arc<dyn ModelCatalogStore>,
}

impl Default for ModelRuntimeDaemonSupervisorDependencies {
    fn default() -> Self {
        Self {
            ownership: Arc::new(StdRuntimeOwnershipUseCase::default()),
            process_probe: Arc::new(StdOwnershipProcessProbe),
            generation_health_probe: Arc::new(StdRuntimeGenerationHealthProbe),
            model_catalog: Arc::new(FileModelCatalogStore),
        }
    }
}

#[derive(Clone)]
struct TrackedRuntimeEndpoint {
    endpoint: ModelRuntimeDaemonEndpoint,
    layout: RuntimeLayout,
    identity: RuntimeExecutionIdentity,
    generation_id: String,
    metadata_path: PathBuf,
}

impl ModelRuntimeDaemonSupervisor {
    pub fn new() -> Self {
        Self::new_with_dependencies(ModelRuntimeDaemonSupervisorDependencies::default())
    }

    pub fn new_with_dependencies(dependencies: ModelRuntimeDaemonSupervisorDependencies) -> Self {
        Self::new_with_dependencies_and_launch(dependencies, RuntimeLaunchAdapters::standard())
    }

    #[cfg(test)]
    fn new_with_launch_adapters(launch: RuntimeLaunchAdapters) -> Self {
        Self::new_with_dependencies_and_launch(
            ModelRuntimeDaemonSupervisorDependencies::default(),
            launch,
        )
    }

    fn new_with_dependencies_and_launch(
        dependencies: ModelRuntimeDaemonSupervisorDependencies,
        launch: RuntimeLaunchAdapters,
    ) -> Self {
        Self {
            inner: Arc::new(ModelRuntimeDaemonSupervisorInner {
                client: reqwest::Client::new(),
                endpoints: Mutex::new(HashMap::new()),
                poller_started: Mutex::new(false),
                launch,
                ownership: dependencies.ownership,
                process_probe: dependencies.process_probe,
                generation_health_probe: dependencies.generation_health_probe,
                model_catalog: dependencies.model_catalog,
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
            RuntimeEnsureTarget {
                capability,
                model_ref: Some(model_ref),
                runtime_profile: None,
            },
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
            RuntimeEnsureTarget {
                capability,
                model_ref: Some(model_ref),
                runtime_profile: None,
            },
            policy,
        )
        .await
    }

    pub async fn ensure_model_bound_with_profile_and_policy(
        &self,
        layout: &RuntimeLayout,
        runtime: &PythonRuntimeLayout,
        executable_resolver: &dyn RuntimeExecutableResolver,
        binding: ModelRuntimeBinding<'_>,
        policy: &ModelRuntimeDaemonLaunchPolicy,
    ) -> KernelResult<ModelRuntimeDaemonEndpoint> {
        self.ensure(
            layout,
            runtime,
            executable_resolver,
            RuntimeEnsureTarget {
                capability: binding.capability,
                model_ref: Some(binding.model_ref),
                runtime_profile: binding.runtime_profile,
            },
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
            RuntimeEnsureTarget {
                capability,
                model_ref: None,
                runtime_profile: None,
            },
            &ModelRuntimeDaemonLaunchPolicy::default(),
        )
        .await
    }

    async fn ensure(
        &self,
        layout: &RuntimeLayout,
        runtime: &PythonRuntimeLayout,
        executable_resolver: &dyn RuntimeExecutableResolver,
        target: RuntimeEnsureTarget<'_>,
        policy: &ModelRuntimeDaemonLaunchPolicy,
    ) -> KernelResult<ModelRuntimeDaemonEndpoint> {
        let RuntimeEnsureTarget {
            capability,
            model_ref,
            runtime_profile,
        } = target;
        let effective_profile = model_ref.and_then(|model_ref| {
            resolve_effective_profile(
                self.inner.model_catalog.as_ref(),
                layout,
                capability,
                model_ref,
                runtime_profile,
            )
        });
        let identity = match model_ref {
            Some(model_ref) => RuntimeExecutionIdentity::model_bound(
                model_ref,
                capability,
                effective_profile.as_ref(),
            ),
            None => RuntimeExecutionIdentity::unbound(capability, effective_profile.as_ref()),
        };
        let key = identity.physical_key().identity;
        if let Some(endpoint) = self.cached_healthy_endpoint(&key).await? {
            return Ok(endpoint);
        }

        let metadata_path = daemon_metadata_path(layout, &key);
        let ownership = Arc::clone(&self.inner.ownership);
        let launch_policy = RuntimeLaunchPolicyRecord {
            runtime_idle_seconds: policy.runtime_idle_seconds,
            model_idle_seconds: policy.model_idle_seconds,
            legacy_unbounded_model: false,
        };
        if let Some(endpoint) = self
            .stored_healthy_endpoint(&metadata_path, capability, model_ref)
            .await?
            .filter(|endpoint| !endpoint.process_token.starts_with("legacy-pid-"))
        {
            let _ = ownership.adopt_runtime_generation(
                layout,
                identity.clone(),
                launch_policy.clone(),
                RuntimeGenerationEndpoint {
                    host: endpoint.host,
                    port: endpoint.port,
                    pid: endpoint.pid,
                    process_token: endpoint.process_token,
                },
            )?;
        }
        let started = std::time::Instant::now();
        loop {
            match ownership.admit_runtime_generation(
                layout,
                identity.clone(),
                launch_policy.clone(),
            )? {
                RuntimeGenerationAdmission::Start(record) => {
                    let port = match allocate_bind_port(DEFAULT_HOST, DEFAULT_PORT) {
                        Ok(port) => port,
                        Err(error) => {
                            let _ = ownership.remove_runtime_generation(
                                layout,
                                &identity,
                                &record.generation_id,
                            );
                            return Err(error);
                        }
                    };
                    if let Err(error) = transition_applied(
                        ownership.prepare_runtime_launch(
                            layout,
                            &identity,
                            &record.generation_id,
                            RuntimeGenerationLaunchTarget {
                                host: DEFAULT_HOST.to_string(),
                                port,
                            },
                        )?,
                        "runtime generation changed before launch preparation",
                    ) {
                        let _ = ownership.remove_runtime_generation(
                            layout,
                            &identity,
                            &record.generation_id,
                        );
                        return Err(error);
                    }
                    let (mut pending, endpoint) =
                        match self.inner.launch.spawner.spawn(RuntimeSpawnRequest {
                            layout,
                            runtime,
                            executable_resolver,
                            capability,
                            model_ref,
                            policy,
                            process_token: &record.process_token,
                            host: DEFAULT_HOST,
                            port,
                        }) {
                            Ok(launch) => launch,
                            Err(error) => {
                                let _ = ownership.remove_runtime_generation(
                                    layout,
                                    &identity,
                                    &record.generation_id,
                                );
                                return Err(error);
                            }
                        };
                    let completed = self
                        .complete_spawned_launch(
                            layout,
                            &identity,
                            &record.generation_id,
                            &metadata_path,
                            &endpoint,
                            key.clone(),
                        )
                        .await;
                    match completed {
                        Ok(_) => {}
                        Err(error) => {
                            return Err(cleanup_failed_launch(
                                &mut pending,
                                ownership.as_ref(),
                                layout,
                                &identity,
                                &record.generation_id,
                                &metadata_path,
                                error,
                            ));
                        }
                    }
                    pending.disarm();
                    self.ensure_poller();
                    return Ok(endpoint);
                }
                RuntimeGenerationAdmission::Reuse(record) => {
                    if record.policy.legacy_unbounded_model {
                        self.retire_legacy_unbounded_generation(
                            layout,
                            &identity,
                            &record,
                            &metadata_path,
                        )
                        .await?;
                        continue;
                    }
                    if let Some(mut endpoint) = endpoint_from_generation(&record) {
                        if self.health_matches(&endpoint).await? {
                            endpoint.policy_mismatch =
                                launch_policy_mismatch(&record.policy, policy);
                            self.remember_endpoint(
                                key,
                                endpoint.clone(),
                                layout,
                                identity.clone(),
                                record.generation_id,
                                metadata_path.clone(),
                            )?;
                            self.ensure_poller();
                            return Ok(endpoint);
                        }
                    }
                    if !generation_process_running(&record, self.inner.process_probe.as_ref())? {
                        let _ = ownership.remove_runtime_generation(
                            layout,
                            &identity,
                            &record.generation_id,
                        )?;
                        continue;
                    }
                    return Err(runtime_error(format!(
                        "runtime generation {} is running but health identity cannot be verified",
                        record.generation_id
                    )));
                }
                RuntimeGenerationAdmission::Starting(record) => {
                    match self
                        .inner
                        .generation_health_probe
                        .probe_generation_health(&record)?
                    {
                        RuntimeGenerationHealth::Matching { status, endpoint }
                            if status != "closing" =>
                        {
                            let ready = transition_applied(
                                ownership.mark_runtime_ready(
                                    layout,
                                    &identity,
                                    &record.generation_id,
                                    endpoint.clone(),
                                )?,
                                "runtime generation changed during worker adoption",
                            )?;
                            let endpoint = model_endpoint_from_parts(&ready, &endpoint);
                            self.inner
                                .launch
                                .metadata
                                .write(&metadata_path, &endpoint)?;
                            self.remember_endpoint(
                                key,
                                endpoint.clone(),
                                layout,
                                identity.clone(),
                                ready.generation_id,
                                metadata_path.clone(),
                            )?;
                            self.ensure_poller();
                            return Ok(endpoint);
                        }
                        RuntimeGenerationHealth::Matching { status, .. } if status == "closing" => {
                            let _ = ownership.mark_runtime_closing(
                                layout,
                                &identity,
                                &record.generation_id,
                            )?;
                        }
                        RuntimeGenerationHealth::Matching { .. }
                        | RuntimeGenerationHealth::Mismatch { .. }
                        | RuntimeGenerationHealth::Unavailable { .. } => {}
                    }
                    if started.elapsed() > STARTUP_TIMEOUT {
                        if let Some(endpoint) = &record.endpoint {
                            if !self.inner.process_probe.is_process_running(endpoint.pid)? {
                                let _ = ownership.remove_runtime_generation(
                                    layout,
                                    &identity,
                                    &record.generation_id,
                                )?;
                                continue;
                            }
                        }
                        return Err(runtime_error(format!(
                            "runtime generation {} is still starting and cannot be safely replaced; run `tentgent runtime reconcile` for diagnostics",
                            record.generation_id
                        )));
                    }
                    tokio::time::sleep(STARTUP_POLL_INTERVAL).await;
                }
                RuntimeGenerationAdmission::Closing(record) => {
                    if started.elapsed() >= CLOSING_BARRIER {
                        return Err(runtime_error(format!(
                            "runtime generation {} is closing; wait for shutdown to finish and retry",
                            record.generation_id
                        )));
                    }
                    tokio::time::sleep(STARTUP_POLL_INTERVAL).await;
                }
                RuntimeGenerationAdmission::Busy(busy) => {
                    if started.elapsed() > STARTUP_TIMEOUT {
                        return Err(runtime_error(busy.description));
                    }
                    tokio::time::sleep(STARTUP_POLL_INTERVAL).await;
                }
            }
        }
    }

    async fn retire_legacy_unbounded_generation(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        record: &RuntimeGenerationRecord,
        metadata_path: &Path,
    ) -> KernelResult<()> {
        if let Some(endpoint) = endpoint_from_generation(record) {
            if self.health_matches(&endpoint).await? {
                let response = self
                    .inner
                    .client
                    .post(endpoint.url("/v1/lifecycle/shutdown"))
                    .send()
                    .await
                    .map_err(|error| {
                        runtime_error(format!(
                            "retire legacy unbounded runtime generation failed: {error}"
                        ))
                    })?;
                if !response.status().is_success() {
                    return Err(runtime_error(format!(
                        "retire legacy unbounded runtime generation returned HTTP {}",
                        response.status()
                    )));
                }
                transition_applied(
                    self.inner.ownership.mark_runtime_closing(
                        layout,
                        identity,
                        &record.generation_id,
                    )?,
                    "legacy unbounded runtime generation changed during retirement",
                )?;
            } else if generation_process_running(record, self.inner.process_probe.as_ref())? {
                return Err(runtime_error(format!(
                    "legacy unbounded runtime generation {} is running but health identity cannot be verified",
                    record.generation_id
                )));
            }
        }

        let started = std::time::Instant::now();
        while generation_process_running(record, self.inner.process_probe.as_ref())? {
            if started.elapsed() > STARTUP_TIMEOUT {
                return Err(runtime_error(format!(
                    "legacy unbounded runtime generation {} did not stop within {} seconds",
                    record.generation_id,
                    STARTUP_TIMEOUT.as_secs()
                )));
            }
            tokio::time::sleep(STARTUP_POLL_INTERVAL).await;
        }
        transition_applied(
            self.inner.ownership.remove_runtime_generation(
                layout,
                identity,
                &record.generation_id,
            )?,
            "legacy unbounded runtime generation changed before removal",
        )?;
        let _ = fs::remove_file(metadata_path);
        Ok(())
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
            .map(|tracked| tracked.endpoint.clone());
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
        Ok(self.health_mismatch_reason(endpoint).await?.is_none())
    }

    async fn health_mismatch_reason(
        &self,
        endpoint: &ModelRuntimeDaemonEndpoint,
    ) -> KernelResult<Option<String>> {
        endpoint_health_mismatch(&self.inner.client, endpoint).await
    }

    async fn complete_spawned_launch(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
        metadata_path: &Path,
        endpoint: &ModelRuntimeDaemonEndpoint,
        key: String,
    ) -> KernelResult<RuntimeGenerationRecord> {
        transition_applied(
            self.inner.ownership.mark_runtime_spawned(
                layout,
                identity,
                generation_id,
                RuntimeGenerationEndpoint {
                    host: endpoint.host.clone(),
                    port: endpoint.port,
                    pid: endpoint.pid,
                    process_token: endpoint.process_token.clone(),
                },
            )?,
            "runtime generation changed before spawned transition",
        )?;
        self.inner.launch.metadata.write(metadata_path, endpoint)?;
        self.inner
            .launch
            .startup
            .wait_until_healthy(&self.inner.client, endpoint)
            .await?;
        let ready = transition_applied(
            self.inner.ownership.mark_runtime_ready(
                layout,
                identity,
                generation_id,
                RuntimeGenerationEndpoint {
                    host: endpoint.host.clone(),
                    port: endpoint.port,
                    pid: endpoint.pid,
                    process_token: endpoint.process_token.clone(),
                },
            )?,
            "runtime generation changed before ready transition",
        )?;
        self.remember_endpoint(
            key,
            endpoint.clone(),
            layout,
            identity.clone(),
            ready.generation_id.clone(),
            metadata_path.to_path_buf(),
        )?;
        Ok(ready)
    }

    fn remember_endpoint(
        &self,
        key: String,
        endpoint: ModelRuntimeDaemonEndpoint,
        layout: &RuntimeLayout,
        identity: RuntimeExecutionIdentity,
        generation_id: String,
        metadata_path: PathBuf,
    ) -> KernelResult<()> {
        self.inner
            .endpoints
            .lock()
            .map_err(|_| runtime_error("model runtime supervisor lock poisoned"))?
            .insert(
                key,
                TrackedRuntimeEndpoint {
                    endpoint,
                    layout: layout.clone(),
                    identity,
                    generation_id,
                    metadata_path,
                },
            );
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
                Ok(endpoints) => endpoints
                    .iter()
                    .map(|(key, endpoint)| (key.clone(), endpoint.clone()))
                    .collect::<Vec<_>>(),
                Err(_) => return,
            };
            for (key, tracked) in endpoints {
                match blocking_healthz(&tracked.endpoint.host, tracked.endpoint.port) {
                    Ok(payload)
                        if payload.pid == tracked.endpoint.pid
                            && payload.process_token.as_deref()
                                == Some(tracked.endpoint.process_token.as_str()) =>
                    {
                        if payload.status == "closing" {
                            let _ = inner.ownership.mark_runtime_closing(
                                &tracked.layout,
                                &tracked.identity,
                                &tracked.generation_id,
                            );
                        }
                    }
                    _ => {
                        let running = inner
                            .process_probe
                            .is_process_running(tracked.endpoint.pid)
                            .unwrap_or(true);
                        if !running {
                            let _ = inner.ownership.remove_runtime_generation(
                                &tracked.layout,
                                &tracked.identity,
                                &tracked.generation_id,
                            );
                            let _ = fs::remove_file(&tracked.metadata_path);
                        }
                        if let Ok(mut endpoints) = inner.endpoints.lock() {
                            endpoints.remove(&key);
                        }
                    }
                }
            }
        });
    }
}

impl Default for ModelRuntimeDaemonSupervisor {
    fn default() -> Self {
        Self::new()
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

fn blocking_healthz(host: &str, port: u16) -> KernelResult<HealthPayload> {
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
    if !body.starts_with("HTTP/1.1 200") && !body.starts_with("HTTP/1.0 200") {
        return Err(runtime_error(format!(
            "healthz {target} returned non-success response"
        )));
    }
    let payload = body
        .split_once("\r\n\r\n")
        .map(|(_, payload)| payload)
        .ok_or_else(|| runtime_error(format!("healthz {target} returned an invalid response")))?;
    serde_json::from_str(payload)
        .map_err(|error| runtime_error(format!("decode healthz {target} failed: {error}")))
}

fn daemon_metadata_path(layout: &RuntimeLayout, key: &str) -> PathBuf {
    layout
        .runtime_dir
        .join(DAEMON_DIRNAME)
        .join(key)
        .join(DAEMON_METADATA_FILENAME)
}

fn endpoint_from_generation(
    record: &RuntimeGenerationRecord,
) -> Option<ModelRuntimeDaemonEndpoint> {
    let endpoint = record.endpoint.as_ref()?;
    Some(model_endpoint_from_parts(record, endpoint))
}

fn model_endpoint_from_parts(
    record: &RuntimeGenerationRecord,
    endpoint: &RuntimeGenerationEndpoint,
) -> ModelRuntimeDaemonEndpoint {
    ModelRuntimeDaemonEndpoint {
        base_url: http_url_from_host_port(&endpoint.host, endpoint.port),
        host: endpoint.host.clone(),
        port: endpoint.port,
        pid: endpoint.pid,
        process_token: endpoint.process_token.clone(),
        capability: record.identity.capability(),
        model_ref: record.identity.model_ref().map(ToOwned::to_owned),
        policy_mismatch: None,
    }
}

fn transition_applied(
    transition: RuntimeGenerationTransition,
    changed_message: &str,
) -> KernelResult<RuntimeGenerationRecord> {
    match transition {
        RuntimeGenerationTransition::Applied(record) => Ok(record),
        RuntimeGenerationTransition::Busy(busy) => Err(runtime_error(busy.description)),
        RuntimeGenerationTransition::Missing
        | RuntimeGenerationTransition::GenerationChanged(_) => Err(runtime_error(changed_message)),
    }
}

fn launch_policy_mismatch(
    stored: &RuntimeLaunchPolicyRecord,
    requested: &ModelRuntimeDaemonLaunchPolicy,
) -> Option<String> {
    if stored.runtime_idle_seconds == requested.runtime_idle_seconds
        && stored.model_idle_seconds == requested.model_idle_seconds
    {
        return None;
    }
    Some(format!(
        "runtime generation keeps first-spawner policy runtime_idle_seconds={}, model_idle_seconds={}",
        stored.runtime_idle_seconds, stored.model_idle_seconds
    ))
}

fn cleanup_failed_launch(
    pending: &mut PendingRuntimeProcess,
    ownership: &dyn RuntimeGenerationOwnershipUseCase,
    layout: &RuntimeLayout,
    identity: &RuntimeExecutionIdentity,
    generation_id: &str,
    metadata_path: &Path,
    launch_error: KernelError,
) -> KernelError {
    let launch_diagnostic = launch_error.to_string();
    match pending.terminate_and_wait() {
        Ok(()) => match ownership.remove_runtime_generation(layout, identity, generation_id) {
            Ok(RuntimeGenerationTransition::Applied(_)
            | RuntimeGenerationTransition::Missing) => {
                let _ = fs::remove_file(metadata_path);
                launch_error
            }
            Ok(RuntimeGenerationTransition::GenerationChanged(_)) => runtime_error(format!(
                "{launch_diagnostic}; spawned worker terminated, but ownership generation changed during cleanup"
            )),
            Ok(RuntimeGenerationTransition::Busy(busy)) => runtime_error(format!(
                "{launch_diagnostic}; spawned worker terminated, but ownership cleanup is busy: {}",
                busy.description
            )),
            Err(cleanup_error) => runtime_error(format!(
                "{launch_diagnostic}; spawned worker terminated, but ownership cleanup failed: {cleanup_error}"
            )),
        },
        Err(cleanup_error) => {
            let diagnostic = format!(
                "{launch_diagnostic}; worker termination could not be verified: {cleanup_error}"
            );
            let _ = ownership.mark_runtime_closing_with_diagnostic(
                layout,
                identity,
                generation_id,
                &diagnostic,
            );
            runtime_error(diagnostic)
        }
    }
}

fn generation_process_running(
    record: &RuntimeGenerationRecord,
    process_probe: &dyn OwnershipProcessProbe,
) -> KernelResult<bool> {
    let pid = record
        .endpoint
        .as_ref()
        .map(|endpoint| endpoint.pid)
        .unwrap_or(record.launcher.pid);
    process_probe.is_process_running(pid)
}

fn resolve_effective_profile(
    model_catalog: &dyn ModelCatalogStore,
    layout: &RuntimeLayout,
    capability: ModelRuntimeCapability,
    model_ref: &str,
    selected: Option<&ServerRuntimeProfileSelection>,
) -> Option<ServerRuntimeProfileSelection> {
    if let Some(selected) = selected {
        return Some(selected.clone());
    }
    let server_capability = server_capability(capability)?;
    let model_ref = ModelRef::parse(model_ref).ok()?;
    let metadata = model_catalog
        .load_model_metadata(
            &ModelStoreLayout::from_models_dir(layout.models_dir.clone()),
            &model_ref,
        )
        .ok()?;
    let backend =
        server_runtime_backend_for_format(server_capability, metadata.primary_format).ok()?;
    local_server_runtime_profile_for(server_capability, backend).map(|profile| profile.selection)
}

fn server_capability(capability: ModelRuntimeCapability) -> Option<ServerCapability> {
    match capability {
        ModelRuntimeCapability::AudioSpeech => Some(ServerCapability::AudioSpeech),
        ModelRuntimeCapability::AudioTranscription => Some(ServerCapability::AudioTranscription),
        ModelRuntimeCapability::Chat => Some(ServerCapability::Chat),
        ModelRuntimeCapability::Embedding => Some(ServerCapability::Embedding),
        ModelRuntimeCapability::ImageGeneration => Some(ServerCapability::ImageGeneration),
        ModelRuntimeCapability::LoraTuning => None,
        ModelRuntimeCapability::Rerank => Some(ServerCapability::Rerank),
        ModelRuntimeCapability::VideoUnderstanding => Some(ServerCapability::VideoUnderstanding),
        ModelRuntimeCapability::VisionChat => Some(ServerCapability::VisionChat),
    }
}

fn runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::RuntimeStateUnavailable(message.into())
}

#[cfg(test)]
mod tests;
