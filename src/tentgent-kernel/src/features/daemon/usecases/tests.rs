use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    time::Duration,
};

use crate::features::daemon::domain::{
    DaemonBind, DaemonInspection, DaemonProcessMetadata, DaemonStoreLayout, DEFAULT_DAEMON_HOST,
    DEFAULT_DAEMON_PORT,
};
use crate::features::daemon::infra::{
    FileDaemonStateStore, StdDaemonBindSafetyChecker, StdDaemonStoreLayoutInitializer,
};
use crate::features::daemon::ports::{
    DaemonClock, DaemonDetachedCommand, DaemonDetachedLauncher, DaemonHttpReadinessProbe,
    DaemonPidFile, DaemonPortFuture, DaemonProcessController, DaemonProcessProbe, DaemonStateStore,
    DaemonStatusProbeOutcome, DaemonStoreSnapshot,
};
use crate::features::daemon::test_support::{
    assert_http_daemon_url, successful_healthz_probe,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{
    LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput, StdRuntimeLayoutResolver,
};

use super::{
    DaemonClearProcessRequest, DaemonDetachedStartRequest, DaemonDetachedStartResult,
    DaemonDetachedStartUseCase, DaemonInspectionMode, DaemonLifecycleUseCase,
    DaemonPrepareRunRequest, DaemonPrepareRunResult, DaemonReadinessToken,
    DaemonRecordProcessStartRequest, DaemonStatusRequest, DaemonStatusResult, DaemonStatusUseCase,
    DaemonStopRequest, DaemonStopResult, DaemonUseCaseFuture, StdDaemonUseCase,
};

#[tokio::test]
async fn daemon_usecase_ports_cover_status_lifecycle_and_detached_start() {
    let usecase = FakeDaemonUseCase;
    let layout_input = layout_input();

    let status = usecase
        .daemon_status(DaemonStatusRequest {
            layout: layout_input.clone(),
            mode: DaemonInspectionMode::CleanupStale,
        })
        .expect("status");
    assert!(!status.inspection.running);

    let prepared = usecase
        .prepare_run(DaemonPrepareRunRequest {
            layout: layout_input.clone(),
            host: Some("127.0.0.1".to_string()),
            port: Some(8790),
            token_enabled: false,
            allow_unsafe_bind: false,
        })
        .expect("prepare run");
    assert_eq!(prepared.bind.daemon_url(), "http://127.0.0.1:8790");

    let recorded = usecase
        .record_process_start(DaemonRecordProcessStartRequest {
            layout: layout_input.clone(),
            pid: 42,
            bind: prepared.bind.clone(),
        })
        .expect("record process");
    assert!(recorded.inspection.running);

    usecase
        .clear_process_if_matches(DaemonClearProcessRequest {
            layout: layout_input.clone(),
            expected_pid: Some(42),
        })
        .expect("clear process");

    let stopped = usecase
        .stop_daemon(DaemonStopRequest {
            layout: layout_input.clone(),
        })
        .expect("stop daemon");
    assert_eq!(stopped.stopped_pid, 42);

    let token = DaemonReadinessToken::parse(" secret ").expect("token");
    assert_eq!(token.as_str(), "secret");
    assert!(!format!("{token:?}").contains("secret"));
    assert_eq!(DaemonReadinessToken::parse("  "), None);

    let detached = usecase
        .start_daemon_detached(DaemonDetachedStartRequest {
            layout: layout_input,
            host: None,
            port: None,
            token_enabled: true,
            allow_unsafe_bind: false,
            executable: PathBuf::from("/bin/tentgent"),
            status_probe_token: Some(token),
            startup_timeout: Duration::from_secs(5),
        })
        .await
        .expect("detached start");
    assert_eq!(detached.daemon_url, "http://127.0.0.1:8790");
    assert_eq!(detached.launched_pid, Some(4242));
}

#[test]
fn standard_daemon_usecase_records_stops_and_cleans_stale_metadata() {
    let fixture = Fixture::new("foreground");
    let layout_resolver = StdRuntimeLayoutResolver;
    let initializer = StdDaemonStoreLayoutInitializer;
    let state_store = FileDaemonStateStore;
    let process_probe = StaticProcessProbe { running: true };
    let controller = StaticProcessController;
    let bind_safety = StdDaemonBindSafetyChecker;
    let launcher = StaticLauncher::default();
    let readiness = StaticReadinessProbe::default();
    let clock = StaticClock;
    let daemon = StdDaemonUseCase::new(
        &layout_resolver,
        &initializer,
        &state_store,
        &process_probe,
        &controller,
        &bind_safety,
        &launcher,
        &readiness,
        &clock,
    );

    let prepared = daemon
        .prepare_run(DaemonPrepareRunRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            host: None,
            port: Some(8791),
            token_enabled: false,
            allow_unsafe_bind: false,
        })
        .expect("prepare run");
    assert_eq!(prepared.bind.daemon_url(), "http://127.0.0.1:8791");
    assert!(!prepared.inspection.running);

    let recorded = daemon
        .record_process_start(DaemonRecordProcessStartRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            pid: 42,
            bind: prepared.bind,
        })
        .expect("record process");
    assert!(recorded.inspection.running);
    assert_eq!(
        recorded.inspection.process.as_ref().map(|p| p.pid),
        Some(42)
    );

    let stopped = daemon
        .stop_daemon(DaemonStopRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
        })
        .expect("stop daemon");
    assert_eq!(stopped.stopped_pid, 42);
    assert!(!stopped.inspection.running);
    assert!(stopped.inspection.process.is_none());

    let stale_probe = StaticProcessProbe { running: false };
    let stale_daemon = StdDaemonUseCase::new(
        &layout_resolver,
        &initializer,
        &state_store,
        &stale_probe,
        &controller,
        &bind_safety,
        &launcher,
        &readiness,
        &clock,
    );
    state_store
        .record_process_start(
            &DaemonStoreLayout::from_home_runtime_log_dirs(
                fixture.home.clone(),
                fixture.home.join("runtime"),
                fixture.home.join("logs"),
            ),
            &DaemonProcessMetadata {
                pid: 99,
                host: DEFAULT_DAEMON_HOST.to_string(),
                port: DEFAULT_DAEMON_PORT,
                started_at: "2026-05-17T00:00:00Z".to_string(),
            },
        )
        .expect("record stale process");

    let stale = stale_daemon
        .daemon_status(DaemonStatusRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            mode: DaemonInspectionMode::CleanupStale,
        })
        .expect("stale status");
    assert!(!stale.inspection.running);
    assert!(stale.inspection.process.is_none());
    assert!(stale
        .inspection
        .warnings
        .iter()
        .any(|warning| warning.code == "process_metadata_stale"));
    assert!(!stale.store.process_metadata_path().exists());
}

