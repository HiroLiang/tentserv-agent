use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tentgent_kernel::features::server::{
    domain::ServerStoreLayout,
    ports::{ServerCatalogStore, ServerProcessProbe, ServerProcessStartRecord},
};

struct Alive;
impl ServerProcessProbe for Alive {
    fn is_process_running(&self, _: u32) -> tentgent_kernel::foundation::error::KernelResult<bool> {
        Ok(true)
    }
}

struct HealthFixture {
    body: Arc<Mutex<Value>>,
    stopped: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    port: u16,
}

impl HealthFixture {
    fn new() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let body = Arc::new(Mutex::new(serde_json::json!({})));
        let stopped = Arc::new(AtomicBool::new(false));
        let (reply, stop) = (body.clone(), stopped.clone());
        let thread = std::thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                if let Ok((mut stream, _)) = listener.accept() {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut request = [0; 2048];
                    stream.read(&mut request).unwrap();
                    let body = reply.lock().unwrap().to_string();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                } else {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        });
        Self {
            body,
            stopped,
            thread: Some(thread),
            port,
        }
    }
    fn health(&self, inspection: &ServerInspection, ready: bool) {
        *self.body.lock().unwrap() = serde_json::json!({
            "server_ref":inspection.spec.server_ref, "runtime_home":inspection.home_dir,
            "process_token":"worker", "ok":ready, "ready":ready,
            "status":if ready {"ready"} else {"starting"}
        });
    }
}

impl Drop for HealthFixture {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        self.thread.take().unwrap().join().unwrap();
    }
}

#[tokio::test]
async fn detached_observation_expiry_preserves_process_and_does_not_claim_ready() {
    let home = std::env::temp_dir().join(format!(
        "tentgent-readiness-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&home).unwrap();
    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: Some(home.clone()),
            data_root_dir: None,
        })
        .unwrap();
    let fixture = HealthFixture::new();
    let catalog = FileServerCatalogStore::new(Alive);
    let store = ServerStoreLayout::from_home_and_servers_dir(&layout.home_dir, &layout.servers_dir);
    let spec = local_server_spec();
    catalog.save_server_spec(&store, &spec).unwrap();
    let inspection = catalog
        .record_process_start(
            &store,
            &spec.server_ref,
            ServerProcessStartRecord {
                pid: 123,
                process_token: Some("worker".into()),
                launch_mode: LaunchMode::Background,
                started_at: spec.created_at.clone(),
                bound_port: fixture.port,
            },
        )
        .unwrap();
    fixture.health(&inspection, false);
    let controller = StdServerProcessController::default();
    let server = StdServerUseCase::new_with_cluster_catalog(
        &StdRuntimeLayoutResolver,
        &StdServerStoreLayoutInitializer,
        &FileModelCatalogStore,
        &FileModelCapabilityProofStore,
        &FileClusterCatalogStore,
        &StdServerIdentityGenerator,
        &catalog,
        &controller,
        &SystemServerClock,
    );
    let result = verify_background_launch(&server, &layout, &inspection, 123)
        .await
        .unwrap();
    assert!(!result.ready && result.inspection.running);
    assert!(inspection.process_path.exists());
    assert_eq!(
        background_health_status(&inspection),
        BackgroundHealthStatus::Starting
    );
    fixture.health(&inspection, true);
    assert_eq!(
        background_health_status(&inspection),
        BackgroundHealthStatus::Matches
    );
    fixture.body.lock().unwrap()["process_token"] = serde_json::json!("stale-worker");
    assert!(matches!(
        background_health_status(&inspection),
        BackgroundHealthStatus::DifferentServer(_)
    ));
    assert!(
        !layout.models_dir.exists()
            || std::fs::read_dir(&layout.models_dir)
                .unwrap()
                .next()
                .is_none()
    );
    drop(fixture);
    std::fs::remove_dir_all(home).unwrap();
}
