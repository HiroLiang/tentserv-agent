use std::{fs, path::PathBuf};

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::features::daemon::domain::{
    daemon_status_probe_warning, DaemonBind, DaemonProcessMetadata, DaemonStoreLayout,
    DEFAULT_DAEMON_HOST, DEFAULT_DAEMON_PORT,
};
use crate::features::daemon::ports::{
    DaemonBindHostClass, DaemonBindSafetyChecker, DaemonBindSafetyRequest, DaemonClock,
    DaemonDetachedCommand, DaemonDetachedLauncher, DaemonPidFile, DaemonStateStore,
    DaemonStoreLayoutInitializer,
};

use super::bind::classify_bind_host_for_test;
use super::{
    FileDaemonStateStore, StdDaemonBindSafetyChecker, StdDaemonDetachedLauncher,
    StdDaemonStoreLayoutInitializer, SystemDaemonClock,
};

#[test]
fn daemon_layout_initializer_creates_runtime_and_log_dirs() {
    let root = unique_root("layout");
    let layout = daemon_layout(&root);

    StdDaemonStoreLayoutInitializer
        .ensure_daemon_store_layout(&layout)
        .expect("ensure daemon layout");

    assert!(layout.runtime_dir.is_dir());
    assert!(layout.log_dir.is_dir());
}

#[test]
fn file_daemon_state_store_round_trips_process_metadata_and_pid_file() {
    let root = unique_root("state");
    let layout = daemon_layout(&root);
    StdDaemonStoreLayoutInitializer
        .ensure_daemon_store_layout(&layout)
        .expect("ensure daemon layout");
    let store = FileDaemonStateStore;
    let metadata = daemon_metadata(42);

    store
        .record_process_start(&layout, &metadata)
        .expect("record process");
    let snapshot = store
        .inspect_daemon_store(&layout)
        .expect("inspect daemon state");

    assert!(snapshot.home_dir_exists);
    assert!(snapshot.runtime_dir_exists);
    assert!(snapshot.log_dir_exists);
    assert!(snapshot.process_path_exists);
    assert!(snapshot.pid_path_exists);
    assert_eq!(snapshot.process, Some(metadata.clone()));
    assert_eq!(snapshot.pid_file, Some(DaemonPidFile::Valid(42)));

    store
        .clear_process_if_matches(&layout, Some(7))
        .expect("ignore non-matching pid");
    assert!(layout.process_metadata_path().exists());

    store
        .clear_process_if_matches(&layout, Some(42))
        .expect("clear matching pid");
    let cleared = store
        .inspect_daemon_store(&layout)
        .expect("inspect cleared state");
    assert_eq!(cleared.process, None);
    assert_eq!(cleared.pid_file, None);
}

#[test]
fn file_daemon_state_store_reports_invalid_pid_files_without_failing_snapshot() {
    let root = unique_root("invalid-pid");
    let layout = daemon_layout(&root);
    StdDaemonStoreLayoutInitializer
        .ensure_daemon_store_layout(&layout)
        .expect("ensure daemon layout");
    fs::write(layout.pid_path(), "not-a-pid\n").expect("write invalid pid");

    let snapshot = FileDaemonStateStore
        .inspect_daemon_store(&layout)
        .expect("inspect daemon state");

    assert!(matches!(
        snapshot.pid_file,
        Some(DaemonPidFile::Invalid { .. })
    ));
}

#[test]
fn bind_safety_matches_http_daemon_contract() {
    assert_eq!(
        classify_bind_host_for_test("localhost"),
        DaemonBindHostClass::Loopback
    );
    assert_eq!(
        classify_bind_host_for_test("[::1]"),
        DaemonBindHostClass::Loopback
    );
    assert_eq!(
        classify_bind_host_for_test("0.0.0.0"),
        DaemonBindHostClass::Wildcard
    );
    assert_eq!(
        classify_bind_host_for_test("agent.local"),
        DaemonBindHostClass::NonLoopback
    );

    let checker = StdDaemonBindSafetyChecker;
    assert!(checker
        .check_bind_safety(DaemonBindSafetyRequest {
            bind: DaemonBind::from_optional(Some("127.0.0.1"), None).expect("loopback"),
            token_enabled: false,
            allow_unsafe_bind: false,
        })
        .expect("loopback")
        .warnings
        .is_empty());
    assert!(checker
        .check_bind_safety(DaemonBindSafetyRequest {
            bind: DaemonBind::from_optional(Some("0.0.0.0"), None).expect("wildcard"),
            token_enabled: false,
            allow_unsafe_bind: false,
        })
        .is_err());
    assert_eq!(
        checker
            .check_bind_safety(DaemonBindSafetyRequest {
                bind: DaemonBind::from_optional(Some("0.0.0.0"), None).expect("wildcard"),
                token_enabled: true,
                allow_unsafe_bind: false,
            })
            .expect("token protected")
            .warnings
            .len(),
        1
    );
}

#[test]
fn system_daemon_clock_returns_rfc3339_timestamp() {
    let timestamp = SystemDaemonClock.now_rfc3339().expect("timestamp");

    OffsetDateTime::parse(&timestamp, &Rfc3339).expect("rfc3339 timestamp");
}

#[test]
fn detached_launcher_creates_log_files_before_reporting_spawn_errors() {
    let root = unique_root("launcher");
    let command = DaemonDetachedCommand {
        executable: root.join("missing-tentgent"),
        args: vec!["daemon".to_string(), "run".to_string()],
        stdout_log_path: root.join("logs/stdout.log"),
        stderr_log_path: root.join("logs/stderr.log"),
    };

    let error = StdDaemonDetachedLauncher
        .launch_detached(&command)
        .expect_err("missing executable should fail");

    assert!(command.stdout_log_path.exists());
    assert!(command.stderr_log_path.exists());
    assert!(error.to_string().contains("launch detached daemon"));
}

#[test]
fn readiness_status_warning_matches_cli_start_behavior() {
    assert_eq!(daemon_status_probe_warning(200, true, "200 OK"), None);
    assert_eq!(
        daemon_status_probe_warning(401, false, "401 Unauthorized").as_deref(),
        Some("daemon ready but status requires a valid token")
    );
    assert_eq!(
        daemon_status_probe_warning(500, false, "500 Internal Server Error").as_deref(),
        Some("daemon ready but /v1/status returned 500 Internal Server Error")
    );
}

fn daemon_layout(root: &PathBuf) -> DaemonStoreLayout {
    DaemonStoreLayout::from_home_runtime_log_dirs(
        root.join("home"),
        root.join("home/runtime"),
        root.join("home/logs"),
    )
}

fn daemon_metadata(pid: u32) -> DaemonProcessMetadata {
    DaemonProcessMetadata {
        pid,
        host: DEFAULT_DAEMON_HOST.to_string(),
        port: DEFAULT_DAEMON_PORT,
        started_at: "2026-05-17T00:00:00Z".to_string(),
    }
}

fn unique_root(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "tentgent-kernel-daemon-infra-{label}-{}-{nanos}",
        std::process::id()
    ))
}
