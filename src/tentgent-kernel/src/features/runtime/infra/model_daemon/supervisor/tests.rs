use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
};

use crate::{
    features::{
        runtime::{
            domain::{PythonRuntimeLayout, PythonRuntimeSource, RuntimeEntrypoint},
            infra::model_daemon::{
                adapters::{
                    RuntimeLaunchAdapters, RuntimeMetadataWriter, RuntimeProcessSpawner,
                    RuntimeSpawnRequest, RuntimeStartupProbe, RuntimeStartupProbeFuture,
                },
                process::{PendingRuntimeProcess, RuntimeProcessTerminator},
            },
            ports::RuntimeExecutableResolver,
        },
        runtime_ownership::{
            OwnershipProcessProbe, RuntimeExecutionIdentity, RuntimeGenerationAdmission,
            RuntimeGenerationLaunchTarget, RuntimeGenerationState, RuntimeLaunchPolicyRecord,
            StdOwnershipProcessProbe, StdRuntimeOwnershipUseCase,
        },
    },
    foundation::{error::KernelResult, fs::atomic_write, layout::RuntimeLayout},
};

use super::*;

struct UnusedSpawner;

impl RuntimeProcessSpawner for UnusedSpawner {
    fn spawn(
        &self,
        _request: RuntimeSpawnRequest<'_>,
    ) -> KernelResult<(PendingRuntimeProcess, ModelRuntimeDaemonEndpoint)> {
        Err(runtime_error("unused test spawner"))
    }
}

struct DummyExecutableResolver;

impl RuntimeExecutableResolver for DummyExecutableResolver {
    fn python_binary_path(&self, _runtime: &PythonRuntimeLayout) -> KernelResult<PathBuf> {
        Ok(PathBuf::from("unused-python"))
    }

    fn entrypoint_path(
        &self,
        _runtime: &PythonRuntimeLayout,
        _entrypoint: RuntimeEntrypoint,
    ) -> KernelResult<PathBuf> {
        Ok(PathBuf::from("unused-entrypoint"))
    }
}

struct FailingMetadata;

impl RuntimeMetadataWriter for FailingMetadata {
    fn write(&self, _path: &Path, _endpoint: &ModelRuntimeDaemonEndpoint) -> KernelResult<()> {
        Err(runtime_error("injected metadata failure"))
    }
}

struct SuccessfulMetadata;

impl RuntimeMetadataWriter for SuccessfulMetadata {
    fn write(&self, path: &Path, _endpoint: &ModelRuntimeDaemonEndpoint) -> KernelResult<()> {
        atomic_write(path, b"test = true\n").map_err(|error| runtime_error(error.to_string()))
    }
}

struct SuccessfulStartup;

impl RuntimeStartupProbe for SuccessfulStartup {
    fn wait_until_healthy<'a>(
        &'a self,
        _client: &'a reqwest::Client,
        _endpoint: &'a ModelRuntimeDaemonEndpoint,
    ) -> RuntimeStartupProbeFuture<'a> {
        Box::pin(async { Ok(()) })
    }
}

struct FailingStartup;

impl RuntimeStartupProbe for FailingStartup {
    fn wait_until_healthy<'a>(
        &'a self,
        _client: &'a reqwest::Client,
        _endpoint: &'a ModelRuntimeDaemonEndpoint,
    ) -> RuntimeStartupProbeFuture<'a> {
        Box::pin(async { Err(runtime_error("injected health timeout")) })
    }
}

struct FailingTerminator;

impl RuntimeProcessTerminator for FailingTerminator {
    fn terminate_and_wait(&self, _child: &mut Child) -> KernelResult<()> {
        Err(runtime_error("injected termination verification failure"))
    }
}