#[tokio::test]
async fn standard_daemon_usecase_detached_start_launches_and_waits_for_readiness() {
    let fixture = Fixture::new("detached");
    let layout_resolver = StdRuntimeLayoutResolver;
    let initializer = StdDaemonStoreLayoutInitializer;
    let state_store = DelayedRunningStateStore::default();
    let process_probe = StaticProcessProbe { running: true };
    let controller = StaticProcessController;
    let bind_safety = StdDaemonBindSafetyChecker;
    let launcher = StaticLauncher::default();
    let readiness = StaticReadinessProbe {
        status_warning: Some("status warning".to_string()),
    };
    let clock = StaticClock;
    let daemon = StdDaemonUseCase::new(
        &layout_resolver,
        &initializer,
        &state_store,
        &process_probe,
        &controller,
        &bind_safety,
        &launcher,
        &readiness,
        &clock,
    );

    let result = daemon
        .start_daemon_detached(DaemonDetachedStartRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            host: Some("127.0.0.1".to_string()),
            port: Some(8792),
            token_enabled: true,
            allow_unsafe_bind: false,
            executable: PathBuf::from("/bin/tentgent"),
            status_probe_token: DaemonReadinessToken::parse("secret"),
            startup_timeout: Duration::from_secs(1),
        })
        .await
        .expect("detached start");

    assert_eq!(result.daemon_url, "http://127.0.0.1:8792");
    assert_eq!(result.launched_pid, Some(4242));
    assert_eq!(result.status_warning.as_deref(), Some("status warning"));
    assert!(!result.already_running);

    let command = launcher.command.lock().expect("launcher command").clone();
    let command = command.expect("detached command");
    assert_eq!(command.executable, PathBuf::from("/bin/tentgent"));
    assert_eq!(
        command.args,
        vec![
            "daemon",
            "run",
            "--home",
            fixture.home.to_str().expect("utf8 path"),
            "--host",
            "127.0.0.1",
            "--port",
            "8792",
        ]
    );
}

#[derive(Debug, Clone, Copy)]
struct FakeDaemonUseCase;

