use super::super::{
    state::ClusterServerState,
    tests::{definition, state_for_definition},
};
use super::*;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use std::fs;
use tentgent_kernel::features::{
    cluster::domain::{ClusterRouteKey, ClusterRouteTarget},
    model::domain::ModelRef,
    server::options::LoadMode,
};
use tower::ServiceExt;

fn fixture(label: &str) -> (ClusterServerState, std::path::PathBuf) {
    state_for_definition(
        label,
        definition(
            &ClusterRef::parse("snapshots").unwrap(),
            &ModelRef::parse("a".repeat(64)).unwrap(),
            false,
        ),
    )
}
fn write(cache: &ClusterDefinitionCache, model: char) {
    let mut definition = cache.committed().unwrap().definition;
    definition.routes.insert(
        ClusterRouteKey::Chat,
        ClusterRouteTarget::LocalModel {
            model_ref: ModelRef::parse(model.to_string().repeat(64)).unwrap(),
            runtime_profile: None,
        },
    );
    FileClusterCatalogStore
        .save_cluster(&cache.store, &definition)
        .unwrap();
}

#[test]
fn candidate_reads_never_publish_and_promotion_is_explicit() {
    let (state, home) = fixture("candidate-read");
    let cache = state.definitions;
    let old = cache.committed().unwrap();
    write(&cache, 'b');
    let candidate = cache.candidate(true).unwrap().unwrap();
    for _ in 0..3 {
        assert_eq!(cache.committed().unwrap().hash, old.hash);
    }
    assert_ne!(candidate.snapshot().hash, old.hash);
    assert!(cache.promote(&candidate).unwrap());
    let new = cache.committed().unwrap();
    assert_eq!(new.hash, candidate.snapshot().hash);
    assert_eq!(new.revision, old.revision + 1);
    assert!(!cache.promote(&candidate).unwrap());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn superseded_and_aba_candidates_cannot_promote() {
    let (state, home) = fixture("candidate-aba");
    let cache = state.definitions;
    write(&cache, 'b');
    let b = cache.candidate(true).unwrap().unwrap();
    write(&cache, 'c');
    assert!(!cache.promote(&b).unwrap());
    let c = cache.candidate(true).unwrap().unwrap();
    assert!(cache.promote(&c).unwrap());
    write(&cache, 'a');
    assert!(cache
        .promote(&cache.candidate(true).unwrap().unwrap())
        .unwrap());
    write(&cache, 'b');
    assert!(
        !cache.promote(&b).unwrap(),
        "same base hash is not same revision"
    );
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn failed_commit_and_invalid_disk_keep_committed_snapshot() {
    let (state, home) = fixture("candidate-failure");
    let cache = state.definitions;
    let old = cache.committed().unwrap();
    write(&cache, 'b');
    let b = cache.candidate(true).unwrap().unwrap();
    assert!(cache
        .promote_with(&b, |_| Err(ClusterServerError::route_unavailable(
            "not prepared".into()
        )))
        .is_err());
    assert_eq!(cache.committed().unwrap().hash, old.hash);
    fs::write(
        cache.store.cluster_definition_path("snapshots"),
        "invalid = [",
    )
    .unwrap();
    assert!(cache.candidate(true).is_err());
    assert!(cache.promote(&b).is_err());
    assert_eq!(cache.committed().unwrap().hash, old.hash);
    assert!(
        cache.current().is_err(),
        "lazy reads preserve invalid-reload failure"
    );
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn candidate_validation_preserves_block_policy() {
    let (state, home) = fixture("candidate-block");
    let cache = state.definitions;
    let mut definition = cache.committed().unwrap().definition;
    definition.route_update_policy =
        tentgent_kernel::features::cluster::domain::ClusterRouteUpdatePolicy::Block;
    FileClusterCatalogStore
        .save_cluster(&cache.store, &definition)
        .unwrap();
    cache.refresh(true).unwrap();
    write(&cache, 'b');
    assert!(cache
        .candidate(true)
        .unwrap_err()
        .to_string()
        .contains("block"));
    fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn eager_health_reads_committed_even_when_candidate_is_invalid() {
    let (mut state, home) = fixture("candidate-health");
    state.config.load_mode = LoadMode::Eager;
    let old = state.definition_snapshot().unwrap();
    write(&state.definitions, 'b');
    assert_eq!(state.definition_snapshot().unwrap().hash, old.hash);
    fs::write(
        state.definitions.store.cluster_definition_path("snapshots"),
        "invalid = [",
    )
    .unwrap();
    let response = super::super::router::cluster_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 10240).await.unwrap()).unwrap();
    assert_eq!(body["definition_hash"], old.hash);
    state.config.load_mode = LoadMode::Lazy;
    assert!(state.definition_snapshot().is_err());
    fs::remove_dir_all(home).unwrap();
}