#[tokio::test]
async fn spawn_failure_removes_prepared_generation_without_worker() {
    let home = std::env::temp_dir().join(format!(
        "tentgent-model-daemon-spawn-failure-{}",
        crate::features::resource_coordination::new_operation_id()
    ));
    let layout = test_layout(home.clone());
    let runtime = PythonRuntimeLayout {
        project_dir: home.join("python-project"),
        env_dir: home.join("python-env"),
        source: PythonRuntimeSource::DevelopmentSource,
    };
    let supervisor =
        ModelRuntimeDaemonSupervisor::new_with_launch_adapters(RuntimeLaunchAdapters {
            spawner: Arc::new(UnusedSpawner),
            metadata: Arc::new(SuccessfulMetadata),
            startup: Arc::new(SuccessfulStartup),
        });

    let error = supervisor
        .ensure_unbound(
            &layout,
            &runtime,
            &DummyExecutableResolver,
            ModelRuntimeCapability::LoraTuning,
        )
        .await
        .expect_err("injected spawn failure");

    assert!(error.to_string().contains("unused test spawner"));
    assert!(StdRuntimeOwnershipUseCase::default()
        .summarize_runtime_ownership(&layout)
        .unwrap()
        .generations
        .is_empty());
    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn metadata_failure_terminates_worker_before_removing_generation() {
    assert_post_spawn_failure_cleans_worker(
        Arc::new(FailingMetadata),
        Arc::new(SuccessfulStartup),
        "metadata-failure",
    )
    .await;
}

#[tokio::test]
async fn health_failure_terminates_worker_before_removing_generation() {
    assert_post_spawn_failure_cleans_worker(
        Arc::new(SuccessfulMetadata),
        Arc::new(FailingStartup),
        "health-failure",
    )
    .await;
}

#[test]
fn reused_generation_keeps_first_spawner_idle_policy() {
    let stored = RuntimeLaunchPolicyRecord {
        idle_keep_alive_seconds: "120".to_string(),
        model_idle_timeout_seconds: "45".to_string(),
    };
    let requested = ModelRuntimeDaemonLaunchPolicy {
        idle_keep_alive_seconds: "300".to_string(),
        model_idle_timeout_seconds: "-1".to_string(),
    };

    let mismatch = launch_policy_mismatch(&stored, &requested)
        .expect("different caller policy should be diagnosed");
    assert!(mismatch.contains("first-spawner policy"));
    assert!(mismatch.contains("idle_keep_alive_seconds=120"));
    assert!(mismatch.contains("model_idle_timeout_seconds=45"));
    assert_eq!(
        launch_policy_mismatch(
            &stored,
            &ModelRuntimeDaemonLaunchPolicy {
                idle_keep_alive_seconds: "120".to_string(),
                model_idle_timeout_seconds: "45".to_string(),
            }
        ),
        None
    );
}

#[test]
fn cleanup_failure_retains_closing_generation_with_diagnostic() {
    let (layout, identity, record) = prepared_generation("cleanup-failure");
    let child = spawn_exited_child();
    let endpoint = test_endpoint(child.id(), &record.process_token);
    let ownership = StdRuntimeOwnershipUseCase::default();
    transition_applied(
        ownership
            .mark_runtime_spawned(
                &layout,
                &identity,
                &record.generation_id,
                RuntimeGenerationEndpoint {
                    host: endpoint.host.clone(),
                    port: endpoint.port,
                    pid: endpoint.pid,
                    process_token: endpoint.process_token.clone(),
                },
            )
            .unwrap(),
        "spawn transition",
    )
    .unwrap();
    let mut pending =
        PendingRuntimeProcess::new_with_terminator(child, Arc::new(FailingTerminator));
    let metadata_path = layout.runtime_dir.join("daemon.toml");
    let error = cleanup_failed_launch(
        &mut pending,
        &ownership,
        &layout,
        &identity,
        &record.generation_id,
        &metadata_path,
        runtime_error("injected launch failure"),
    );

    assert!(error
        .to_string()
        .contains("termination could not be verified"));
    let inspection = ownership.summarize_runtime_ownership(&layout).unwrap();
    assert_eq!(inspection.generations.len(), 1);
    assert_eq!(
        inspection.generations[0].state,
        RuntimeGenerationState::Closing
    );
    assert!(inspection.generations[0]
        .diagnostic
        .as_deref()
        .unwrap()
        .contains("injected termination verification failure"));
    let _ = fs::remove_dir_all(layout.home_dir);
}

async fn assert_post_spawn_failure_cleans_worker(
    metadata: Arc<dyn RuntimeMetadataWriter>,
    startup: Arc<dyn RuntimeStartupProbe>,
    label: &str,
) {
    let (layout, identity, record) = prepared_generation(label);
    let child = spawn_sleeping_child();
    let pid = child.id();
    let endpoint = test_endpoint(pid, &record.process_token);
    let mut pending = PendingRuntimeProcess::new(child);
    let supervisor =
        ModelRuntimeDaemonSupervisor::new_with_launch_adapters(RuntimeLaunchAdapters {
            spawner: Arc::new(UnusedSpawner),
            metadata,
            startup,
        });
    let metadata_path = layout.runtime_dir.join("daemon.toml");
    let error = supervisor
        .complete_spawned_launch(
            &layout,
            &identity,
            &record.generation_id,
            &metadata_path,
            &endpoint,
            identity.physical_key().identity,
        )
        .await
        .expect_err("injected launch completion failure");
    let ownership = StdRuntimeOwnershipUseCase::default();
    let cleanup_error = cleanup_failed_launch(
        &mut pending,
        &ownership,
        &layout,
        &identity,
        &record.generation_id,
        &metadata_path,
        error,
    );

    assert!(cleanup_error.to_string().contains("injected"));
    assert!(!StdOwnershipProcessProbe
        .is_process_running(pid)
        .expect("process probe"));
    assert!(ownership
        .summarize_runtime_ownership(&layout)
        .unwrap()
        .generations
        .is_empty());
    assert!(!metadata_path.exists());
    let _ = fs::remove_dir_all(layout.home_dir);
}

fn prepared_generation(
    label: &str,
) -> (
    RuntimeLayout,
    RuntimeExecutionIdentity,
    RuntimeGenerationRecord,
) {
    let home = std::env::temp_dir().join(format!(
        "tentgent-model-daemon-{label}-{}",
        crate::features::resource_coordination::new_operation_id()
    ));
    let layout = test_layout(home);
    let identity = RuntimeExecutionIdentity::unbound(ModelRuntimeCapability::LoraTuning, None);
    let ownership = StdRuntimeOwnershipUseCase::default();
    let record = match ownership
        .admit_runtime_generation(
            &layout,
            identity.clone(),
            RuntimeLaunchPolicyRecord {
                idle_keep_alive_seconds: "300".to_string(),
                model_idle_timeout_seconds: "-1".to_string(),
            },
        )
        .unwrap()
    {
        RuntimeGenerationAdmission::Start(record) => record,
        other => panic!("unexpected generation admission: {other:?}"),
    };
    transition_applied(
        ownership
            .prepare_runtime_launch(
                &layout,
                &identity,
                &record.generation_id,
                RuntimeGenerationLaunchTarget {
                    host: DEFAULT_HOST.to_string(),
                    port: 18780,
                },
            )
            .unwrap(),
        "launch preparation",
    )
    .unwrap();
    (layout, identity, record)
}

fn test_endpoint(pid: u32, process_token: &str) -> ModelRuntimeDaemonEndpoint {
    ModelRuntimeDaemonEndpoint {
        base_url: "http://127.0.0.1:18780".to_string(),
        host: DEFAULT_HOST.to_string(),
        port: 18780,
        pid,
        process_token: process_token.to_string(),
        capability: ModelRuntimeCapability::LoraTuning,
        model_ref: None,
        policy_mismatch: None,
    }
}

#[cfg(unix)]
fn spawn_sleeping_child() -> Child {
    use std::os::unix::process::CommandExt;
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg("sleep 30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    command.spawn().expect("sleeping process")
}

#[cfg(windows)]
fn spawn_sleeping_child() -> Child {
    Command::new("cmd")
        .args(["/C", "ping", "127.0.0.1", "-n", "30"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("sleeping process")
}

#[cfg(unix)]
fn spawn_exited_child() -> Child {
    Command::new("sh")
        .args(["-c", "exit 0"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("exited process")
}

#[cfg(windows)]
fn spawn_exited_child() -> Child {
    Command::new("cmd")
        .args(["/C", "exit", "0"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("exited process")
}

fn test_layout(home: PathBuf) -> RuntimeLayout {
    RuntimeLayout {
        data_root_dir: home.join("data"),
        models_dir: home.join("models"),
        servers_dir: home.join("servers"),
        adapters_dir: home.join("adapters"),
        datasets_dir: home.join("datasets"),
        sessions_dir: home.join("sessions"),
        train_dir: home.join("train"),
        cache_dir: home.join("cache"),
        runtime_dir: home.join("runtime"),
        logs_dir: home.join("logs"),
        locks_dir: home.join("locks"),
        python_env_dir: home.join("runtime/python"),
        bootstrap_dir: home.join("runtime/bootstrap"),
        bootstrap_uv_dir: home.join("runtime/bootstrap/uv"),
        bootstrap_uv_cache_dir: home.join("runtime/bootstrap/uv-cache"),
        capabilities_path: home.join("runtime/capabilities.toml"),
        config_path: home.join("config.toml"),
        auth_metadata_path: home.join("runtime/auth.toml"),
        home_dir: home,
    }
}
