use super::*;
use crate::features::resource_coordination::{
    infra::FileResourceCoordinator, ResourceCoordinator, ResourceKey, ResourceKind,
    ResourceLockMode, ResourceLockRequest,
};

#[test]
fn deduplicated_import_cannot_change_metadata_during_a_proof_snapshot() {
    let home = unique_path("import-proof-coordination");
    let imported = import_local_for_test(&home, None, b"model");
    let permit = FileResourceCoordinator
        .acquire(
            &imported.layout,
            ResourceLockRequest::new(
                "fixture-proof-snapshot",
                vec![
                    (ResourceKey::maintenance(), ResourceLockMode::Shared),
                    (
                        ResourceKey::new(
                            ResourceKind::Model,
                            imported.outcome.metadata.model_ref.as_str(),
                        ),
                        ResourceLockMode::Shared,
                    ),
                ],
            ),
        )
        .unwrap()
        .unwrap();

    let error = reimport(&home).unwrap_err();
    assert!(matches!(
        error,
        crate::foundation::error::KernelError::ResourceCoordinationUnavailable(_)
    ));
    let unchanged = FileModelCatalogStore
        .load_model_metadata(&imported.store, &imported.outcome.metadata.model_ref)
        .unwrap();
    assert_eq!(unchanged, imported.outcome.metadata);

    drop(permit);
    let updated = reimport(&home).unwrap();
    assert!(updated.outcome.deduplicated);
    assert_eq!(
        updated.outcome.metadata.model_capabilities,
        vec![ModelCapability::Embedding]
    );
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn import_commit_respects_runtime_maintenance_exclusion() {
    let home = unique_path("import-maintenance-coordination");
    let imported = import_local_for_test(&home, None, b"model");
    let permit = FileResourceCoordinator
        .acquire(
            &imported.layout,
            ResourceLockRequest::new(
                "fixture-maintenance",
                vec![(ResourceKey::maintenance(), ResourceLockMode::Exclusive)],
            ),
        )
        .unwrap()
        .unwrap();
    assert!(matches!(
        reimport(&home),
        Err(crate::foundation::error::KernelError::ResourceCoordinationUnavailable(_))
    ));
    drop(permit);
    assert!(reimport(&home).unwrap().outcome.deduplicated);
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn deduplicated_import_rejects_metadata_redirect_without_mutating_other_model() {
    let home = unique_path("import-identity-corruption");
    let first = import_local_for_test(&home, None, b"model-a");
    let second = import_local_for_test(&home, None, b"model-b");
    assert_ne!(
        first.outcome.metadata.model_ref,
        second.outcome.metadata.model_ref
    );
    let first_path = first
        .store
        .model_metadata_path(&first.outcome.metadata.model_ref);
    let second_path = second
        .store
        .model_metadata_path(&second.outcome.metadata.model_ref);
    let unchanged_second = fs::read(&second_path).unwrap();
    // Simulate a misplaced, parseable metadata record without using the catalog
    // writer, which correctly chooses its destination from the record identity.
    let corrupt_first = toml::to_string_pretty(&second.outcome.metadata).unwrap();
    fs::write(&first_path, &corrupt_first).unwrap();
    fs::write(home.join("source/model.gguf"), b"model-a").unwrap();

    let second_permit = FileResourceCoordinator
        .acquire(
            &second.layout,
            ResourceLockRequest::new(
                "other-model-snapshot",
                vec![
                    (ResourceKey::maintenance(), ResourceLockMode::Shared),
                    (
                        ResourceKey::new(
                            ResourceKind::Model,
                            second.outcome.metadata.model_ref.as_str(),
                        ),
                        ResourceLockMode::Shared,
                    ),
                ],
            ),
        )
        .unwrap()
        .unwrap();
    let error = reimport(&home).unwrap_err();
    assert!(matches!(
        error,
        crate::foundation::error::KernelError::ModelStoreUnavailable(_)
    ));
    assert!(error.to_string().contains("does not match canonical model"));
    assert_eq!(fs::read(&second_path).unwrap(), unchanged_second);
    assert_eq!(fs::read_to_string(&first_path).unwrap(), corrupt_first);
    drop(second_permit);
    fs::remove_dir_all(home).unwrap();
}

fn reimport(home: &Path) -> KernelResult<super::super::port::ModelLocalImportResult> {
    StdModelLocalImportUseCase::new(
        &FakeLayoutResolver,
        &StdModelStoreLayoutInitializer,
        &StdModelSourceStager,
        &StdModelManifestBuilder,
        &StdModelIdentityGenerator,
        &FileModelCatalogStore,
        &FileModelSourceIndexStore,
        &FileModelContentStore,
    )
    .import_local_model(ModelLocalImportRequest {
        layout: layout_input(home.to_str().unwrap()),
        source_path: home.join("source"),
        capability: Some(ModelCapability::Embedding),
    })
}
