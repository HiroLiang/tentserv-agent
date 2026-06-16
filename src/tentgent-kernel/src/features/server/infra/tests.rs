use std::{net::TcpListener, path::PathBuf};

use crate::features::auth::domain::{AuthSecretMaterial, AuthSecretSource, Provider};
use crate::features::model::domain::ModelRef;
use crate::features::server::domain::{
    CloudProvider, LaunchMode, ServerCapability, ServerRef, ServerRefSelector,
    ServerRuntimeBackend, ServerRuntimeKind, ServerRuntimeProfileSelection, ServerRuntimeTarget,
    ServerSpec, ServerStoreLayout, SERVER_REF_HEX_LENGTH,
};
use crate::features::server::ports::{
    ServerCatalogStore, ServerIdentityGenerator, ServerProcessProbe, ServerStoreLayoutInitializer,
};
use crate::foundation::error::KernelResult;

use super::identity::{
    local_capability_identity_json_for_test, local_identity_json_for_test,
    StdServerIdentityGenerator,
};
use super::runtime::{allocate_bind_port_for_spec, server_runtime_command_parts};
use super::{FileServerCatalogStore, StdServerStoreLayoutInitializer};

#[test]
fn server_layout_initializer_creates_servers_dir() {
    let root = unique_root("layout");
    let layout =
        ServerStoreLayout::from_home_and_servers_dir(root.join("home"), root.join("servers"));

    StdServerStoreLayoutInitializer
        .ensure_server_store_layout(&layout)
        .expect("ensure server layout");

    assert!(PathBuf::from(&layout.servers_dir).is_dir());
}

#[test]
fn local_identity_json_preserves_legacy_field_order() {
    let body = local_identity_json_for_test("abc123", "127.0.0.1", 8780, false, None);

    assert_eq!(
        body,
        r#"{"model_ref":"abc123","host":"127.0.0.1","port":8780,"lazy_load":false,"idle_seconds":null}"#
    );
}

#[test]
fn embedding_identity_json_includes_capability_without_changing_chat_shape() {
    let body = local_capability_identity_json_for_test(
        "abc123",
        "embedding",
        "127.0.0.1",
        8780,
        false,
        None,
    );

    assert_eq!(
        body,
        r#"{"model_ref":"abc123","capability":"embedding","host":"127.0.0.1","port":8780,"lazy_load":false,"idle_seconds":null}"#
    );
}

#[test]
fn local_chat_identity_includes_runtime_profile_when_selected() {
    let identity = StdServerIdentityGenerator;
    let model_ref = ModelRef::parse("a".repeat(64)).expect("model ref");
    let first = identity
        .server_ref_for_target(
            &ServerRuntimeTarget::LocalModel {
                model_ref: model_ref.clone(),
                backend: ServerRuntimeBackend::TransformersPeft,
                capability: ServerCapability::Chat,
                runtime_profile: Some(ServerRuntimeProfileSelection::new(
                    "local-chat-transformers-peft",
                    1,
                )),
            },
            "127.0.0.1",
            8780,
            false,
            false,
            None,
        )
        .expect("first ref");
    let second = identity
        .server_ref_for_target(
            &ServerRuntimeTarget::LocalModel {
                model_ref,
                backend: ServerRuntimeBackend::TransformersPeft,
                capability: ServerCapability::Chat,
                runtime_profile: Some(ServerRuntimeProfileSelection::new(
                    "local-chat-transformers-peft",
                    2,
                )),
            },
            "127.0.0.1",
            8780,
            false,
            false,
            None,
        )
        .expect("second ref");

    assert_ne!(first, second);
}

