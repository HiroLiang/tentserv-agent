use std::path::PathBuf;

use crate::features::model::domain::{
    default_model_capabilities, default_model_capability_source, ModelCapability, ModelFormat,
    ModelMetadata, ModelRef, ModelSourceKind, ModelStoreLayout,
};
use crate::features::model::infra::FileModelCatalogStore;
use crate::features::model::ports::ModelCatalogStore;
use crate::features::server::domain::{
    CloudProvider, LaunchMode, ServerCapability, ServerRef, ServerRefSelector, ServerRuntimeKind,
    ServerSpec,
};
use crate::features::server::infra::{
    FileServerCatalogStore, StdServerIdentityGenerator, StdServerStoreLayoutInitializer,
};
use crate::features::server::ports::{
    ServerCatalogStore, ServerClock, ServerProcessController, ServerProcessProbe,
    ServerStoreLayoutInitializer,
};
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
            capability: ServerCapability::Chat,
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
            capability: ServerCapability::Chat,
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
            capability: ServerCapability::Chat,
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

#[test]
fn standard_server_usecase_rejects_non_chat_models_for_chat_specs() {
    for (label, capability) in [
        ("embedding", ModelCapability::Embedding),
        ("rerank", ModelCapability::Rerank),
    ] {
        let fixture = Fixture::new(label);
        fixture.write_model_capabilities(vec![capability]);
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

        let err = servers
            .prepare_server(ServerPrepareRequest {
                layout: fixture.layout_input(LayoutResolveMode::Create),
                runtime_ref: fixture.model_ref.short_ref().to_string(),
                capability: ServerCapability::Chat,
                host: None,
                port: Some(8781),
                lazy_load: false,
                idle_seconds: None,
            })
            .expect_err("non-chat model should not prepare a chat server");

        let message = err.to_string();
        assert!(message.contains("server capability `chat`"));
        assert!(message.contains("requires model capability `chat`"));
        assert!(message.contains(capability.as_str()));
    }
}

#[test]
fn standard_server_usecase_allows_embedding_stored_specs_before_start() {
    let fixture = Fixture::new("stored-embedding");
    fixture.write_model_capabilities(vec![ModelCapability::Embedding]);
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
    let layout = StdRuntimeLayoutResolver
        .resolve(fixture.layout_input(LayoutResolveMode::Create))
        .expect("layout");
    let server_store =
        crate::features::server::domain::ServerStoreLayout::from_home_and_servers_dir(
            layout.home_dir.clone(),
            layout.servers_dir.clone(),
        );
    StdServerStoreLayoutInitializer
        .ensure_server_store_layout(&server_store)
        .expect("server layout");
    let server_ref =
        ServerRef::parse("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
            .expect("server ref");
    FileServerCatalogStore::new(StaticProcessProbe { running: false })
        .save_server_spec(
            &server_store,
            &ServerSpec {
                server_ref: server_ref.clone(),
                short_ref: server_ref.short_ref().to_string(),
                runtime_kind: ServerRuntimeKind::Local,
                capability: ServerCapability::Embedding,
                model_ref: Some(fixture.model_ref.clone()),
                provider: None,
                provider_model: None,
                host: "127.0.0.1".to_string(),
                port: 8781,
                lazy_load: false,
                idle_seconds: None,
                created_at: "2026-05-17T00:00:00Z".to_string(),
            },
        )
        .expect("save server spec");

    let result = servers
        .resolve_for_start(ServerResolveForStartRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            selector: ServerRefSelector::parse(server_ref.short_ref()).expect("selector"),
        })
        .expect("embedding server runtime is implemented");

    assert_eq!(
        result.inspection.spec.capability,
        ServerCapability::Embedding
    );
}

#[test]
fn standard_server_usecase_prepares_embedding_specs() {
    let fixture = Fixture::new("embedding");
    fixture.write_model_capabilities(vec![ModelCapability::Embedding]);
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

    let prepared = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: fixture.model_ref.short_ref().to_string(),
            capability: ServerCapability::Embedding,
            host: None,
            port: Some(8781),
            lazy_load: false,
            idle_seconds: None,
        })
        .expect("prepare embedding server");

    assert!(prepared.outcome.created);
    assert_eq!(
        prepared.outcome.inspection.spec.capability,
        ServerCapability::Embedding
    );
    assert_eq!(
        prepared.outcome.inspection.spec.model_ref.as_ref(),
        Some(&fixture.model_ref)
    );
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
        self.write_model_capabilities(default_model_capabilities());
    }

    fn write_model_capabilities(&self, capabilities: Vec<ModelCapability>) {
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
                    model_capabilities: capabilities,
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
