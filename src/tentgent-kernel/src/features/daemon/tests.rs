use std::path::PathBuf;

use crate::foundation::error::KernelResult;

use super::domain::{
    daemon_url, host_for_daemon_url, DaemonBind, DaemonHostError, DaemonProcessMetadata,
    DaemonRuntimeStatus, DaemonStoreLayout, DaemonWarning, DAEMON_PID_FILENAME,
    DAEMON_PROCESS_METADATA_FILENAME, DAEMON_STDERR_LOG_FILENAME, DAEMON_STDOUT_LOG_FILENAME,
    DEFAULT_DAEMON_HOST, DEFAULT_DAEMON_PORT,
};
use super::ports::{
    DaemonBindHostClass, DaemonBindSafetyChecker, DaemonBindSafetyReport, DaemonBindSafetyRequest,
    DaemonClock, DaemonDetachedCommand, DaemonDetachedLauncher, DaemonHttpReadinessProbe,
    DaemonPidFile, DaemonPortFuture, DaemonProcessController, DaemonProcessProbe, DaemonStateStore,
    DaemonStatusProbeOutcome, DaemonStoreLayoutInitializer, DaemonStoreSnapshot,
};
use super::test_support::{assert_http_daemon_url, successful_healthz_probe};

const CREATED_AT: &str = "2026-05-17T00:00:00Z";

#[test]
fn daemon_bind_defaults_host_and_port_and_rejects_empty_host() {
    let bind = DaemonBind::from_optional(None, None).expect("default bind");

    assert_eq!(bind.host, DEFAULT_DAEMON_HOST);
    assert_eq!(bind.port, DEFAULT_DAEMON_PORT);
    assert_eq!(bind.daemon_url(), "http://127.0.0.1:8790");
    assert_eq!(
        DaemonBind::from_optional(Some("   "), None),
        Err(DaemonHostError::Empty)
    );
}

#[test]
fn daemon_url_formats_ipv6_hosts() {
    assert_eq!(daemon_url("::1", 8790), "http://[::1]:8790");
    assert_eq!(daemon_url("[::1]", 8790), "http://[::1]:8790");
    assert_eq!(host_for_daemon_url("localhost"), "localhost");
}

#[test]
fn daemon_store_layout_matches_existing_runtime_paths() {
    let layout = DaemonStoreLayout::from_home_runtime_log_dirs(
        "/tmp/tentgent-home",
        "/tmp/tentgent-home/runtime",
        "/tmp/tentgent-home/logs",
    );

    assert_eq!(
        layout.process_metadata_path(),
        PathBuf::from("/tmp/tentgent-home/runtime").join(DAEMON_PROCESS_METADATA_FILENAME)
    );
    assert_eq!(
        layout.pid_path(),
        PathBuf::from("/tmp/tentgent-home/runtime").join(DAEMON_PID_FILENAME)
    );
    assert_eq!(
        layout.stdout_log_path(),
        PathBuf::from("/tmp/tentgent-home/logs").join(DAEMON_STDOUT_LOG_FILENAME)
    );
    assert_eq!(
        layout.stderr_log_path(),
        PathBuf::from("/tmp/tentgent-home/logs").join(DAEMON_STDERR_LOG_FILENAME)
    );
}

#[test]
fn stopped_inspection_uses_layout_paths_and_warning_records() {
    let warning = DaemonWarning::new(
        "runtime_home_missing",
        "runtime home is missing",
        Some(PathBuf::from("/tmp/tentgent-home")),
    );
    let inspection = DaemonStoreLayout::from_home_runtime_log_dirs(
        "/tmp/tentgent-home",
        "/tmp/tentgent-home/runtime",
        "/tmp/tentgent-home/logs",
    )
    .stopped_inspection(vec![warning.clone()]);

    assert_eq!(inspection.status(), DaemonRuntimeStatus::Stopped);
    assert!(!inspection.running);
    assert_eq!(inspection.process, None);
    assert_eq!(inspection.warnings, vec![warning]);
    assert_eq!(inspection.daemon_url(), "http://127.0.0.1:8790");
}

#[test]
fn process_metadata_round_trips_existing_toml_shape() {
    let metadata = DaemonProcessMetadata {
        pid: 42,
        host: "127.0.0.1".to_string(),
        port: 8790,
        started_at: "2026-05-17T00:00:00Z".to_string(),
    };

    assert_eq!(metadata.daemon_url(), "http://127.0.0.1:8790");
    assert_eq!(
        metadata.bind(),
        DaemonBind::from_optional(None, None).expect("bind")
    );

    let body = toml::to_string_pretty(&metadata).expect("serialize daemon metadata");
    assert!(body.contains("pid = 42"));
    assert!(body.contains("host = \"127.0.0.1\""));

    let parsed: DaemonProcessMetadata = toml::from_str(&body).expect("parse daemon metadata");
    assert_eq!(parsed, metadata);
}