#[test]
fn local_embedding_identity_includes_runtime_profile_when_selected() {
    let identity = StdServerIdentityGenerator;
    let model_ref = ModelRef::parse("a".repeat(64)).expect("model ref");
    let first = identity
        .server_ref_for_target(
            &ServerRuntimeTarget::LocalModel {
                model_ref: model_ref.clone(),
                backend: ServerRuntimeBackend::TransformersPeft,
                capability: ServerCapability::Embedding,
                runtime_profile: Some(ServerRuntimeProfileSelection::new(
                    "local-embedding-transformers-peft",
                    1,
                )),
            },
            "127.0.0.1",
            8781,
            false,
            false,
            None,
        )
        .expect("first ref");
    let second = identity
        .server_ref_for_target(
            &ServerRuntimeTarget::LocalModel {
                model_ref,
                backend: ServerRuntimeBackend::TransformersPeft,
                capability: ServerCapability::Embedding,
                runtime_profile: Some(ServerRuntimeProfileSelection::new(
                    "local-embedding-transformers-peft",
                    2,
                )),
            },
            "127.0.0.1",
            8781,
            false,
            false,
            None,
        )
        .expect("second ref");

    assert_ne!(first, second);
}

#[test]
fn identity_generator_normalizes_anthropic_alias_inputs() {
    let identity = StdServerIdentityGenerator;
    let first = identity
        .server_ref_for_target(
            &ServerRuntimeTarget::CloudProvider {
                provider: CloudProvider::Anthropic,
                provider_model: "claude-3-5-sonnet-latest".to_string(),
                capability: ServerCapability::Chat,
            },
            "127.0.0.1",
            8780,
            false,
            false,
            None,
        )
        .expect("first ref");
    let second = identity
        .server_ref_for_target(
            &ServerRuntimeTarget::CloudProvider {
                provider: CloudProvider::Anthropic,
                provider_model: "claude-3-5-sonnet-latest".to_string(),
                capability: ServerCapability::Chat,
            },
            "127.0.0.1",
            8780,
            false,
            false,
            None,
        )
        .expect("second ref");

    assert_eq!(first, second);
}

#[test]
fn cloud_identity_separates_non_chat_capabilities() {
    let identity = StdServerIdentityGenerator;
    let chat = identity
        .server_ref_for_target(
            &ServerRuntimeTarget::CloudProvider {
                provider: CloudProvider::OpenAI,
                provider_model: "gpt-4.1-mini".to_string(),
                capability: ServerCapability::Chat,
            },
            "127.0.0.1",
            8780,
            false,
            false,
            None,
        )
        .expect("chat ref");
    let embedding = identity
        .server_ref_for_target(
            &ServerRuntimeTarget::CloudProvider {
                provider: CloudProvider::OpenAI,
                provider_model: "gpt-4.1-mini".to_string(),
                capability: ServerCapability::Embedding,
            },
            "127.0.0.1",
            8780,
            false,
            false,
            None,
        )
        .expect("embedding ref");

    assert_ne!(chat, embedding);
}

#[test]
fn file_catalog_stores_specs_and_process_metadata() {
    let root = unique_root("catalog");
    let layout =
        ServerStoreLayout::from_home_and_servers_dir(root.join("home"), root.join("servers"));
    StdServerStoreLayoutInitializer
        .ensure_server_store_layout(&layout)
        .expect("ensure server layout");
    let catalog = FileServerCatalogStore::new(StaticProcessProbe { running: true });
    let server_ref = ServerRef::parse("a".repeat(SERVER_REF_HEX_LENGTH)).expect("server ref");
    let model_ref = ModelRef::parse("b".repeat(64)).expect("model ref");
    let spec = ServerSpec {
        short_ref: server_ref.short_ref().to_string(),
        server_ref: server_ref.clone(),
        runtime_kind: super::super::domain::ServerRuntimeKind::Local,
        capability: ServerCapability::Chat,
        model_ref: Some(model_ref),
        provider: None,
        provider_model: None,
        runtime_profile: None,
        host: "127.0.0.1".to_string(),
        port: 8780,
        port_auto: false,
        lazy_load: false,
        idle_seconds: None,
        created_at: "2026-05-17T00:00:00Z".to_string(),
    };

    catalog
        .save_server_spec(&layout, &spec)
        .expect("save server spec");
    let inspection = catalog
        .record_process_start(
            &layout,
            &server_ref,
            42,
            8780,
            LaunchMode::Background,
            "2026-05-17T00:00:01Z".to_string(),
        )
        .expect("record process");
    assert!(inspection.running);
    let process = inspection.process.expect("process");
    assert_eq!(process.pid, 42);
    assert_eq!(process.bound_port, Some(8780));

    let listed = catalog.list_servers(&layout).expect("list servers");
    assert_eq!(listed.len(), 1);
    assert!(listed[0].running);

    let selector = ServerRefSelector::parse(server_ref.short_ref()).expect("selector");
    let stale_catalog = FileServerCatalogStore::new(StaticProcessProbe { running: false });
    let stale = stale_catalog
        .inspect_server(&layout, &selector)
        .expect("inspect stale");
    assert!(!stale.running);
    assert!(stale.process.is_none());
    assert!(!stale.process_path.exists());
}