impl DaemonStatusUseCase for FakeDaemonUseCase {
    fn daemon_status(&self, request: DaemonStatusRequest) -> KernelResult<DaemonStatusResult> {
        let layout = runtime_layout(&request.layout);
        let store = daemon_store(&layout);
        Ok(DaemonStatusResult {
            layout,
            store: store.clone(),
            inspection: inspection(store, request.mode == DaemonInspectionMode::Observational),
        })
    }
}

impl DaemonLifecycleUseCase for FakeDaemonUseCase {
    fn prepare_run(
        &self,
        request: DaemonPrepareRunRequest,
    ) -> KernelResult<DaemonPrepareRunResult> {
        let layout = runtime_layout(&request.layout);
        let store = daemon_store(&layout);
        let bind =
            DaemonBind::from_optional(request.host.as_deref(), request.port).expect("fixture bind");
        Ok(DaemonPrepareRunResult {
            layout,
            store: store.clone(),
            bind,
            inspection: inspection(store, false),
            bind_warnings: Vec::new(),
        })
    }

    fn record_process_start(
        &self,
        request: DaemonRecordProcessStartRequest,
    ) -> KernelResult<DaemonStatusResult> {
        let layout = runtime_layout(&request.layout);
        let store = daemon_store(&layout);
        let mut inspection = inspection(store.clone(), true);
        inspection.process = Some(DaemonProcessMetadata {
            pid: request.pid,
            host: request.bind.host,
            port: request.bind.port,
            started_at: "2026-05-17T00:00:00Z".to_string(),
        });
        Ok(DaemonStatusResult {
            layout,
            store,
            inspection,
        })
    }

    fn clear_process_if_matches(&self, _request: DaemonClearProcessRequest) -> KernelResult<()> {
        Ok(())
    }

    fn stop_daemon(&self, request: DaemonStopRequest) -> KernelResult<DaemonStopResult> {
        let layout = runtime_layout(&request.layout);
        let store = daemon_store(&layout);
        Ok(DaemonStopResult {
            layout,
            store: store.clone(),
            inspection: inspection(store, false),
            stopped_pid: 42,
        })
    }
}

impl DaemonDetachedStartUseCase for FakeDaemonUseCase {
    fn start_daemon_detached<'a>(
        &'_ self,
        request: DaemonDetachedStartRequest,
    ) -> DaemonUseCaseFuture<'_, DaemonDetachedStartResult> {
        Box::pin(async move {
            let layout = runtime_layout(&request.layout);
            let store = daemon_store(&layout);
            let bind =
                DaemonBind::from_optional(request.host.as_deref(), request.port).expect("bind");
            Ok(DaemonDetachedStartResult {
                layout,
                store: store.clone(),
                inspection: inspection(store.clone(), true),
                daemon_url: bind.daemon_url(),
                launched_pid: Some(4242),
                stdout_log_path: store.stdout_log_path(),
                stderr_log_path: store.stderr_log_path(),
                status_warning: None,
                bind_warnings: Vec::new(),
                already_running: false,
            })
        })
    }
}

fn layout_input() -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode: LayoutResolveMode::ReadOnly,
        home_dir: Some(PathBuf::from("/tmp/tentgent-home")),
        data_root_dir: None,
    }
}

fn runtime_layout(input: &RuntimeLayoutInput) -> RuntimeLayout {
    let home = input
        .home_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("/tmp/tentgent-home"));
    RuntimeLayout {
        home_dir: home.clone(),
        data_root_dir: home.clone(),
        config_path: home.join("config.toml"),
        models_dir: home.join("models"),
        adapters_dir: home.join("adapters"),
        datasets_dir: home.join("datasets"),
        sessions_dir: home.join("sessions"),
        servers_dir: home.join("servers"),
        train_dir: home.join("train"),
        cache_dir: home.join("cache"),
        runtime_dir: home.join("runtime"),
        logs_dir: home.join("logs"),
        locks_dir: home.join("locks"),
        python_env_dir: home.join("runtime/python-env"),
        bootstrap_dir: home.join("runtime/bootstrap"),
        bootstrap_uv_dir: home.join("runtime/bootstrap/uv"),
        bootstrap_uv_cache_dir: home.join("runtime/bootstrap/uv-cache"),
        capabilities_path: home.join("runtime/capabilities.toml"),
        auth_metadata_path: home.join("runtime/auth.toml"),
    }
}

fn daemon_store(layout: &RuntimeLayout) -> DaemonStoreLayout {
    DaemonStoreLayout::from_home_runtime_log_dirs(
        layout.home_dir.clone(),
        layout.runtime_dir.clone(),
        layout.logs_dir.clone(),
    )
}

