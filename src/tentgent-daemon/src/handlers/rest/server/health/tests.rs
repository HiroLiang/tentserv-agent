use super::*;
use axum::{routing::get, Router};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tentgent_kernel::features::server::domain::{LaunchMode, ServerProcessMetadata};

fn inspection() -> ServerInspection {
    ServerInspection {
        spec: serde_json::from_value(json!({
            "server_ref":"a".repeat(64), "short_ref":"a".repeat(12),
            "runtime_kind":"local", "capability":"chat", "model_ref":"b".repeat(64),
            "host":"127.0.0.1", "port":8780, "lazy_load":false,
            "created_at":"2026-09-28T00:00:00Z"
        }))
        .unwrap(),
        home_dir: "/isolated-readiness-fixture".into(),
        server_dir: Default::default(),
        spec_path: Default::default(),
        process_path: Default::default(),
        stdout_log_path: Default::default(),
        stderr_log_path: Default::default(),
        running: true,
        process: Some(ServerProcessMetadata {
            pid: 123,
            process_token: Some("current-worker".into()),
            launch_mode: LaunchMode::Background,
            started_at: "fixture".into(),
            bound_port: None,
        }),
    }
}

fn payload(inspection: &ServerInspection, ready: bool) -> Value {
    json!({"ok":ready, "ready":ready, "status":if ready {"ready"} else {"starting"},
        "server_ref":inspection.spec.server_ref, "runtime_home":inspection.home_dir,
        "process_token":"current-worker"})
}

async fn fixture() -> (
    ServerInspection,
    Arc<Mutex<Value>>,
    tokio::task::JoinHandle<()>,
) {
    let mut inspection = inspection();
    let body = Arc::new(Mutex::new(payload(&inspection, false)));
    let reply = body.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    inspection.process.as_mut().unwrap().bound_port = Some(listener.local_addr().unwrap().port());
    let router = Router::new().route(
        "/healthz",
        get(move || {
            let body = reply.lock().unwrap().clone();
            async { Json(body) }
        }),
    );
    let worker = tokio::spawn(async { axum::serve(listener, router).await.unwrap() });
    (inspection, body, worker)
}

#[tokio::test]
async fn readiness_expiry_is_only_observation_and_later_completion_is_ready() {
    let (inspection, body, worker) = fixture().await;
    let pending = wait_for_server_ready(&inspection, 0).await;
    assert!(!pending.ready);
    assert!(pending.reachable);
    assert_eq!(pending.target_status, Some(200));
    assert_eq!(pending.target_health.unwrap()["status"], "starting");
    assert!(!worker.is_finished());
    *body.lock().unwrap() = payload(&inspection, true);
    let ready = wait_for_server_ready(&inspection, 1).await;
    assert!(ready.ready && ready.reachable);
    assert!(ready.error.is_none());
    worker.abort();
}

#[tokio::test]
async fn reachable_stale_or_legacy_local_health_is_not_load_readiness() {
    let (inspection, body, worker) = fixture().await;
    for field in ["process_token", "server_ref", "runtime_home"] {
        let mut stale = payload(&inspection, true);
        stale[field] = json!("wrong");
        *body.lock().unwrap() = stale;
        let probe = probe_server_health(&inspection).await;
        assert!(probe.reachable);
        assert!(!probe.ready, "{field}");
    }
    let mut old = payload(&inspection, true);
    old.as_object_mut().unwrap().remove("ready");
    old.as_object_mut().unwrap().remove("status");
    *body.lock().unwrap() = old;
    assert!(!probe_server_health(&inspection).await.ready);
    // Cloud and Cluster retain their existing health contract in this slice.
    for kind in [
        tentgent_kernel::features::server::domain::ServerRuntimeKind::Cloud,
        tentgent_kernel::features::server::domain::ServerRuntimeKind::Cluster,
    ] {
        let mut other = inspection.clone();
        other.spec.runtime_kind = kind;
        assert!(probe_server_health(&other).await.ready);
    }
    worker.abort();
}

#[tokio::test]
async fn stopped_server_never_probes_or_reports_ready() {
    let mut inspection = inspection();
    inspection.running = false;
    let probe = probe_server_health(&inspection).await;
    assert!(!probe.ready && !probe.reachable);
    assert!(probe.target_status.is_none());
}
