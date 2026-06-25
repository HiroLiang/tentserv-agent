use std::path::PathBuf;

use crate::features::model::domain::{
    default_model_capabilities, default_model_capability_source, MlxRuntimeFamily, ModelCapability,
    ModelCapabilityProof, ModelCapabilityProofSource, ModelCapabilityProofStatus, ModelFormat,
    ModelImportMethod, ModelManifest, ModelManifestEntry, ModelMetadata, ModelRef, ModelSourceKind,
    ModelStoreLayout, ModelVariantMetadata, ModelVariantStatus, SOURCE_DIRNAME,
};
use crate::features::model::infra::{FileModelCapabilityProofStore, FileModelCatalogStore};
use crate::features::model::ports::{ModelCapabilityProofStore, ModelCatalogStore};
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

mod support_gate;

#[test]
fn standard_server_usecase_prepares_cloud_specs_and_reuses_aliases() {
    let fixture = Fixture::new("cloud");
    let layout_resolver = StdRuntimeLayoutResolver;
    let initializer = StdServerStoreLayoutInitializer;
    let model_catalog = FileModelCatalogStore;
    let model_proofs = FileModelCapabilityProofStore;
    let identity = StdServerIdentityGenerator;
    let catalog = FileServerCatalogStore::new(StaticProcessProbe { running: false });
    let controller = StaticProcessController;
    let clock = StaticClock;
    let servers = StdServerUseCase::new(
        &layout_resolver,
        &initializer,
        &model_catalog,
        &model_proofs,
        &identity,
        &catalog,
        &controller,
        &clock,
    );

    let first = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: "claude:claude-3-5-sonnet-latest".to_string(),
            capability: Some(ServerCapability::Chat),
            host: Some("127.0.0.1".to_string()),
            port: Some(8780),
            lazy_load: false,
            idle_seconds: None,
            allow_unverified: true,
        })
        .expect("prepare cloud server");
    assert!(first.outcome.created);
    assert_eq!(
        first.outcome.inspection.spec.provider,
        Some(CloudProvider::Anthropic)
    );
    assert_eq!(
        first.outcome.inspection.spec.capability,
        ServerCapability::Chat
    );
    assert!(first.outcome.inspection.spec.runtime_profile.is_none());

    let reused = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: "anthropic:claude-3-5-sonnet-latest".to_string(),
            capability: Some(ServerCapability::Chat),
            host: Some("127.0.0.1".to_string()),
            port: Some(8780),
            lazy_load: false,
            idle_seconds: None,
            allow_unverified: true,
        })
        .expect("reuse cloud server");
    assert!(!reused.outcome.created);
    assert_eq!(
        first.outcome.inspection.spec.server_ref,
        reused.outcome.inspection.spec.server_ref
    );

    let embedding = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: "gemini:text-embedding-004".to_string(),
            capability: Some(ServerCapability::Embedding),
            host: Some("127.0.0.1".to_string()),
            port: Some(8781),
            lazy_load: false,
            idle_seconds: None,
            allow_unverified: true,
        })
        .expect("prepare cloud embedding server");
    assert!(embedding.outcome.created);
    assert_eq!(
        embedding.outcome.inspection.spec.provider,
        Some(CloudProvider::Gemini)
    );
    assert_eq!(
        embedding.outcome.inspection.spec.capability,
        ServerCapability::Embedding
    );
    assert!(embedding.outcome.inspection.spec.runtime_profile.is_none());

    let listed = servers
        .list_servers(ServerListRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            running_only: false,
        })
        .expect("list servers");
    assert_eq!(listed.servers.len(), 2);

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
fn standard_server_usecase_rejects_cloud_capabilities_not_supported_by_provider() {
    let fixture = Fixture::new("cloud-unsupported-capability");
    let layout_resolver = StdRuntimeLayoutResolver;
    let initializer = StdServerStoreLayoutInitializer;
    let model_catalog = FileModelCatalogStore;
    let model_proofs = FileModelCapabilityProofStore;
    let identity = StdServerIdentityGenerator;
    let catalog = FileServerCatalogStore::new(StaticProcessProbe { running: false });
    let controller = StaticProcessController;
    let clock = StaticClock;
    let servers = StdServerUseCase::new(
        &layout_resolver,
        &initializer,
        &model_catalog,
        &model_proofs,
        &identity,
        &catalog,
        &controller,
        &clock,
    );

    let err = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: "anthropic:claude-3-5-sonnet-latest".to_string(),
            capability: Some(ServerCapability::Embedding),
            host: None,
            port: None,
            lazy_load: false,
            idle_seconds: None,
            allow_unverified: true,
        })
        .expect_err("anthropic embedding server should be rejected");

    let message = err.to_string();
    assert!(message.contains("cloud provider `anthropic`"));
    assert!(message.contains("server capability `embedding`"));
    assert!(message.contains("[chat, vision-chat]"));
}