fn inspection(store: DaemonStoreLayout, running: bool) -> DaemonInspection {
    DaemonInspection {
        home_dir: store.home_dir.clone(),
        runtime_dir: store.runtime_dir.clone(),
        log_dir: store.log_dir.clone(),
        process_path: store.process_metadata_path(),
        pid_path: store.pid_path(),
        stdout_log_path: store.stdout_log_path(),
        stderr_log_path: store.stderr_log_path(),
        running,
        process: running.then(|| DaemonProcessMetadata {
            pid: 42,
            host: DEFAULT_DAEMON_HOST.to_string(),
            port: DEFAULT_DAEMON_PORT,
            started_at: "2026-05-17T00:00:00Z".to_string(),
        }),
        warnings: Vec::new(),
    }
}

struct Fixture {
    home: PathBuf,
    data: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "tentgent-kernel-daemon-usecase-{label}-{}-{nanos}",
            std::process::id()
        ));
        Self {
            home: root.join("home"),
            data: root.join("data"),
        }
    }

    fn layout_input(&self, mode: LayoutResolveMode) -> RuntimeLayoutInput {
        RuntimeLayoutInput {
            mode,
            home_dir: Some(self.home.clone()),
            data_root_dir: Some(self.data.clone()),
        }
    }
}

struct StaticClock;

impl DaemonClock for StaticClock {
    fn now_rfc3339(&self) -> KernelResult<String> {
        Ok("2026-05-17T00:00:00Z".to_string())
    }
}

struct StaticProcessProbe {
    running: bool,
}

impl DaemonProcessProbe for StaticProcessProbe {
    fn is_process_running(&self, _pid: u32) -> KernelResult<bool> {
        Ok(self.running)
    }
}

struct StaticProcessController;

impl DaemonProcessController for StaticProcessController {
    fn terminate_process(&self, _pid: u32) -> KernelResult<()> {
        Ok(())
    }
}

#[derive(Default)]
struct StaticLauncher {
    command: Mutex<Option<DaemonDetachedCommand>>,
}

impl DaemonDetachedLauncher for StaticLauncher {
    fn launch_detached(&self, command: &DaemonDetachedCommand) -> KernelResult<u32> {
        *self.command.lock().expect("launcher command") = Some(command.clone());
        Ok(4242)
    }
}

#[derive(Default)]
struct StaticReadinessProbe {
    status_warning: Option<String>,
}

impl DaemonHttpReadinessProbe for StaticReadinessProbe {
    fn probe_healthz<'a>(&'a self, daemon_url: &'a str) -> DaemonPortFuture<'a, ()> {
        successful_healthz_probe(daemon_url)
    }

    fn probe_status<'a>(
        &'a self,
        daemon_url: &'a str,
        _token: &'a str,
    ) -> DaemonPortFuture<'a, DaemonStatusProbeOutcome> {
        Box::pin(async move {
            assert_http_daemon_url(daemon_url);
            Ok(DaemonStatusProbeOutcome {
                status_warning: self.status_warning.clone(),
            })
        })
    }
}

#[derive(Default)]
struct DelayedRunningStateStore {
    inspect_count: AtomicUsize,
}

impl DaemonStateStore for DelayedRunningStateStore {
    fn inspect_daemon_store(
        &self,
        _layout: &DaemonStoreLayout,
    ) -> KernelResult<DaemonStoreSnapshot> {
        let count = self.inspect_count.fetch_add(1, Ordering::SeqCst);
        let process = (count >= 2).then(|| DaemonProcessMetadata {
            pid: 4242,
            host: "127.0.0.1".to_string(),
            port: 8792,
            started_at: "2026-05-17T00:00:00Z".to_string(),
        });
        Ok(DaemonStoreSnapshot {
            home_dir_exists: true,
            runtime_dir_exists: true,
            log_dir_exists: true,
            process_path_exists: process.is_some(),
            pid_path_exists: process.is_some(),
            pid_file: process
                .as_ref()
                .map(|process| DaemonPidFile::Valid(process.pid)),
            process,
        })
    }

    fn record_process_start(
        &self,
        _layout: &DaemonStoreLayout,
        _metadata: &DaemonProcessMetadata,
    ) -> KernelResult<()> {
        Ok(())
    }

    fn clear_process_if_matches(
        &self,
        _layout: &DaemonStoreLayout,
        _expected_pid: Option<u32>,
    ) -> KernelResult<()> {
        Ok(())
    }
}