#[test]
fn local_runtime_args_use_rust_proxy_shape() {
    let server_ref = ServerRef::parse("c".repeat(SERVER_REF_HEX_LENGTH)).expect("server ref");
    let model_ref = ModelRef::parse("d".repeat(64)).expect("model ref");
    let spec = ServerSpec {
        short_ref: server_ref.short_ref().to_string(),
        server_ref,
        runtime_kind: ServerRuntimeKind::Local,
        capability: ServerCapability::Chat,
        model_ref: Some(model_ref),
        provider: None,
        provider_model: None,
        runtime_profile: Some(ServerRuntimeProfileSelection::new(
            "local-chat-transformers-peft",
            1,
        )),
        host: "127.0.0.1".to_string(),
        port: 8780,
        port_auto: false,
        lazy_load: true,
        idle_seconds: Some(30),
        created_at: "2026-05-17T00:00:00Z".to_string(),
    };

    let parts =
        server_runtime_command_parts(&spec, &PathBuf::from("/tmp/tentgent-home"), None, 8780)
            .expect("parts");

    assert_eq!(
        parts.args,
        vec![
            "__local-server-runtime",
            "--server-ref",
            spec.server_ref.as_str(),
            "--capability",
            "chat",
            "--host",
            "127.0.0.1",
            "--port",
            "8780",
            "--home",
            "/tmp/tentgent-home",
            "--model-ref",
            spec.model_ref.as_ref().expect("model ref").as_str(),
            "--runtime-profile",
            "local-chat-transformers-peft-v1",
            "--lazy-load",
            "--idle-seconds",
            "30"
        ]
    );
    assert!(parts.env.is_empty());
    assert_eq!(parts.env_remove, vec!["TENTGENT_DAEMON_TOKEN".to_string()]);
}

#[test]
fn local_embedding_runtime_args_include_runtime_profile() {
    let server_ref = ServerRef::parse("e".repeat(SERVER_REF_HEX_LENGTH)).expect("server ref");
    let model_ref = ModelRef::parse("f".repeat(64)).expect("model ref");
    let spec = ServerSpec {
        short_ref: server_ref.short_ref().to_string(),
        server_ref,
        runtime_kind: ServerRuntimeKind::Local,
        capability: ServerCapability::Embedding,
        model_ref: Some(model_ref),
        provider: None,
        provider_model: None,
        runtime_profile: Some(ServerRuntimeProfileSelection::new(
            "local-embedding-transformers-peft",
            1,
        )),
        host: "127.0.0.1".to_string(),
        port: 8781,
        port_auto: false,
        lazy_load: false,
        idle_seconds: None,
        created_at: "2026-05-17T00:00:00Z".to_string(),
    };

    let parts =
        server_runtime_command_parts(&spec, &PathBuf::from("/tmp/tentgent-home"), None, 8781)
            .expect("parts");

    assert!(parts
        .args
        .windows(2)
        .any(|pair| pair == ["--runtime-profile", "local-embedding-transformers-peft-v1"]));
}

#[test]
fn local_rerank_runtime_args_are_supported() {
    let server_ref = ServerRef::parse("e".repeat(SERVER_REF_HEX_LENGTH)).expect("server ref");
    let model_ref = ModelRef::parse("f".repeat(64)).expect("model ref");
    let spec = ServerSpec {
        short_ref: server_ref.short_ref().to_string(),
        server_ref,
        runtime_kind: ServerRuntimeKind::Local,
        capability: ServerCapability::Rerank,
        model_ref: Some(model_ref),
        provider: None,
        provider_model: None,
        runtime_profile: None,
        host: "127.0.0.1".to_string(),
        port: 8782,
        port_auto: false,
        lazy_load: false,
        idle_seconds: None,
        created_at: "2026-05-17T00:00:00Z".to_string(),
    };

    let parts =
        server_runtime_command_parts(&spec, &PathBuf::from("/tmp/tentgent-home"), None, 8782)
            .expect("parts");

    assert!(parts
        .args
        .windows(2)
        .any(|pair| pair == ["--capability", "rerank"]));
}