#[test]
fn standard_server_usecase_prepares_local_specs_and_tracks_process_state() {
    let fixture = Fixture::new("local");
    fixture.write_chat_model();
    let layout_resolver = StdRuntimeLayoutResolver;
    let initializer = StdServerStoreLayoutInitializer;
    let model_catalog = FileModelCatalogStore;
    let model_proofs = FileModelCapabilityProofStore;
    let identity = StdServerIdentityGenerator;
    let catalog = FileServerCatalogStore::new(StaticProcessProbe { running: true });
    let controller = StaticProcessController;
    let clock = StaticClock;
    let servers = StdServerUseCase::new(
        &layout_resolver,
        &initializer,
        &model_catalog,
        &model_proofs,
        &identity,
        &catalog,
        &controller,
        &clock,
    );

    let prepared = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: fixture.model_ref.short_ref().to_string(),
            capability: Some(ServerCapability::Chat),
            host: None,
            port: Some(8781),
            lazy_load: true,
            idle_seconds: Some(30),
            allow_unverified: true,
        })
        .expect("prepare local server");
    assert!(prepared.outcome.created);
    assert_eq!(
        prepared.outcome.inspection.spec.model_ref.as_ref(),
        Some(&fixture.model_ref)
    );
    assert_eq!(
        prepared
            .outcome
            .inspection
            .spec
            .runtime_profile
            .as_ref()
            .map(|profile| profile.label())
            .as_deref(),
        Some("local-chat-transformers-peft-v1")
    );

    let selector = ServerRefSelector::parse(prepared.outcome.inspection.spec.short_ref.clone())
        .expect("selector");
    let startable = servers
        .resolve_for_start(ServerResolveForStartRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            selector: selector.clone(),
            allow_unverified: true,
        })
        .expect("resolve for start");
    assert!(!startable.inspection.running);

    let recorded = servers
        .record_process_start(ServerRecordProcessStartRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            server_ref: prepared.outcome.inspection.spec.server_ref.clone(),
            pid: 42,
            bound_port: 8781,
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
fn standard_server_usecase_uses_auto_default_port_when_port_is_omitted() {
    let fixture = Fixture::new("local-default-port");
    fixture.write_chat_model();
    let layout_resolver = StdRuntimeLayoutResolver;
    let initializer = StdServerStoreLayoutInitializer;
    let model_catalog = FileModelCatalogStore;
    let model_proofs = FileModelCapabilityProofStore;
    let identity = StdServerIdentityGenerator;
    let catalog = FileServerCatalogStore::new(StaticProcessProbe { running: false });
    let controller = StaticProcessController;
    let clock = StaticClock;
    let servers = StdServerUseCase::new(
        &layout_resolver,
        &initializer,
        &model_catalog,
        &model_proofs,
        &identity,
        &catalog,
        &controller,
        &clock,
    );

    let prepared = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: fixture.model_ref.short_ref().to_string(),
            capability: Some(ServerCapability::Chat),
            host: None,
            port: None,
            lazy_load: false,
            idle_seconds: None,
            allow_unverified: true,
        })
        .expect("prepare local server");

    assert_eq!(prepared.outcome.inspection.spec.port, 8780);
    assert!(prepared.outcome.inspection.spec.port_auto);
}