#[tokio::test]
async fn daemon_ports_cover_store_process_bind_launch_clock_and_readiness_boundaries() {
    let ports = FakeDaemonPorts;
    let layout = DaemonStoreLayout::from_home_runtime_log_dirs(
        "/tmp/tentgent-home",
        "/tmp/tentgent-home/runtime",
        "/tmp/tentgent-home/logs",
    );

    ports
        .ensure_daemon_store_layout(&layout)
        .expect("ensure daemon layout");

    let snapshot = ports
        .inspect_daemon_store(&layout)
        .expect("inspect daemon store");
    assert!(snapshot.home_dir_exists);
    assert!(snapshot.runtime_dir_exists);
    assert_eq!(snapshot.pid_file, Some(DaemonPidFile::Valid(42)));
    assert_eq!(
        snapshot
            .process
            .as_ref()
            .map(DaemonProcessMetadata::daemon_url),
        Some("http://127.0.0.1:8790".to_string())
    );

    let metadata = DaemonProcessMetadata {
        pid: 42,
        host: DEFAULT_DAEMON_HOST.to_string(),
        port: DEFAULT_DAEMON_PORT,
        started_at: ports.now_rfc3339().expect("clock"),
    };
    ports
        .record_process_start(&layout, &metadata)
        .expect("record process");
    ports
        .clear_process_if_matches(&layout, Some(metadata.pid))
        .expect("clear process");

    assert!(ports
        .is_process_running(metadata.pid)
        .expect("process probe"));
    ports
        .terminate_process(metadata.pid)
        .expect("terminate process");

    let safety = ports
        .check_bind_safety(DaemonBindSafetyRequest {
            bind: metadata.bind(),
            token_enabled: false,
            allow_unsafe_bind: false,
        })
        .expect("bind safety");
    assert_eq!(safety.host_class, DaemonBindHostClass::Loopback);
    assert!(safety.warnings.is_empty());

    let command = DaemonDetachedCommand {
        executable: PathBuf::from("/bin/tentgent"),
        args: vec!["daemon".to_string(), "run".to_string()],
        stdout_log_path: layout.stdout_log_path(),
        stderr_log_path: layout.stderr_log_path(),
    };
    assert_eq!(ports.launch_detached(&command).expect("launch"), 4242);

    ports
        .probe_healthz(&metadata.daemon_url())
        .await
        .expect("healthz");
    assert_eq!(
        ports
            .probe_status(&metadata.daemon_url(), "secret")
            .await
            .expect("status probe")
            .status_warning,
        None
    );
}

#[derive(Debug, Clone, Copy)]
struct FakeDaemonPorts;

impl DaemonStoreLayoutInitializer for FakeDaemonPorts {
    fn ensure_daemon_store_layout(&self, _layout: &DaemonStoreLayout) -> KernelResult<()> {
        Ok(())
    }
}

impl DaemonStateStore for FakeDaemonPorts {
    fn inspect_daemon_store(
        &self,
        _layout: &DaemonStoreLayout,
    ) -> KernelResult<DaemonStoreSnapshot> {
        Ok(DaemonStoreSnapshot {
            home_dir_exists: true,
            runtime_dir_exists: true,
            log_dir_exists: true,
            process_path_exists: true,
            pid_path_exists: true,
            process: Some(DaemonProcessMetadata {
                pid: 42,
                host: DEFAULT_DAEMON_HOST.to_string(),
                port: DEFAULT_DAEMON_PORT,
                started_at: CREATED_AT.to_string(),
            }),
            pid_file: Some(DaemonPidFile::Valid(42)),
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

impl DaemonProcessProbe for FakeDaemonPorts {
    fn is_process_running(&self, pid: u32) -> KernelResult<bool> {
        Ok(pid == 42)
    }
}

impl DaemonProcessController for FakeDaemonPorts {
    fn terminate_process(&self, _pid: u32) -> KernelResult<()> {
        Ok(())
    }
}

impl DaemonClock for FakeDaemonPorts {
    fn now_rfc3339(&self) -> KernelResult<String> {
        Ok(CREATED_AT.to_string())
    }
}

impl DaemonBindSafetyChecker for FakeDaemonPorts {
    fn check_bind_safety(
        &self,
        request: DaemonBindSafetyRequest,
    ) -> KernelResult<DaemonBindSafetyReport> {
        let host_class = if request.bind.host == DEFAULT_DAEMON_HOST {
            DaemonBindHostClass::Loopback
        } else {
            DaemonBindHostClass::NonLoopback
        };
        Ok(DaemonBindSafetyReport {
            host_class,
            warnings: Vec::new(),
        })
    }
}

impl DaemonDetachedLauncher for FakeDaemonPorts {
    fn launch_detached(&self, _command: &DaemonDetachedCommand) -> KernelResult<u32> {
        Ok(4242)
    }
}

impl DaemonHttpReadinessProbe for FakeDaemonPorts {
    fn probe_healthz<'a>(&'a self, daemon_url: &'a str) -> DaemonPortFuture<'a, ()> {
        successful_healthz_probe(daemon_url)
    }

    fn probe_status<'a>(
        &'a self,
        daemon_url: &'a str,
        token: &'a str,
    ) -> DaemonPortFuture<'a, DaemonStatusProbeOutcome> {
        Box::pin(async move {
            assert_http_daemon_url(daemon_url);
            assert_eq!(token, "secret");
            Ok(DaemonStatusProbeOutcome {
                status_warning: None,
            })
        })
    }
}
