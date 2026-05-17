use std::path::PathBuf;

use crate::features::model::domain::{
    default_model_capabilities, default_model_capability_source, ModelFormat, ModelMetadata,
    ModelRef, ModelSourceKind, ModelStoreLayout,
};
use crate::features::model::infra::FileModelCatalogStore;
use crate::features::model::ports::ModelCatalogStore;
use crate::features::server::domain::{CloudProvider, LaunchMode, ServerRefSelector};
use crate::features::server::infra::{
    FileServerCatalogStore, StdServerIdentityGenerator, StdServerStoreLayoutInitializer,
};
use crate::features::server::ports::{ServerClock, ServerProcessController, ServerProcessProbe};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
};

use super::{
    ServerInspectRequest, ServerLifecycleUseCase, ServerListRequest, ServerPrepareRequest,
    ServerRecordProcessStartRequest, ServerRemoveRequest, ServerResolveForStartRequest,
    ServerSpecUseCase, ServerStopRequest, StdServerUseCase,
};

#[test]
fn standard_server_usecase_prepares_cloud_specs_and_reuses_aliases() {
    let fixture = Fixture::new("cloud");
    let layout_resolver = StdRuntimeLayoutResolver;
    let initializer = StdServerStoreLayoutInitializer;
    let model_catalog = FileModelCatalogStore;
    let identity = StdServerIdentityGenerator;
    let catalog = FileServerCatalogStore::new(StaticProcessProbe { running: false });
    let controller = StaticProcessController;
    let clock = StaticClock;
    let servers = StdServerUseCase::new(
        &layout_resolver,
        &initializer,
        &model_catalog,
        &identity,
        &catalog,
        &controller,
        &clock,
    );

    let first = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: "claude:claude-3-5-sonnet-latest".to_string(),
            host: Some("127.0.0.1".to_string()),
            port: Some(8780),
            lazy_load: false,
            idle_seconds: None,
        })
        .expect("prepare cloud server");
    assert!(first.outcome.created);
    assert_eq!(
        first.outcome.inspection.spec.provider,
        Some(CloudProvider::Anthropic)
    );

    let reused = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: "anthropic:claude-3-5-sonnet-latest".to_string(),
            host: Some("127.0.0.1".to_string()),
            port: Some(8780),
            lazy_load: false,
            idle_seconds: None,
        })
        .expect("reuse cloud server");
    assert!(!reused.outcome.created);
    assert_eq!(
        first.outcome.inspection.spec.server_ref,
        reused.outcome.inspection.spec.server_ref
    );

    let listed = servers
        .list_servers(ServerListRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            running_only: false,
        })
        .expect("list servers");
    assert_eq!(listed.servers.len(), 1);

    let selector = ServerRefSelector::parse(first.outcome.inspection.spec.short_ref.clone())
        .expect("selector");
    let removed = servers
        .remove_server(ServerRemoveRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            selector,
        })
        .expect("remove cloud server");
    assert!(!removed.outcome.inspection.server_dir.exists());
}

#[test]
fn standard_server_usecase_prepares_local_specs_and_tracks_process_state() {
    let fixture = Fixture::new("local");
    fixture.write_chat_model();
    let layout_resolver = StdRuntimeLayoutResolver;
    let initializer = StdServerStoreLayoutInitializer;
    let model_catalog = FileModelCatalogStore;
    let identity = StdServerIdentityGenerator;
    let catalog = FileServerCatalogStore::new(StaticProcessProbe { running: true });
    let controller = StaticProcessController;
    let clock = StaticClock;
    let servers = StdServerUseCase::new(
        &layout_resolver,
        &initializer,
        &model_catalog,
        &identity,
        &catalog,
        &controller,
        &clock,
    );

    let prepared = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: fixture.model_ref.short_ref().to_string(),
            host: None,
            port: Some(8781),
            lazy_load: true,
            idle_seconds: Some(30),
        })
        .expect("prepare local server");
    assert!(prepared.outcome.created);
    assert_eq!(
        prepared.outcome.inspection.spec.model_ref.as_ref(),
        Some(&fixture.model_ref)
    );

    let selector = ServerRefSelector::parse(prepared.outcome.inspection.spec.short_ref.clone())
        .expect("selector");
    let startable = servers
        .resolve_for_start(ServerResolveForStartRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            selector: selector.clone(),
        })
        .expect("resolve for start");
    assert!(!startable.inspection.running);

    let recorded = servers
        .record_process_start(ServerRecordProcessStartRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            server_ref: prepared.outcome.inspection.spec.server_ref.clone(),
            pid: 42,
            launch_mode: LaunchMode::Background,
        })
        .expect("record process start");
    assert!(recorded.inspection.running);
    assert_eq!(recorded.inspection.process.expect("process").pid, 42);

    let stopped = servers
        .stop_server(ServerStopRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            selector: selector.clone(),
        })
        .expect("stop server");
    assert_eq!(stopped.outcome.stopped_pid, 42);
    assert!(!stopped.outcome.inspection.running);
    assert!(stopped.outcome.inspection.process.is_none());

    let inspected = servers
        .inspect_server(ServerInspectRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            selector: selector.clone(),
        })
        .expect("inspect stopped");
    assert!(!inspected.inspection.running);

    servers
        .remove_server(ServerRemoveRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            selector,
        })
        .expect("remove local server");
}

struct Fixture {
    home: PathBuf,
    data: PathBuf,
    model_ref: ModelRef,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "tentgent-kernel-server-usecase-{label}-{}-{nanos}",
            std::process::id()
        ));
        Self {
            home: root.join("home"),
            data: root.join("data"),
            model_ref: ModelRef::parse(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            )
            .expect("model ref"),
        }
    }

    fn layout_input(&self, mode: LayoutResolveMode) -> RuntimeLayoutInput {
        RuntimeLayoutInput {
            mode,
            home_dir: Some(self.home.clone()),
            data_root_dir: Some(self.data.clone()),
        }
    }

    fn write_chat_model(&self) {
        let layout = StdRuntimeLayoutResolver
            .resolve(self.layout_input(LayoutResolveMode::Create))
            .expect("layout");
        let model_store = ModelStoreLayout::from_models_dir(layout.models_dir);
        FileModelCatalogStore
            .save_model_metadata(
                &model_store,
                &ModelMetadata {
                    model_ref: self.model_ref.clone(),
                    short_ref: self.model_ref.short_ref().to_string(),
                    source_kind: ModelSourceKind::Local,
                    source_repo: None,
                    source_revision: None,
                    source_path: Some("/tmp/model".to_string()),
                    primary_format: ModelFormat::Safetensors,
                    detected_formats: vec![ModelFormat::Safetensors],
                    model_capabilities: default_model_capabilities(),
                    model_capability_source: default_model_capability_source(),
                    file_count: 1,
                    total_bytes: 1024,
                    imported_at: "2026-05-17T00:00:00Z".to_string(),
                },
            )
            .expect("save model");
    }
}

struct StaticClock;

impl ServerClock for StaticClock {
    fn now_rfc3339(&self) -> KernelResult<String> {
        Ok("2026-05-17T00:00:00Z".to_string())
    }
}

struct StaticProcessProbe {
    running: bool,
}

impl ServerProcessProbe for StaticProcessProbe {
    fn is_process_running(&self, _pid: u32) -> KernelResult<bool> {
        Ok(self.running)
    }
}

struct StaticProcessController;

impl ServerProcessController for StaticProcessController {
    fn terminate_process(&self, _pid: u32) -> KernelResult<()> {
        Ok(())
    }
}