#[test]
fn standard_server_usecase_rejects_non_chat_models_for_chat_specs() {
    for (label, capability) in [
        ("embedding", ModelCapability::Embedding),
        ("rerank", ModelCapability::Rerank),
        ("audio-transcription", ModelCapability::AudioTranscription),
        ("vision-chat", ModelCapability::VisionChat),
    ] {
        let fixture = Fixture::new(label);
        fixture.write_model_capabilities(vec![capability]);
        let layout_resolver = StdRuntimeLayoutResolver;
        let initializer = StdServerStoreLayoutInitializer;
        let model_catalog = FileModelCatalogStore;
        let model_proofs = FileModelCapabilityProofStore;
        let identity = StdServerIdentityGenerator;
        let catalog = FileServerCatalogStore::new(StaticProcessProbe { running: false });
        let controller = StaticProcessController;
        let clock = StaticClock;
        let servers = StdServerUseCase::new(
            &layout_resolver,
            &initializer,
            &model_catalog,
            &model_proofs,
            &identity,
            &catalog,
            &controller,
            &clock,
        );

        let err = servers
            .prepare_server(ServerPrepareRequest {
                layout: fixture.layout_input(LayoutResolveMode::Create),
                runtime_ref: fixture.model_ref.short_ref().to_string(),
                capability: Some(ServerCapability::Chat),
                host: None,
                port: Some(8781),
                lazy_load: false,
                idle_seconds: None,
                allow_unverified: true,
            })
            .expect_err("non-chat model should not prepare a chat server");

        let message = err.to_string();
        assert!(message.contains("server capability `chat`"));
        assert!(message.contains("requires model capability `chat`"));
        assert!(message.contains(capability.as_str()));
    }
}

#[test]
fn standard_server_usecase_infers_capability_from_local_model_metadata() {
    let fixture = Fixture::new("inferred-vision");
    fixture.write_model_format_capabilities(
        ModelFormat::Safetensors,
        vec![
            ModelCapability::Chat,
            ModelCapability::VisionChat,
            ModelCapability::VideoUnderstanding,
        ],
    );
    let layout_resolver = StdRuntimeLayoutResolver;
    let initializer = StdServerStoreLayoutInitializer;
    let model_catalog = FileModelCatalogStore;
    let model_proofs = FileModelCapabilityProofStore;
    let identity = StdServerIdentityGenerator;
    let catalog = FileServerCatalogStore::new(StaticProcessProbe { running: false });
    let controller = StaticProcessController;
    let clock = StaticClock;
    let servers = StdServerUseCase::new(
        &layout_resolver,
        &initializer,
        &model_catalog,
        &model_proofs,
        &identity,
        &catalog,
        &controller,
        &clock,
    );

    let prepared = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: fixture.model_ref.short_ref().to_string(),
            capability: None,
            host: None,
            port: Some(8781),
            lazy_load: false,
            idle_seconds: None,
            allow_unverified: true,
        })
        .expect("prepare inferred server");

    assert_eq!(
        prepared.outcome.inspection.spec.capability,
        ServerCapability::VideoUnderstanding
    );
}

#[test]
fn standard_server_usecase_rejects_embedding_stored_specs_without_runtime_profile() {
    let fixture = Fixture::new("stored-embedding");
    fixture.write_model_capabilities(vec![ModelCapability::Embedding]);
    let layout_resolver = StdRuntimeLayoutResolver;
    let initializer = StdServerStoreLayoutInitializer;
    let model_catalog = FileModelCatalogStore;
    let model_proofs = FileModelCapabilityProofStore;
    let identity = StdServerIdentityGenerator;
    let catalog = FileServerCatalogStore::new(StaticProcessProbe { running: false });
    let controller = StaticProcessController;
    let clock = StaticClock;
    let servers = StdServerUseCase::new(
        &layout_resolver,
        &initializer,
        &model_catalog,
        &model_proofs,
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
                runtime_profile: None,
                host: "127.0.0.1".to_string(),
                port: 8781,
                port_auto: false,
                lazy_load: false,
                idle_seconds: None,
                created_at: "2026-05-17T00:00:00Z".to_string(),
            },
        )
        .expect("save server spec");

    let err = servers
        .resolve_for_start(ServerResolveForStartRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            selector: ServerRefSelector::parse(server_ref.short_ref()).expect("selector"),
            allow_unverified: true,
        })
        .expect_err("embedding server without profile should fail");

    let message = err.to_string();
    assert!(message.contains("requires a runtime profile"));
    assert!(message.contains("embedding"));
}

