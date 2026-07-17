use std::collections::BTreeMap;

use crate::features::cluster::domain::{
    ClusterDefinition, ClusterRef, ClusterRouteKey, ClusterRouteTarget, ClusterStoreLayout,
    CLUSTER_SCHEMA_VERSION,
};
use crate::features::cluster::infra::FileClusterCatalogStore;
use crate::features::cluster::ports::ClusterCatalogStore;
use crate::features::model::domain::ModelRef;

#[test]
fn cluster_store_roundtrips_definition() {
    let temp = std::env::temp_dir().join(format!(
        "tentgent-cluster-store-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&temp);
    let layout = ClusterStoreLayout::from_home_dir(&temp);
    let cluster_ref = ClusterRef::parse("local-assistant").expect("cluster ref");
    let model_ref = ModelRef::parse("a".repeat(64)).expect("model ref");
    let mut routes = BTreeMap::new();
    routes.insert(
        ClusterRouteKey::Chat,
        ClusterRouteTarget::LocalModel {
            model_ref,
            runtime_profile: None,
        },
    );
    let definition = ClusterDefinition {
        schema_version: CLUSTER_SCHEMA_VERSION,
        cluster_ref: cluster_ref.clone(),
        route_update_policy: Default::default(),
        routes,
    };

    let store = FileClusterCatalogStore;
    let inspection = store
        .save_cluster(&layout, &definition)
        .expect("save cluster");
    assert_eq!(inspection.definition, definition);
    assert!(inspection.definition_path.exists());

    let listed = store.list_clusters(&layout).expect("list clusters");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].cluster_ref, cluster_ref);

    let _ = std::fs::remove_dir_all(&temp);
}
