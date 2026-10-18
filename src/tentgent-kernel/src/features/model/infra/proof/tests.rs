use std::fs;

use crate::features::model::{
    domain::{ModelCapability, ModelCapabilityProofKey, MODEL_CAPABILITY_CANONICAL_ORDER},
    infra::FileModelCatalogStore,
    ports::{ModelCapabilityProofStore, ModelCatalogStore},
};
use crate::features::resource_coordination::{
    infra::FileResourceCoordinator, ResourceCoordinator, ResourceKey, ResourceKind,
    ResourceLockMode, ResourceLockRequest,
};
use crate::foundation::error::KernelError;

use super::FileModelCapabilityProofStore;

mod fixtures;
mod processes;

use fixtures::Fixture;

#[test]
fn borrowed_permit_requires_matching_root_keys_and_sufficient_modes() {
    let fixture = Fixture::new("borrowed");
    let proof = fixture.proof("same");
    let store = FileModelCapabilityProofStore;
    let shared = fixture.permit(ResourceLockMode::Shared);
    let context = fixture.context().with_permit(&shared);
    assert!(store
        .list_capability_proofs_for(&context, fixture.model_ref(), ModelCapability::Chat)
        .unwrap()
        .is_empty());
    assert!(matches!(
        store.save_capability_proof(&context, &proof),
        Err(KernelError::ResourceCoordinationUnavailable(_))
    ));
    assert!(matches!(
        store.list_capability_proofs(&context, fixture.model_ref()),
        Err(KernelError::ResourceCoordinationUnavailable(_))
    ));
    drop(shared);

    let other = Fixture::new("other-root");
    let wrong_root = other.permit(ResourceLockMode::Exclusive);
    assert!(matches!(
        store.save_capability_proof(&fixture.context().with_permit(&wrong_root), &proof),
        Err(KernelError::ResourceCoordinationUnavailable(_))
    ));
    drop(wrong_root);

    let model_only = FileResourceCoordinator
        .acquire(
            &fixture.runtime,
            ResourceLockRequest::new(
                "insufficient",
                vec![
                    (ResourceKey::maintenance(), ResourceLockMode::Shared),
                    (
                        ResourceKey::new(ResourceKind::Model, fixture.model_ref().as_str()),
                        ResourceLockMode::Exclusive,
                    ),
                ],
            ),
        )
        .unwrap()
        .unwrap();
    assert!(matches!(
        store.save_capability_proof(&fixture.context().with_permit(&model_only), &proof),
        Err(KernelError::ResourceCoordinationUnavailable(_))
    ));
    drop(model_only);

    let exclusive = fixture.permit(ResourceLockMode::Exclusive);
    let context = fixture.context().with_permit(&exclusive);
    store
        .save_capability_proof(&context, &proof)
        .expect("borrowed write must not reacquire its own lock");
    assert_eq!(
        store
            .list_capability_proofs_for(&context, fixture.model_ref(), ModelCapability::Chat)
            .unwrap(),
        vec![proof]
    );
    assert_eq!(
        store
            .remove_capability_proof(&context, fixture.model_ref(), ModelCapability::Chat)
            .unwrap(),
        1
    );
}

#[test]
fn model_wide_snapshot_borrows_all_capabilities_as_one_permit() {
    let fixture = Fixture::new("whole-snapshot");
    let mut locks = fixture.locks(ResourceLockMode::Shared);
    locks.extend(
        MODEL_CAPABILITY_CANONICAL_ORDER
            .into_iter()
            .map(|capability| {
                (
                    ResourceKey::new(
                        ResourceKind::ModelCapability,
                        format!("{}|{capability}", fixture.model_ref()),
                    ),
                    ResourceLockMode::Shared,
                )
            }),
    );
    let permit = FileResourceCoordinator
        .acquire(
            &fixture.runtime,
            ResourceLockRequest::new("all-capabilities", locks),
        )
        .unwrap()
        .unwrap();
    assert!(FileModelCapabilityProofStore
        .list_capability_proofs(&fixture.context().with_permit(&permit), fixture.model_ref())
        .unwrap()
        .is_empty());
}