#[test]
fn standard_server_usecase_prepares_embedding_specs() {
    let fixture = Fixture::new("embedding");
    fixture.write_model_capabilities(vec![ModelCapability::Embedding]);
    let layout_resolver = StdRuntimeLayoutResolver;
    let initializer = StdServerStoreLayoutInitializer;
    let model_catalog = FileModelCatalogStore;
    let model_proofs = FileModelCapabilityProofStore;
    let identity = StdServerIdentityGenerator;
    let catalog = FileServerCatalogStore::new(StaticProcessProbe { running: false });
    let controller = StaticProcessController;
    let clock = StaticClock;
    let servers = StdServerUseCase::new(
        &layout_resolver,
        &initializer,
        &model_catalog,
        &model_proofs,
        &identity,
        &catalog,
        &controller,
        &clock,
    );

    let prepared = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: fixture.model_ref.short_ref().to_string(),
            capability: Some(ServerCapability::Embedding),
            host: None,
            port: Some(8781),
            lazy_load: false,
            idle_seconds: None,
            allow_unverified: true,
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
    assert_eq!(
        prepared
            .outcome
            .inspection
            .spec
            .runtime_profile
            .as_ref()
            .map(|profile| profile.label())
            .as_deref(),
        Some("local-embedding-transformers-peft-v1")
    );
}

#[test]
fn standard_server_usecase_rejects_mlx_embedding_specs_without_profile() {
    let fixture = Fixture::new("embedding-mlx");
    fixture.write_model_format_capabilities(ModelFormat::Mlx, vec![ModelCapability::Embedding]);
    let layout_resolver = StdRuntimeLayoutResolver;
    let initializer = StdServerStoreLayoutInitializer;
    let model_catalog = FileModelCatalogStore;
    let model_proofs = FileModelCapabilityProofStore;
    let identity = StdServerIdentityGenerator;
    let catalog = FileServerCatalogStore::new(StaticProcessProbe { running: false });
    let controller = StaticProcessController;
    let clock = StaticClock;
    let servers = StdServerUseCase::new(
        &layout_resolver,
        &initializer,
        &model_catalog,
        &model_proofs,
        &identity,
        &catalog,
        &controller,
        &clock,
    );

    let err = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: fixture.model_ref.short_ref().to_string(),
            capability: Some(ServerCapability::Embedding),
            host: None,
            port: Some(8781),
            lazy_load: false,
            idle_seconds: None,
            allow_unverified: true,
        })
        .expect_err("mlx embedding server has no runtime profile yet");

    let message = err.to_string();
    assert!(message.contains("embedding"));
    assert!(message.contains("backend `mlx`"));
    assert!(message.contains("does not have a runtime profile"));
}

#[test]
fn standard_server_usecase_prepares_rerank_specs() {
    let fixture = Fixture::new("rerank");
    fixture.write_model_capabilities(vec![ModelCapability::Rerank]);
    let layout_resolver = StdRuntimeLayoutResolver;
    let initializer = StdServerStoreLayoutInitializer;
    let model_catalog = FileModelCatalogStore;
    let model_proofs = FileModelCapabilityProofStore;
    let identity = StdServerIdentityGenerator;
    let catalog = FileServerCatalogStore::new(StaticProcessProbe { running: false });
    let controller = StaticProcessController;
    let clock = StaticClock;
    let servers = StdServerUseCase::new(
        &layout_resolver,
        &initializer,
        &model_catalog,
        &model_proofs,
        &identity,
        &catalog,
        &controller,
        &clock,
    );

    let prepared = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: fixture.model_ref.short_ref().to_string(),
            capability: Some(ServerCapability::Rerank),
            host: None,
            port: Some(8782),
            lazy_load: false,
            idle_seconds: None,
            allow_unverified: true,
        })
        .expect("prepare rerank server");

    assert!(prepared.outcome.created);
    assert_eq!(
        prepared.outcome.inspection.spec.capability,
        ServerCapability::Rerank
    );
    assert_eq!(
        prepared.outcome.inspection.spec.model_ref.as_ref(),
        Some(&fixture.model_ref)
    );

    let selector = ServerRefSelector::parse(prepared.outcome.inspection.spec.short_ref.clone())
        .expect("selector");
    let startable = servers
        .resolve_for_start(ServerResolveForStartRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            selector,
            allow_unverified: true,
        })
        .expect("rerank server runtime is implemented");
    assert_eq!(
        startable.inspection.spec.capability,
        ServerCapability::Rerank
    );
}

