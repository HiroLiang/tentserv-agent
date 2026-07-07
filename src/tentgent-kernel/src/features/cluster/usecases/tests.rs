use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::features::cluster::domain::{ClusterRef, ClusterRouteKey};
use crate::features::cluster::infra::{FileClusterCatalogStore, StdClusterStoreLayoutInitializer};
use crate::features::cluster::usecases::{
    ClusterApplyFileRequest, ClusterInspectRequest, ClusterSpecUseCase, StdClusterUseCase,
};
use crate::features::model::infra::FileModelCatalogStore;
use crate::foundation::layout::{LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver};

#[test]
fn cluster_ref_rejects_path_like_values() {
    assert!(ClusterRef::parse("../secret").is_err());
    assert!(ClusterRef::parse(".hidden").is_err());
    assert!(ClusterRef::parse("Local").is_err());
    assert!(ClusterRef::parse(" local-assistant").is_err());
    assert!(ClusterRef::parse("local-assistant ").is_err());
    assert!(ClusterRef::parse("local assistant").is_err());
    assert!(ClusterRef::parse("local-assistant").is_ok());
}

#[test]
fn route_key_limits_first_cluster_mvp_routes() {
    assert_eq!(
        ClusterRouteKey::parse("audio-transcription").expect("route"),
        ClusterRouteKey::AudioTranscription
    );
    assert!(ClusterRouteKey::parse(" chat").is_err());
    assert!(ClusterRouteKey::parse("chat ").is_err());
    assert!(ClusterRouteKey::parse("image-generation").is_err());
}

#[test]
fn apply_cluster_file_reads_toml_validates_and_stores_definition() {
    let root = unique_path("cluster-apply-file");
    let source_dir = root.join("source");
    let home = root.join("home");
    fs::create_dir_all(&source_dir).expect("source dir");
    let source_path = source_dir.join("cluster.toml");
    fs::write(
        &source_path,
        r#"
schema_version = 1
cluster_ref = "local-assistant"

[routes.chat]
kind = "provider"
provider = "openai"
provider_model = "gpt-test"
"#,
    )
    .expect("cluster source");

    let layout_resolver = StdRuntimeLayoutResolver;
    let layout_initializer = StdClusterStoreLayoutInitializer;
    let catalog = FileClusterCatalogStore;
    let model_catalog = FileModelCatalogStore;
    let usecase = StdClusterUseCase::new(
        &layout_resolver,
        &layout_initializer,
        &catalog,
        &model_catalog,
    );

    let applied = usecase
        .apply_cluster_file(ClusterApplyFileRequest {
            layout: layout_input(&home, LayoutResolveMode::Create),
            source_path,
            force_unsafe_source: false,
        })
        .expect("apply cluster file");

    assert_eq!(
        applied.inspection.definition.cluster_ref.to_string(),
        "local-assistant"
    );
    assert_eq!(applied.inspection.definition.routes.len(), 1);
    assert!(applied.inspection.definition_path.exists());

    let stored =
        fs::read_to_string(&applied.inspection.definition_path).expect("stored cluster definition");
    assert!(stored.contains(r#"cluster_ref = "local-assistant""#));
    assert!(stored.contains(r#"provider_model = "gpt-test""#));

    let inspected = usecase
        .inspect_cluster(ClusterInspectRequest {
            layout: layout_input(&home, LayoutResolveMode::ReadOnly),
            cluster_ref: ClusterRef::parse("local-assistant").expect("cluster ref"),
        })
        .expect("inspect stored cluster");
    assert_eq!(
        inspected.inspection.definition,
        applied.inspection.definition
    );

    let _ = fs::remove_dir_all(root);
}

fn layout_input(home: &std::path::Path, mode: LayoutResolveMode) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode,
        home_dir: Some(home.to_path_buf()),
        data_root_dir: None,
    }
}

fn unique_path(label: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("tentgent-{label}-{nanos}"))
}