#[test]
fn maintenance_exclusion_blocks_owned_proof_reads_until_the_holder_releases() {
    let fixture = Fixture::new("maintenance");
    let permit = FileResourceCoordinator
        .acquire(
            &fixture.runtime,
            ResourceLockRequest::new(
                "maintenance",
                vec![(ResourceKey::maintenance(), ResourceLockMode::Exclusive)],
            ),
        )
        .unwrap()
        .unwrap();
    let error = FileModelCapabilityProofStore
        .list_capability_proofs_for(
            &fixture.context(),
            fixture.model_ref(),
            ModelCapability::Chat,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        KernelError::ResourceCoordinationUnavailable(_)
    ));
    drop(permit);
    assert!(FileModelCapabilityProofStore
        .list_capability_proofs_for(
            &fixture.context(),
            fixture.model_ref(),
            ModelCapability::Chat
        )
        .unwrap()
        .is_empty());
}

#[test]
fn stale_observed_metadata_is_rejected_before_creating_proof_files() {
    let fixture = Fixture::new("stale-metadata");
    let mut changed = fixture.metadata.clone();
    changed.model_capabilities = vec![ModelCapability::Embedding];
    FileModelCatalogStore
        .save_model_metadata(&fixture.store, &changed)
        .unwrap();
    let context = fixture.context().with_expected_metadata(&fixture.metadata);
    let error = FileModelCapabilityProofStore
        .save_capability_proof(&context, &fixture.proof("same"))
        .unwrap_err();
    assert!(matches!(error, KernelError::ResourceStateUnstable { .. }));
    assert!(!fixture
        .store
        .capability_proof_path(fixture.model_ref(), ModelCapability::Chat)
        .exists());
    assert!(!fixture
        .store
        .support_proofs_capability_dir(fixture.model_ref(), ModelCapability::Chat)
        .exists());
}

#[test]
fn failed_primary_replacement_does_not_modify_the_existing_mirror() {
    let fixture = Fixture::new("primary-failure");
    let store = FileModelCapabilityProofStore;
    let original = fixture.proof("original");
    store
        .save_capability_proof(&fixture.context(), &original)
        .unwrap();
    let latest = fixture
        .store
        .capability_proof_path(fixture.model_ref(), ModelCapability::Chat);
    let old_mirror = fs::read(&latest).unwrap();
    let replacement = fixture.proof("different-key");
    let blocked = fixture
        .store
        .support_proof_path(&ModelCapabilityProofKey::from_proof(&replacement));
    fs::create_dir(&blocked).unwrap();
    let error = store
        .save_capability_proof(&fixture.context(), &replacement)
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("atomically replace model support proof"));
    assert_eq!(fs::read(&latest).unwrap(), old_mirror);
    assert_eq!(
        store
            .list_capability_proofs_for(
                &fixture.context(),
                fixture.model_ref(),
                ModelCapability::Chat
            )
            .unwrap(),
        vec![original]
    );
    assert!(fs::read_dir(blocked.parent().unwrap())
        .unwrap()
        .all(|entry| !entry
            .unwrap()
            .path()
            .extension()
            .is_some_and(|extension| extension == "tmp")));
}

#[test]
fn mirror_failure_reports_committed_primary_and_can_be_retried_idempotently() {
    let fixture = Fixture::new("mirror-failure");
    let store = FileModelCapabilityProofStore;
    let proof = fixture.proof("same");
    let latest = fixture
        .store
        .capability_proof_path(fixture.model_ref(), ModelCapability::Chat);
    fs::create_dir_all(&latest).unwrap();
    let error = store
        .save_capability_proof(&fixture.context(), &proof)
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("primary support proof is already committed"));
    assert_eq!(
        store
            .list_support_proofs(&fixture.context(), fixture.model_ref())
            .unwrap(),
        vec![proof.clone()]
    );
    assert!(store
        .list_capability_proofs_for(
            &fixture.context(),
            fixture.model_ref(),
            ModelCapability::Chat
        )
        .is_err());
    fs::remove_dir(&latest).unwrap();
    store
        .save_capability_proof(&fixture.context(), &proof)
        .unwrap();
    assert_eq!(
        store
            .list_capability_proofs_for(
                &fixture.context(),
                fixture.model_ref(),
                ModelCapability::Chat
            )
            .unwrap(),
        vec![proof]
    );
    assert_eq!(
        store
            .remove_capability_proof(
                &fixture.context(),
                fixture.model_ref(),
                ModelCapability::Chat
            )
            .unwrap(),
        1,
        "v1 primary and mirror count once"
    );
    assert_eq!(
        store
            .remove_capability_proof(
                &fixture.context(),
                fixture.model_ref(),
                ModelCapability::Chat
            )
            .unwrap(),
        0
    );
}