#[test]
fn standard_server_usecase_prepares_model_runtime_capability_specs() {
    for (label, server_capability, model_capability, model_format) in [
        (
            "audio-speech",
            ServerCapability::AudioSpeech,
            ModelCapability::AudioSpeech,
            ModelFormat::Safetensors,
        ),
        (
            "audio-transcription",
            ServerCapability::AudioTranscription,
            ModelCapability::AudioTranscription,
            ModelFormat::Mlx,
        ),
        (
            "embedding-gguf",
            ServerCapability::Embedding,
            ModelCapability::Embedding,
            ModelFormat::Gguf,
        ),
        (
            "image-generation",
            ServerCapability::ImageGeneration,
            ModelCapability::ImageGeneration,
            ModelFormat::Diffusers,
        ),
        (
            "video-understanding",
            ServerCapability::VideoUnderstanding,
            ModelCapability::VideoUnderstanding,
            ModelFormat::Safetensors,
        ),
        (
            "vision-chat",
            ServerCapability::VisionChat,
            ModelCapability::VisionChat,
            ModelFormat::Mlx,
        ),
    ] {
        let fixture = Fixture::new(label);
        fixture.write_model_format_capabilities(model_format, vec![model_capability]);
        let layout_resolver = StdRuntimeLayoutResolver;
        let initializer = StdServerStoreLayoutInitializer;
        let model_catalog = FileModelCatalogStore;
        let model_proofs = FileModelCapabilityProofStore;
        let identity = StdServerIdentityGenerator;
        let catalog = FileServerCatalogStore::new(StaticProcessProbe { running: false });
        let controller = StaticProcessController;
        let clock = StaticClock;
        let servers = StdServerUseCase::new(
            &layout_resolver,
            &initializer,
            &model_catalog,
            &model_proofs,
            &identity,
            &catalog,
            &controller,
            &clock,
        );

        let prepared = servers
            .prepare_server(ServerPrepareRequest {
                layout: fixture.layout_input(LayoutResolveMode::Create),
                runtime_ref: fixture.model_ref.short_ref().to_string(),
                capability: Some(server_capability),
                host: None,
                port: Some(8782),
                lazy_load: false,
                idle_seconds: None,
                allow_unverified: true,
            })
            .expect("prepare model runtime server");

        assert!(prepared.outcome.created);
        assert_eq!(
            prepared.outcome.inspection.spec.capability,
            server_capability
        );
        if server_capability == ServerCapability::Embedding {
            assert_eq!(
                prepared
                    .outcome
                    .inspection
                    .spec
                    .runtime_profile
                    .as_ref()
                    .map(|profile| profile.label())
                    .as_deref(),
                Some("local-embedding-llama-cpp-v1")
            );
        }

        let selector = ServerRefSelector::parse(prepared.outcome.inspection.spec.short_ref.clone())
            .expect("selector");
        let startable = servers
            .resolve_for_start(ServerResolveForStartRequest {
                layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
                selector,
                allow_unverified: true,
            })
            .expect("model runtime server is implemented");
        assert_eq!(startable.inspection.spec.capability, server_capability);
    }
}

