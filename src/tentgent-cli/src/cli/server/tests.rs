use super::*;
use tentgent_kernel::features::model::domain::ModelRef;
use tentgent_kernel::features::server::domain::ServerRef;

#[test]
fn server_list_target_label_shortens_local_model_refs() {
    let spec = local_server_spec();

    assert_eq!(server_list_target_label(&spec), "abcdefabcdef");
}

#[test]
fn server_list_target_label_keeps_cloud_provider_model_names() {
    let spec = cloud_server_spec();

    assert_eq!(server_list_target_label(&spec), "gpt-4o-mini");
}

#[test]
fn server_list_target_label_uses_cluster_ref_for_cluster_targets() {
    let spec = cluster_server_spec();

    assert_eq!(server_list_target_label(&spec), "local-assistant");
}

#[test]
fn runtime_idle_alias_accepts_matching_values_and_rejects_conflicts() {
    assert_eq!(resolve_runtime_idle_alias(None, None).unwrap(), None);
    assert_eq!(
        resolve_runtime_idle_alias(Some(30), None).unwrap(),
        Some(30)
    );
    assert_eq!(
        resolve_runtime_idle_alias(None, Some(30)).unwrap(),
        Some(30)
    );
    assert_eq!(
        resolve_runtime_idle_alias(Some(30), Some(30)).unwrap(),
        Some(30)
    );

    let error = resolve_runtime_idle_alias(Some(30), Some(31)).expect_err("conflict");
    assert!(error.to_string().contains("must match"));
}

fn local_server_spec() -> ServerSpec {
    let model_ref =
        ModelRef::parse("abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd")
            .expect("model ref");

    ServerSpec {
        server_ref: server_ref(),
        short_ref: "0123456789ab".to_string(),
        runtime_kind: ServerRuntimeKind::Local,
        capability: Some(ServerCapability::Chat),
        model_ref: Some(model_ref),
        provider: None,
        provider_model: None,
        cluster_ref: None,
        runtime_profile: None,
        host: "127.0.0.1".to_string(),
        port: 8780,
        port_auto: false,
        lazy_load: false,
        idle_seconds: None,
        model_idle_seconds: None,
        created_at: "2026-06-15T00:00:00Z".to_string(),
    }
}

fn cloud_server_spec() -> ServerSpec {
    ServerSpec {
        server_ref: server_ref(),
        short_ref: "0123456789ab".to_string(),
        runtime_kind: ServerRuntimeKind::Cloud,
        capability: Some(ServerCapability::Chat),
        model_ref: None,
        provider: Some(CloudProvider::OpenAI),
        provider_model: Some("gpt-4o-mini".to_string()),
        cluster_ref: None,
        runtime_profile: None,
        host: "127.0.0.1".to_string(),
        port: 8780,
        port_auto: false,
        lazy_load: false,
        idle_seconds: None,
        model_idle_seconds: None,
        created_at: "2026-06-15T00:00:00Z".to_string(),
    }
}

fn cluster_server_spec() -> ServerSpec {
    ServerSpec {
        server_ref: server_ref(),
        short_ref: "0123456789ab".to_string(),
        runtime_kind: ServerRuntimeKind::Cluster,
        capability: None,
        model_ref: None,
        provider: None,
        provider_model: None,
        cluster_ref: Some(ClusterRef::parse("local-assistant").expect("cluster ref")),
        runtime_profile: None,
        host: "127.0.0.1".to_string(),
        port: 8780,
        port_auto: false,
        lazy_load: false,
        idle_seconds: None,
        model_idle_seconds: None,
        created_at: "2026-07-12T00:00:00Z".to_string(),
    }
}

fn server_ref() -> ServerRef {
    ServerRef::parse("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        .expect("server ref")
}