#[test]
fn cloud_runtime_args_include_provider_auth_env() {
    let server_ref = ServerRef::parse("e".repeat(SERVER_REF_HEX_LENGTH)).expect("server ref");
    let spec = ServerSpec {
        short_ref: server_ref.short_ref().to_string(),
        server_ref,
        runtime_kind: ServerRuntimeKind::Cloud,
        capability: ServerCapability::Chat,
        model_ref: None,
        provider: Some(CloudProvider::OpenAI),
        provider_model: Some("gpt-4.1-mini".to_string()),
        runtime_profile: None,
        host: "127.0.0.1".to_string(),
        port: 8781,
        port_auto: false,
        lazy_load: false,
        idle_seconds: None,
        created_at: "2026-05-17T00:00:00Z".to_string(),
    };
    let auth = AuthSecretMaterial::new(Provider::OpenAI, AuthSecretSource::Env, "secret");

    let parts = server_runtime_command_parts(
        &spec,
        &PathBuf::from("/tmp/tentgent-home"),
        Some(&auth),
        8781,
    )
    .expect("parts");

    assert_eq!(
        parts.env,
        vec![("OPENAI_API_KEY".to_string(), "secret".to_string())]
    );
    assert_eq!(parts.env_remove, vec!["TENTGENT_DAEMON_TOKEN".to_string()]);
    assert!(parts.args.ends_with(&[
        "--provider".to_string(),
        "openai".to_string(),
        "--provider-model".to_string(),
        "gpt-4.1-mini".to_string(),
    ]));
}

#[test]
fn auto_port_allocator_scans_forward_from_requested_port() {
    let listener = busy_listener_below_scan_ceiling();
    let start = listener.local_addr().expect("listener addr").port();
    let spec = local_chat_spec_for_port(start, true);

    let selected = allocate_bind_port_for_spec(&spec).expect("auto port");

    assert!(selected > start);
}

#[test]
fn explicit_port_allocator_rejects_busy_port() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind busy port");
    let port = listener.local_addr().expect("listener addr").port();
    let spec = local_chat_spec_for_port(port, false);

    let err = allocate_bind_port_for_spec(&spec).expect_err("explicit port should fail");

    assert!(err.to_string().contains("is not available"));
}

struct StaticProcessProbe {
    running: bool,
}

impl ServerProcessProbe for StaticProcessProbe {
    fn is_process_running(&self, _pid: u32) -> KernelResult<bool> {
        Ok(self.running)
    }
}

fn unique_root(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "tentgent-kernel-server-infra-{label}-{}-{nanos}",
        std::process::id()
    ))
}

fn local_chat_spec_for_port(port: u16, port_auto: bool) -> ServerSpec {
    let server_ref = ServerRef::parse("c".repeat(SERVER_REF_HEX_LENGTH)).expect("server ref");
    let model_ref = ModelRef::parse("d".repeat(64)).expect("model ref");
    ServerSpec {
        short_ref: server_ref.short_ref().to_string(),
        server_ref,
        runtime_kind: ServerRuntimeKind::Local,
        capability: ServerCapability::Chat,
        model_ref: Some(model_ref),
        provider: None,
        provider_model: None,
        runtime_profile: Some(ServerRuntimeProfileSelection::new(
            "local-chat-transformers-peft",
            1,
        )),
        host: "127.0.0.1".to_string(),
        port,
        port_auto,
        lazy_load: false,
        idle_seconds: None,
        created_at: "2026-05-17T00:00:00Z".to_string(),
    }
}

fn busy_listener_below_scan_ceiling() -> TcpListener {
    for _ in 0..16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind busy port");
        let port = listener.local_addr().expect("listener addr").port();
        if port < u16::MAX - 100 {
            return listener;
        }
    }
    panic!("could not allocate an ephemeral port safely below the scan ceiling");
}