#[test]
fn standard_server_usecase_rejects_unsupported_non_chat_server_formats() {
    for (label, server_capability, model_capability) in [
        (
            "rerank-gguf",
            ServerCapability::Rerank,
            ModelCapability::Rerank,
        ),
        (
            "image-generation-safetensors",
            ServerCapability::ImageGeneration,
            ModelCapability::ImageGeneration,
        ),
    ] {
        let fixture = Fixture::new(label);
        let format = if server_capability == ServerCapability::ImageGeneration {
            ModelFormat::Safetensors
        } else {
            ModelFormat::Gguf
        };
        fixture.write_model_format_capabilities(format, vec![model_capability]);
        let layout_resolver = StdRuntimeLayoutResolver;
        let initializer = StdServerStoreLayoutInitializer;
        let model_catalog = FileModelCatalogStore;
        let model_proofs = FileModelCapabilityProofStore;
        let identity = StdServerIdentityGenerator;
        let catalog = FileServerCatalogStore::new(StaticProcessProbe { running: false });
        let controller = StaticProcessController;
        let clock = StaticClock;
        let servers = StdServerUseCase::new(
            &layout_resolver,
            &initializer,
            &model_catalog,
            &model_proofs,
            &identity,
            &catalog,
            &controller,
            &clock,
        );

        let err = servers
            .prepare_server(ServerPrepareRequest {
                layout: fixture.layout_input(LayoutResolveMode::Create),
                runtime_ref: fixture.model_ref.short_ref().to_string(),
                capability: Some(server_capability),
                host: None,
                port: Some(8782),
                lazy_load: false,
                idle_seconds: None,
                allow_unverified: true,
            })
            .expect_err("unsupported non-chat format");

        assert!(err.to_string().contains("does not support"));
    }
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
        self.write_model_format_capabilities(ModelFormat::Safetensors, capabilities);
    }

    fn write_model_format_capabilities(
        &self,
        format: ModelFormat,
        capabilities: Vec<ModelCapability>,
    ) {
        self.write_model_metadata(format, capabilities, ModelSourceKind::Local, None);
    }

    fn write_hf_model_format_capabilities(
        &self,
        format: ModelFormat,
        capabilities: Vec<ModelCapability>,
        source_repo: &str,
    ) {
        self.write_model_metadata(
            format,
            capabilities,
            ModelSourceKind::HuggingFace,
            Some(source_repo.to_string()),
        );
    }

    fn write_hf_mlx_model_capabilities(
        &self,
        capabilities: Vec<ModelCapability>,
        source_repo: &str,
        mlx_runtime_family: MlxRuntimeFamily,
    ) {
        let layout = StdRuntimeLayoutResolver
            .resolve(self.layout_input(LayoutResolveMode::Create))
            .expect("layout");
        let model_store = ModelStoreLayout::from_models_dir(layout.models_dir);
        let stored_capabilities = capabilities.clone();
        FileModelCatalogStore
            .save_model_metadata(
                &model_store,
                &ModelMetadata {
                    model_ref: self.model_ref.clone(),
                    short_ref: self.model_ref.short_ref().to_string(),
                    source_kind: ModelSourceKind::HuggingFace,
                    source_repo: Some(source_repo.to_string()),
                    source_revision: None,
                    source_path: Some("/tmp/model".to_string()),
                    primary_format: ModelFormat::Mlx,
                    detected_formats: vec![ModelFormat::Mlx],
                    mlx_runtime_family: Some(mlx_runtime_family),
                    model_capabilities: capabilities,
                    model_capability_source: default_model_capability_source(),
                    file_count: 1,
                    total_bytes: 1024,
                    imported_at: "2026-05-17T00:00:00Z".to_string(),
                },
            )
            .expect("save model");
        self.write_model_files(&model_store, ModelFormat::Mlx, &stored_capabilities);
    }

    fn write_model_metadata(
        &self,
        format: ModelFormat,
        capabilities: Vec<ModelCapability>,
        source_kind: ModelSourceKind,
        source_repo: Option<String>,
    ) {
        let layout = StdRuntimeLayoutResolver
            .resolve(self.layout_input(LayoutResolveMode::Create))
            .expect("layout");
        let model_store = ModelStoreLayout::from_models_dir(layout.models_dir);
        let stored_capabilities = capabilities.clone();
        FileModelCatalogStore
            .save_model_metadata(
                &model_store,
                &ModelMetadata {
                    model_ref: self.model_ref.clone(),
                    short_ref: self.model_ref.short_ref().to_string(),
                    source_kind,
                    source_repo,
                    source_revision: None,
                    source_path: Some("/tmp/model".to_string()),
                    primary_format: format,
                    detected_formats: vec![format],
                    mlx_runtime_family: None,
                    model_capabilities: capabilities,
                    model_capability_source: default_model_capability_source(),
                    file_count: 1,
                    total_bytes: 1024,
                    imported_at: "2026-05-17T00:00:00Z".to_string(),
                },
            )
            .expect("save model");
        self.write_model_files(&model_store, format, &stored_capabilities);
    }

    fn write_model_files(
        &self,
        model_store: &ModelStoreLayout,
        format: ModelFormat,
        capabilities: &[ModelCapability],
    ) {
        let catalog = FileModelCatalogStore;
        catalog
            .save_model_manifest(
                model_store,
                &self.model_ref,
                &ModelManifest {
                    files: vec![ModelManifestEntry {
                        relative_path: "source/config.json".to_string(),
                        size_bytes: 2,
                        sha256: "0".repeat(64),
                    }],
                },
            )
            .expect("save manifest");
        catalog
            .save_variant_metadata(
                model_store,
                &self.model_ref,
                &ModelVariantMetadata {
                    format,
                    status: ModelVariantStatus::Imported,
                    import_method: ModelImportMethod::Add,
                    relative_source_path: SOURCE_DIRNAME.to_string(),
                },
            )
            .expect("save variant");

        let source = model_store.variant_source_dir(&self.model_ref, format);
        std::fs::create_dir_all(&source).expect("create model source");
        match format {
            ModelFormat::Gguf => write_fixture_file(source.join("model.gguf"), "gguf"),
            ModelFormat::Diffusers => write_fixture_file(source.join("model_index.json"), "{}"),
            ModelFormat::Safetensors | ModelFormat::Mlx => {
                write_fixture_file(source.join("config.json"), "{}");
                if capabilities.iter().any(|capability| {
                    matches!(
                        capability,
                        ModelCapability::Chat
                            | ModelCapability::Embedding
                            | ModelCapability::Rerank
                    )
                }) {
                    write_fixture_file(source.join("tokenizer.json"), "{}");
                }
                if capabilities.iter().any(|capability| {
                    matches!(
                        capability,
                        ModelCapability::AudioTranscription
                            | ModelCapability::AudioSpeech
                            | ModelCapability::VisionChat
                            | ModelCapability::VideoUnderstanding
                    )
                }) {
                    write_fixture_file(source.join("processor_config.json"), "{}");
                }
            }
        }
    }

    fn write_capability_proof(
        &self,
        capability: ModelCapability,
        status: ModelCapabilityProofStatus,
        backend: &str,
        error: Option<&str>,
    ) {
        let (runtime_profile, runtime_profile_version) =
            if capability == ModelCapability::Chat && backend == "safetensors" {
                (Some("local-chat-transformers-peft".to_string()), Some(1))
            } else {
                (None, None)
            };
        let layout = StdRuntimeLayoutResolver
            .resolve(self.layout_input(LayoutResolveMode::Create))
            .expect("layout");
        let model_store = ModelStoreLayout::from_models_dir(layout.models_dir);
        FileModelCapabilityProofStore
            .save_capability_proof(
                &model_store,
                &ModelCapabilityProof {
                    model_ref: self.model_ref.clone(),
                    capability,
                    status,
                    source: ModelCapabilityProofSource::ServerStart,
                    primary_format: ModelFormat::Safetensors,
                    mlx_runtime_family: None,
                    backend: backend.to_string(),
                    runtime_version: None,
                    runtime_profile,
                    runtime_profile_version,
                    server_ref: Some("server-ref".to_string()),
                    checked_at: "2026-05-17T00:00:00Z".to_string(),
                    error: error.map(str::to_string),
                },
            )
            .expect("save proof");
    }
}

fn write_fixture_file(path: PathBuf, body: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create fixture parent");
    }
    std::fs::write(path, body).expect("write fixture file");
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
