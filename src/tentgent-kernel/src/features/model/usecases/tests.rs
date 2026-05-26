use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::features::auth::domain::{AuthEnvLoadPolicy, Provider};
use crate::features::auth::usecases::{
    AuthSecretResolution, AuthSecretResolutionRequest, AuthSecretResolverUseCase,
};
use crate::features::model::domain::{
    default_model_capabilities, default_model_capability_source, HfModelMetadata,
    HfModelPullProgress, MlxRuntimeFamily, ModelCapability, ModelCapabilityProof,
    ModelCapabilityProofSource, ModelCapabilityProofStatus, ModelCapabilitySource, ModelFormat,
    ModelImportOutcome, ModelInspection, ModelMetadata, ModelRef, ModelRefSelector,
    ModelRemovalOutcome, ModelSourceKind, ModelStoreLayout, ModelSummary,
};
use crate::features::model::infra::{
    FileModelCapabilityProofStore, FileModelCatalogStore, FileModelContentStore,
    FileModelServerReferenceProbe, FileModelSourceIndexStore, StdModelIdentityGenerator,
    StdModelManifestBuilder, StdModelSourceStager, StdModelStoreLayoutInitializer,
};
use crate::features::model::ports::{
    HfModelSnapshot, HfModelSnapshotFetcher, HfModelSnapshotRequest, ModelCatalogStore, ModelClock,
};
use crate::features::runtime::domain::{
    PythonRuntimeLayout, PythonRuntimeResolutionInput, PythonRuntimeSource,
};
use crate::features::runtime::ports::PythonRuntimeResolver;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{
    LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput, RuntimeLayoutResolver,
};

use super::port::{
    ModelCapabilityMutation, ModelCapabilityProofListRequest, ModelCapabilityProofRecordRequest,
    ModelCapabilityProofUseCase, ModelCapabilityUpdateRequest, ModelCapabilityUpdateUseCase,
    ModelCapabilityVerifyRequest, ModelCatalogReadUseCase, ModelHfPullRequest, ModelHfPullUseCase,
    ModelInspectRequest, ModelListRequest, ModelLocalImportRequest, ModelLocalImportUseCase,
    ModelRemoveRequest, ModelRemoveUseCase,
};
use super::{
    StdModelCapabilityProofUseCase, StdModelCapabilityUpdateUseCase, StdModelCatalogReadUseCase,
    StdModelHfPullUseCase, StdModelLocalImportUseCase, StdModelRemoveUseCase,
};

#[test]
fn model_usecase_ports_cover_catalog_import_pull_and_remove_workflows() {
    let usecases = FakeModelUseCases;
    let layout = layout_input("/tmp/tentgent-model-usecases");
    let selector = ModelRefSelector::parse(model_ref().short_ref()).expect("selector");

    let listed = usecases
        .list_models(ModelListRequest {
            layout: layout.clone(),
        })
        .expect("list models");
    assert_eq!(listed.models.len(), 1);
    assert_eq!(listed.store.models_dir, listed.layout.models_dir);

    let inspected = usecases
        .inspect_model(ModelInspectRequest {
            layout: layout.clone(),
            selector: selector.clone(),
        })
        .expect("inspect model");
    assert_eq!(inspected.model.metadata.model_ref, model_ref());

    let imported = usecases
        .import_local_model(ModelLocalImportRequest {
            layout: layout.clone(),
            source_path: PathBuf::from("/tmp/source-model"),
            capability: None,
        })
        .expect("import local model");
    assert!(!imported.outcome.deduplicated);

    let mut progress_events = Vec::new();
    let pulled = usecases
        .pull_hf_model(
            ModelHfPullRequest {
                layout: layout.clone(),
                runtime: PythonRuntimeResolutionInput {
                    project_dir: Some(PathBuf::from("/tmp/python-project")),
                    python_env_dir: Some(PathBuf::from("/tmp/python-env")),
                },
                repo_id: "org/model".to_string(),
                revision: Some("main".to_string()),
                capability: None,
                auth: AuthSecretResolutionRequest::for_secret_use(
                    Provider::HuggingFace,
                    AuthEnvLoadPolicy::ProcessOnly,
                ),
            },
            &mut |event| progress_events.push(event),
        )
        .expect("pull hf model");
    assert_eq!(
        pulled.runtime.project_dir,
        PathBuf::from("/tmp/python-project")
    );
    assert_eq!(progress_events.len(), 1);

    let removed = usecases
        .remove_model(ModelRemoveRequest { layout, selector })
        .expect("remove model");
    assert_eq!(removed.outcome.metadata.model_ref, model_ref());

    let updated = usecases
        .update_model_capability(ModelCapabilityUpdateRequest {
            layout: layout_input("/tmp/tentgent-model-usecases"),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
            mutation: ModelCapabilityMutation::Set(vec![ModelCapability::Embedding]),
        })
        .expect("update capability");
    assert_eq!(updated.model.metadata.model_ref, model_ref());

    let proofs = usecases
        .list_model_capability_proofs(ModelCapabilityProofListRequest {
            layout: layout_input("/tmp/tentgent-model-usecases"),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
        })
        .expect("list proofs");
    assert_eq!(proofs.proofs.len(), 1);

    let verified = usecases
        .verify_model_capability(ModelCapabilityVerifyRequest {
            layout: layout_input("/tmp/tentgent-model-usecases"),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
            capability: ModelCapability::Chat,
        })
        .expect("verify capability");
    assert_eq!(verified.proof.status, ModelCapabilityProofStatus::Verified);

    let recorded = usecases
        .record_model_capability_proof(ModelCapabilityProofRecordRequest {
            layout: layout_input("/tmp/tentgent-model-usecases"),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
            capability: ModelCapability::Chat,
            status: ModelCapabilityProofStatus::Failed,
            source: ModelCapabilityProofSource::ServerStart,
            server_ref: Some("server-ref".to_string()),
            error: Some("boom".to_string()),
        })
        .expect("record proof");
    assert_eq!(
        recorded.proof.source,
        ModelCapabilityProofSource::ServerStart
    );
}

#[test]
fn standard_model_usecases_import_list_inspect_and_remove_local_model() {
    let home = unique_path("model-local-usecase");
    let source_dir = home.join("source");
    fs::create_dir_all(&source_dir).expect("source dir");
    fs::write(source_dir.join("model.gguf"), b"model").expect("source model");

    let layout_resolver = FakeLayoutResolver;
    let initializer = StdModelStoreLayoutInitializer;
    let stager = StdModelSourceStager;
    let manifest_builder = StdModelManifestBuilder;
    let identity = StdModelIdentityGenerator;
    let catalog = FileModelCatalogStore;
    let indexes = FileModelSourceIndexStore;
    let content = FileModelContentStore;
    let server_refs = FileModelServerReferenceProbe;

    let importer = StdModelLocalImportUseCase::new(
        &layout_resolver,
        &initializer,
        &stager,
        &manifest_builder,
        &identity,
        &catalog,
        &indexes,
        &content,
    );
    let imported = importer
        .import_local_model(ModelLocalImportRequest {
            layout: layout_input(home.to_str().expect("home path")),
            source_path: source_dir.clone(),
            capability: None,
        })
        .expect("import local model");
    assert!(!imported.outcome.deduplicated);
    assert_eq!(
        imported.outcome.metadata.source_kind,
        ModelSourceKind::Local
    );
    assert_eq!(
        imported.outcome.metadata.model_capabilities,
        vec![ModelCapability::Chat]
    );
    assert_eq!(
        imported.outcome.metadata.model_capability_source,
        ModelCapabilitySource::DefaultChat
    );
    assert!(imported.outcome.store_path.is_dir());

    let reader = StdModelCatalogReadUseCase::new(&layout_resolver, &catalog);
    let listed = reader
        .list_models(ModelListRequest {
            layout: layout_input(home.to_str().expect("home path")),
        })
        .expect("list models");
    assert_eq!(listed.models.len(), 1);

    let selector =
        ModelRefSelector::parse(imported.outcome.metadata.short_ref.as_str()).expect("selector");
    let inspected = reader
        .inspect_model(ModelInspectRequest {
            layout: layout_input(home.to_str().expect("home path")),
            selector: selector.clone(),
        })
        .expect("inspect model");
    assert_eq!(
        inspected.model.metadata.model_ref,
        imported.outcome.metadata.model_ref
    );

    let remover =
        StdModelRemoveUseCase::new(&layout_resolver, &catalog, &indexes, &content, &server_refs);
    let removed = remover
        .remove_model(ModelRemoveRequest {
            layout: layout_input(home.to_str().expect("home path")),
            selector,
        })
        .expect("remove model");
    assert_eq!(
        removed.outcome.metadata.model_ref,
        imported.outcome.metadata.model_ref
    );
    assert!(!removed.outcome.store_path.exists());
}

#[test]
fn standard_model_usecase_imports_local_model_with_explicit_capability_and_updates_dedup() {
    let home = unique_path("model-local-capability-usecase");
    let source_dir = home.join("source");
    fs::create_dir_all(&source_dir).expect("source dir");
    fs::write(source_dir.join("model.gguf"), b"model").expect("source model");

    let layout_resolver = FakeLayoutResolver;
    let initializer = StdModelStoreLayoutInitializer;
    let stager = StdModelSourceStager;
    let manifest_builder = StdModelManifestBuilder;
    let identity = StdModelIdentityGenerator;
    let catalog = FileModelCatalogStore;
    let indexes = FileModelSourceIndexStore;
    let content = FileModelContentStore;

    let importer = StdModelLocalImportUseCase::new(
        &layout_resolver,
        &initializer,
        &stager,
        &manifest_builder,
        &identity,
        &catalog,
        &indexes,
        &content,
    );
    let first = importer
        .import_local_model(ModelLocalImportRequest {
            layout: layout_input(home.to_str().expect("home path")),
            source_path: source_dir.clone(),
            capability: None,
        })
        .expect("import local model");
    assert_eq!(
        first.outcome.metadata.model_capabilities,
        vec![ModelCapability::Chat]
    );

    let second = importer
        .import_local_model(ModelLocalImportRequest {
            layout: layout_input(home.to_str().expect("home path")),
            source_path: source_dir,
            capability: Some(ModelCapability::Embedding),
        })
        .expect("deduplicate local model");
    assert!(second.outcome.deduplicated);
    assert_eq!(
        second.outcome.metadata.model_capabilities,
        vec![ModelCapability::Embedding]
    );
    assert_eq!(
        second.outcome.metadata.model_capability_source,
        ModelCapabilitySource::ExplicitUser
    );

    let reader = StdModelCatalogReadUseCase::new(&layout_resolver, &catalog);
    let inspected = reader
        .inspect_model(ModelInspectRequest {
            layout: layout_input(home.to_str().expect("home path")),
            selector: ModelRefSelector::parse(second.outcome.metadata.short_ref.as_str())
                .expect("selector"),
        })
        .expect("inspect model");
    assert_eq!(
        inspected.model.metadata.model_capabilities,
        vec![ModelCapability::Embedding]
    );
    assert_eq!(
        inspected.model.metadata.model_capability_source,
        ModelCapabilitySource::ExplicitUser
    );

    let _ = fs::remove_dir_all(home);
}

#[test]
fn standard_hf_pull_usecase_resolves_runtime_auth_fetches_snapshot_and_imports() {
    let home = unique_path("model-hf-usecase");
    let layout_resolver = FakeLayoutResolver;
    let runtime_resolver = FakeRuntimeResolver;
    let auth_resolver = FakeAuthResolver;
    let initializer = StdModelStoreLayoutInitializer;
    let stager = StdModelSourceStager;
    let snapshot_fetcher = FakeSnapshotFetcher::default();
    let manifest_builder = StdModelManifestBuilder;
    let identity = StdModelIdentityGenerator;
    let catalog = FileModelCatalogStore;
    let indexes = FileModelSourceIndexStore;
    let content = FileModelContentStore;
    let usecase = StdModelHfPullUseCase::new(
        &layout_resolver,
        &runtime_resolver,
        &auth_resolver,
        &initializer,
        &stager,
        &snapshot_fetcher,
        &manifest_builder,
        &identity,
        &catalog,
        &indexes,
        &content,
    );

    let mut progress = Vec::new();
    let result = usecase
        .pull_hf_model(
            ModelHfPullRequest {
                layout: layout_input(home.to_str().expect("home path")),
                runtime: PythonRuntimeResolutionInput {
                    project_dir: Some(home.join("python")),
                    python_env_dir: Some(home.join("python-env")),
                },
                repo_id: "org/model".to_string(),
                revision: Some("main".to_string()),
                capability: Some(ModelCapability::Rerank),
                auth: AuthSecretResolutionRequest::for_secret_use(
                    Provider::HuggingFace,
                    AuthEnvLoadPolicy::ProcessOnly,
                ),
            },
            &mut |event| progress.push(event),
        )
        .expect("pull hf model");

    assert_eq!(progress.len(), 1);
    assert_eq!(result.runtime.project_dir, home.join("python"));
    assert_eq!(
        result.outcome.metadata.source_kind,
        ModelSourceKind::HuggingFace
    );
    assert_eq!(
        result.outcome.metadata.source_repo.as_deref(),
        Some("org/model")
    );
    assert_eq!(
        result.outcome.metadata.source_revision.as_deref(),
        Some("resolved-sha")
    );
    assert_eq!(
        result.outcome.metadata.model_capabilities,
        vec![ModelCapability::Rerank]
    );
    assert_eq!(
        result.outcome.metadata.model_capability_source,
        ModelCapabilitySource::ExplicitUser
    );
    assert!(result.outcome.store_path.is_dir());
}

#[test]
fn standard_hf_pull_detects_embedding_rerank_chat_and_ambiguous_metadata() {
    let embedding_home = unique_path("model-hf-detect-embedding");
    let embedding = pull_hf_model_for_test(
        &embedding_home,
        Some(HfModelMetadata {
            pipeline_tag: Some("sentence-similarity".to_string()),
            tags: vec!["sentence-transformers".to_string()],
            ..HfModelMetadata::default()
        }),
        None,
    );
    assert_eq!(
        embedding.outcome.metadata.model_capabilities,
        vec![ModelCapability::Embedding]
    );
    assert_eq!(
        embedding.outcome.metadata.model_capability_source,
        ModelCapabilitySource::HuggingFaceMetadata
    );
    let _ = fs::remove_dir_all(embedding_home);

    let rerank_home = unique_path("model-hf-detect-rerank");
    let rerank = pull_hf_model_for_test(
        &rerank_home,
        Some(HfModelMetadata {
            pipeline_tag: Some("text-ranking".to_string()),
            tags: vec!["cross-encoder".to_string()],
            ..HfModelMetadata::default()
        }),
        None,
    );
    assert_eq!(
        rerank.outcome.metadata.model_capabilities,
        vec![ModelCapability::Rerank]
    );
    assert_eq!(
        rerank.outcome.metadata.model_capability_source,
        ModelCapabilitySource::HuggingFaceMetadata
    );
    let _ = fs::remove_dir_all(rerank_home);

    let chat_home = unique_path("model-hf-detect-chat");
    let chat = pull_hf_model_for_test(
        &chat_home,
        Some(HfModelMetadata {
            tokenizer_chat_template: true,
            ..HfModelMetadata::default()
        }),
        None,
    );
    assert_eq!(
        chat.outcome.metadata.model_capabilities,
        vec![ModelCapability::Chat]
    );
    assert_eq!(
        chat.outcome.metadata.model_capability_source,
        ModelCapabilitySource::HuggingFaceMetadata
    );
    let _ = fs::remove_dir_all(chat_home);

    let ambiguous_home = unique_path("model-hf-detect-ambiguous");
    let ambiguous = pull_hf_model_for_test(
        &ambiguous_home,
        Some(HfModelMetadata {
            pipeline_tag: Some("text-generation".to_string()),
            tags: vec!["sentence-transformers".to_string()],
            ..HfModelMetadata::default()
        }),
        None,
    );
    assert_eq!(
        ambiguous.outcome.metadata.model_capabilities,
        vec![ModelCapability::Chat]
    );
    assert_eq!(
        ambiguous.outcome.metadata.model_capability_source,
        ModelCapabilitySource::DefaultChat
    );
    assert!(ambiguous.outcome.metadata.capability_warning().is_some());
    let _ = fs::remove_dir_all(ambiguous_home);
}

#[test]
fn standard_hf_pull_explicit_capability_overrides_detected_metadata() {
    let home = unique_path("model-hf-explicit-over-detected");
    let result = pull_hf_model_for_test(
        &home,
        Some(HfModelMetadata {
            pipeline_tag: Some("sentence-similarity".to_string()),
            ..HfModelMetadata::default()
        }),
        Some(ModelCapability::Rerank),
    );

    assert_eq!(
        result.outcome.metadata.model_capabilities,
        vec![ModelCapability::Rerank]
    );
    assert_eq!(
        result.outcome.metadata.model_capability_source,
        ModelCapabilitySource::ExplicitUser
    );

    let _ = fs::remove_dir_all(home);
}

#[test]
fn standard_hf_pull_dedup_detection_updates_only_auto_owned_metadata() {
    let home = unique_path("model-hf-dedup-detection");
    import_local_for_test(&home, None, b"hf model");

    let detected = pull_hf_model_for_test(
        &home,
        Some(HfModelMetadata {
            pipeline_tag: Some("sentence-similarity".to_string()),
            ..HfModelMetadata::default()
        }),
        None,
    );
    assert!(detected.outcome.deduplicated);
    assert_eq!(
        detected.outcome.metadata.model_capabilities,
        vec![ModelCapability::Embedding]
    );
    assert_eq!(
        detected.outcome.metadata.model_capability_source,
        ModelCapabilitySource::HuggingFaceMetadata
    );

    let _ = fs::remove_dir_all(home);
}

#[test]
fn standard_hf_pull_dedup_detection_preserves_user_owned_metadata() {
    let explicit_home = unique_path("model-hf-dedup-explicit-preserved");
    import_local_for_test(&explicit_home, Some(ModelCapability::Rerank), b"hf model");
    let explicit = pull_hf_model_for_test(
        &explicit_home,
        Some(HfModelMetadata {
            pipeline_tag: Some("sentence-similarity".to_string()),
            ..HfModelMetadata::default()
        }),
        None,
    );
    assert!(explicit.outcome.deduplicated);
    assert_eq!(
        explicit.outcome.metadata.model_capabilities,
        vec![ModelCapability::Rerank]
    );
    assert_eq!(
        explicit.outcome.metadata.model_capability_source,
        ModelCapabilitySource::ExplicitUser
    );
    let _ = fs::remove_dir_all(explicit_home);

    let manual_home = unique_path("model-hf-dedup-manual-preserved");
    let imported = import_local_for_test(&manual_home, None, b"hf model");
    update_capability_for_test(
        &manual_home,
        imported.outcome.metadata.short_ref.as_str(),
        ModelCapability::Rerank,
    );
    let manual = pull_hf_model_for_test(
        &manual_home,
        Some(HfModelMetadata {
            pipeline_tag: Some("sentence-similarity".to_string()),
            ..HfModelMetadata::default()
        }),
        None,
    );
    assert!(manual.outcome.deduplicated);
    assert_eq!(
        manual.outcome.metadata.model_capabilities,
        vec![ModelCapability::Rerank]
    );
    assert_eq!(
        manual.outcome.metadata.model_capability_source,
        ModelCapabilitySource::ManualUpdate
    );
    let _ = fs::remove_dir_all(manual_home);
}

#[test]
fn standard_model_capability_update_rewrites_metadata_without_changing_ref() {
    let home = unique_path("model-capability-update");
    let imported = import_local_for_test(&home, None, b"model");
    let original_ref = imported.outcome.metadata.model_ref.clone();

    let updated = update_capability_for_test(
        &home,
        imported.outcome.metadata.short_ref.as_str(),
        ModelCapability::Embedding,
    );

    assert_eq!(updated.model.metadata.model_ref, original_ref);
    assert_eq!(
        updated.model.metadata.model_capabilities,
        vec![ModelCapability::Embedding]
    );
    assert_eq!(
        updated.model.metadata.model_capability_source,
        ModelCapabilitySource::ManualUpdate
    );
    assert_eq!(updated.previous_capabilities, vec![ModelCapability::Chat]);
    assert_eq!(updated.added_capabilities, vec![ModelCapability::Embedding]);
    assert_eq!(updated.removed_capabilities, vec![ModelCapability::Chat]);

    let _ = fs::remove_dir_all(home);
}

#[test]
fn standard_model_capability_update_adds_removes_and_canonicalizes_metadata() {
    let home = unique_path("model-capability-update-add-remove");
    let imported = import_local_for_test(&home, None, b"model");

    let updated = update_capabilities_for_test(
        &home,
        imported.outcome.metadata.short_ref.as_str(),
        ModelCapabilityMutation::AddRemove {
            add: vec![
                ModelCapability::VisionChat,
                ModelCapability::Embedding,
                ModelCapability::VisionChat,
            ],
            remove: vec![ModelCapability::Chat],
        },
    );

    assert_eq!(
        updated.model.metadata.model_capabilities,
        vec![ModelCapability::Embedding, ModelCapability::VisionChat]
    );
    assert_eq!(
        updated.added_capabilities,
        vec![ModelCapability::Embedding, ModelCapability::VisionChat]
    );
    assert_eq!(updated.removed_capabilities, vec![ModelCapability::Chat]);
    assert_eq!(
        updated.model.metadata.model_capability_source,
        ModelCapabilitySource::ManualUpdate
    );

    let _ = fs::remove_dir_all(home);
}

#[test]
fn standard_model_capability_update_rejects_empty_final_capability_set() {
    let home = unique_path("model-capability-update-empty");
    let imported = import_local_for_test(&home, None, b"model");

    let err = try_update_capabilities_for_test(
        &home,
        imported.outcome.metadata.short_ref.as_str(),
        ModelCapabilityMutation::AddRemove {
            add: vec![],
            remove: vec![ModelCapability::Chat],
        },
    )
    .expect_err("empty capability set should fail");

    assert!(err
        .to_string()
        .contains("model capability set must not be empty"));

    let _ = fs::remove_dir_all(home);
}

#[test]
fn standard_model_capability_update_recalculates_mlx_runtime_family() {
    let home = unique_path("model-capability-update-mlx-family");
    let imported = import_local_for_test(&home, None, b"model");
    let mut metadata = imported.outcome.metadata.clone();
    metadata.primary_format = ModelFormat::Mlx;
    metadata.detected_formats = vec![ModelFormat::Mlx];
    metadata.mlx_runtime_family = None;
    FileModelCatalogStore
        .save_model_metadata(&imported.store, &metadata)
        .expect("save mlx metadata");

    let updated = update_capability_for_test(
        &home,
        imported.outcome.metadata.short_ref.as_str(),
        ModelCapability::VisionChat,
    );

    assert_eq!(
        updated.model.metadata.mlx_runtime_family,
        Some(MlxRuntimeFamily::Vlm)
    );

    let _ = fs::remove_dir_all(home);
}

#[test]
fn standard_model_capability_proof_usecase_writes_and_lists_latest_proofs() {
    let home = unique_path("model-capability-proof-usecase");
    let imported = import_local_for_test(&home, Some(ModelCapability::VisionChat), b"model");
    let layout_resolver = FakeLayoutResolver;
    let catalog = FileModelCatalogStore;
    let proofs = FileModelCapabilityProofStore;
    let clock = StaticModelClock;
    let usecase = StdModelCapabilityProofUseCase::new(&layout_resolver, &catalog, &proofs, &clock);
    let selector =
        ModelRefSelector::parse(imported.outcome.metadata.short_ref.as_str()).expect("selector");

    let verified = usecase
        .verify_model_capability(ModelCapabilityVerifyRequest {
            layout: layout_input(home.to_str().expect("home path")),
            selector: selector.clone(),
            capability: ModelCapability::VisionChat,
        })
        .expect("verify capability");
    assert_eq!(verified.proof.status, ModelCapabilityProofStatus::Verified);
    assert_eq!(
        verified.proof.source,
        ModelCapabilityProofSource::ManualProbe
    );
    assert_eq!(verified.proof.checked_at, STATIC_TIME);

    let recorded = usecase
        .record_model_capability_proof(ModelCapabilityProofRecordRequest {
            layout: layout_input(home.to_str().expect("home path")),
            selector: selector.clone(),
            capability: ModelCapability::VisionChat,
            status: ModelCapabilityProofStatus::Failed,
            source: ModelCapabilityProofSource::ServerStart,
            server_ref: Some("server-ref".to_string()),
            error: Some("runtime failed".to_string()),
        })
        .expect("record proof");
    assert_eq!(recorded.proof.status, ModelCapabilityProofStatus::Failed);
    assert_eq!(recorded.proof.server_ref.as_deref(), Some("server-ref"));

    let listed = usecase
        .list_model_capability_proofs(ModelCapabilityProofListRequest {
            layout: layout_input(home.to_str().expect("home path")),
            selector,
        })
        .expect("list proofs");
    assert_eq!(listed.proofs.len(), 1);
    assert_eq!(listed.proofs[0].status, ModelCapabilityProofStatus::Failed);
    assert_eq!(listed.proofs[0].error.as_deref(), Some("runtime failed"));

    let _ = fs::remove_dir_all(home);
}

#[test]
fn standard_model_capability_verify_records_failed_proof_for_undeclared_capability() {
    let home = unique_path("model-capability-proof-failed");
    let imported = import_local_for_test(&home, Some(ModelCapability::Chat), b"model");
    let layout_resolver = FakeLayoutResolver;
    let catalog = FileModelCatalogStore;
    let proofs = FileModelCapabilityProofStore;
    let clock = StaticModelClock;
    let usecase = StdModelCapabilityProofUseCase::new(&layout_resolver, &catalog, &proofs, &clock);

    let result = usecase
        .verify_model_capability(ModelCapabilityVerifyRequest {
            layout: layout_input(home.to_str().expect("home path")),
            selector: ModelRefSelector::parse(imported.outcome.metadata.short_ref.as_str())
                .expect("selector"),
            capability: ModelCapability::Embedding,
        })
        .expect("verify capability");

    assert_eq!(result.proof.status, ModelCapabilityProofStatus::Failed);
    assert!(result
        .proof
        .error
        .as_deref()
        .expect("error")
        .contains("does not advertise capability"));

    let _ = fs::remove_dir_all(home);
}

fn pull_hf_model_for_test(
    home: &Path,
    metadata: Option<HfModelMetadata>,
    capability: Option<ModelCapability>,
) -> super::port::ModelHfPullResult {
    let layout_resolver = FakeLayoutResolver;
    let runtime_resolver = FakeRuntimeResolver;
    let auth_resolver = FakeAuthResolver;
    let initializer = StdModelStoreLayoutInitializer;
    let stager = StdModelSourceStager;
    let snapshot_fetcher = FakeSnapshotFetcher { metadata };
    let manifest_builder = StdModelManifestBuilder;
    let identity = StdModelIdentityGenerator;
    let catalog = FileModelCatalogStore;
    let indexes = FileModelSourceIndexStore;
    let content = FileModelContentStore;
    let usecase = StdModelHfPullUseCase::new(
        &layout_resolver,
        &runtime_resolver,
        &auth_resolver,
        &initializer,
        &stager,
        &snapshot_fetcher,
        &manifest_builder,
        &identity,
        &catalog,
        &indexes,
        &content,
    );

    usecase
        .pull_hf_model(
            ModelHfPullRequest {
                layout: layout_input(home.to_str().expect("home path")),
                runtime: PythonRuntimeResolutionInput {
                    project_dir: Some(home.join("python")),
                    python_env_dir: Some(home.join("python-env")),
                },
                repo_id: "org/model".to_string(),
                revision: Some("main".to_string()),
                capability,
                auth: AuthSecretResolutionRequest::for_secret_use(
                    Provider::HuggingFace,
                    AuthEnvLoadPolicy::ProcessOnly,
                ),
            },
            &mut |_| {},
        )
        .expect("pull hf model")
}

fn import_local_for_test(
    home: &Path,
    capability: Option<ModelCapability>,
    model_bytes: &[u8],
) -> super::port::ModelLocalImportResult {
    let source_dir = home.join("source");
    fs::create_dir_all(&source_dir).expect("source dir");
    fs::write(source_dir.join("model.gguf"), model_bytes).expect("source model");

    let layout_resolver = FakeLayoutResolver;
    let initializer = StdModelStoreLayoutInitializer;
    let stager = StdModelSourceStager;
    let manifest_builder = StdModelManifestBuilder;
    let identity = StdModelIdentityGenerator;
    let catalog = FileModelCatalogStore;
    let indexes = FileModelSourceIndexStore;
    let content = FileModelContentStore;
    let importer = StdModelLocalImportUseCase::new(
        &layout_resolver,
        &initializer,
        &stager,
        &manifest_builder,
        &identity,
        &catalog,
        &indexes,
        &content,
    );

    importer
        .import_local_model(ModelLocalImportRequest {
            layout: layout_input(home.to_str().expect("home path")),
            source_path: source_dir,
            capability,
        })
        .expect("import local model")
}

fn update_capability_for_test(
    home: &Path,
    reference: &str,
    capability: ModelCapability,
) -> super::port::ModelCapabilityUpdateResult {
    let layout_resolver = FakeLayoutResolver;
    let catalog = FileModelCatalogStore;
    let updater = StdModelCapabilityUpdateUseCase::new(&layout_resolver, &catalog);

    updater
        .update_model_capability(ModelCapabilityUpdateRequest {
            layout: layout_input(home.to_str().expect("home path")),
            selector: ModelRefSelector::parse(reference).expect("selector"),
            mutation: ModelCapabilityMutation::Set(vec![capability]),
        })
        .expect("update capability")
}

fn update_capabilities_for_test(
    home: &Path,
    reference: &str,
    mutation: ModelCapabilityMutation,
) -> super::port::ModelCapabilityUpdateResult {
    try_update_capabilities_for_test(home, reference, mutation).expect("update capability")
}

fn try_update_capabilities_for_test(
    home: &Path,
    reference: &str,
    mutation: ModelCapabilityMutation,
) -> KernelResult<super::port::ModelCapabilityUpdateResult> {
    let layout_resolver = FakeLayoutResolver;
    let catalog = FileModelCatalogStore;
    let updater = StdModelCapabilityUpdateUseCase::new(&layout_resolver, &catalog);

    updater.update_model_capability(ModelCapabilityUpdateRequest {
        layout: layout_input(home.to_str().expect("home path")),
        selector: ModelRefSelector::parse(reference).expect("selector"),
        mutation,
    })
}

struct FakeModelUseCases;

impl ModelCatalogReadUseCase for FakeModelUseCases {
    fn list_models(&self, request: ModelListRequest) -> KernelResult<super::port::ModelListResult> {
        let layout = runtime_layout(request.layout);
        let store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        Ok(super::port::ModelListResult {
            layout,
            store: store.clone(),
            models: vec![ModelSummary {
                metadata: metadata_fixture(),
                store_path: store.model_dir(&model_ref()),
            }],
        })
    }

    fn inspect_model(
        &self,
        request: ModelInspectRequest,
    ) -> KernelResult<super::port::ModelInspectResult> {
        let layout = runtime_layout(request.layout);
        let store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        let metadata = metadata_fixture();
        Ok(super::port::ModelInspectResult {
            layout,
            store: store.clone(),
            model: ModelInspection {
                store_path: store.model_dir(&metadata.model_ref),
                manifest_path: store.manifest_path(&metadata.model_ref),
                variant_source_path: store
                    .variant_source_dir(&metadata.model_ref, metadata.primary_format),
                metadata,
            },
        })
    }
}

impl ModelLocalImportUseCase for FakeModelUseCases {
    fn import_local_model(
        &self,
        request: ModelLocalImportRequest,
    ) -> KernelResult<super::port::ModelLocalImportResult> {
        let layout = runtime_layout(request.layout);
        let store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        Ok(super::port::ModelLocalImportResult {
            layout,
            store: store.clone(),
            outcome: import_outcome(&store),
        })
    }
}

impl ModelHfPullUseCase for FakeModelUseCases {
    fn pull_hf_model(
        &self,
        request: ModelHfPullRequest,
        progress: &mut dyn FnMut(HfModelPullProgress),
    ) -> KernelResult<super::port::ModelHfPullResult> {
        progress(HfModelPullProgress {
            description: request.repo_id,
            position: 1,
            total: Some(1),
            unit: "files".to_string(),
            finished: true,
        });

        let layout = runtime_layout(request.layout);
        let store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        let runtime = PythonRuntimeLayout {
            project_dir: request.runtime.project_dir.unwrap_or_default(),
            env_dir: request.runtime.python_env_dir.unwrap_or_default(),
            source: PythonRuntimeSource::EnvironmentOverride,
        };
        Ok(super::port::ModelHfPullResult {
            layout,
            store: store.clone(),
            runtime,
            outcome: import_outcome(&store),
        })
    }
}

impl ModelRemoveUseCase for FakeModelUseCases {
    fn remove_model(
        &self,
        request: ModelRemoveRequest,
    ) -> KernelResult<super::port::ModelRemoveResult> {
        let layout = runtime_layout(request.layout);
        let store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        Ok(super::port::ModelRemoveResult {
            layout,
            store: store.clone(),
            outcome: ModelRemovalOutcome {
                metadata: metadata_fixture(),
                store_path: store.model_dir(&model_ref()),
                removed_index_paths: vec![store.local_index_path(&model_ref())],
            },
        })
    }
}

impl ModelCapabilityUpdateUseCase for FakeModelUseCases {
    fn update_model_capability(
        &self,
        request: ModelCapabilityUpdateRequest,
    ) -> KernelResult<super::port::ModelCapabilityUpdateResult> {
        let layout = runtime_layout(request.layout);
        let store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        let metadata = metadata_fixture();
        Ok(super::port::ModelCapabilityUpdateResult {
            layout,
            store: store.clone(),
            model: ModelInspection {
                store_path: store.model_dir(&metadata.model_ref),
                manifest_path: store.manifest_path(&metadata.model_ref),
                variant_source_path: store
                    .variant_source_dir(&metadata.model_ref, metadata.primary_format),
                metadata,
            },
            previous_capabilities: vec![ModelCapability::Chat],
            added_capabilities: vec![ModelCapability::Embedding],
            removed_capabilities: vec![ModelCapability::Chat],
        })
    }
}

impl ModelCapabilityProofUseCase for FakeModelUseCases {
    fn list_model_capability_proofs(
        &self,
        request: ModelCapabilityProofListRequest,
    ) -> KernelResult<super::port::ModelCapabilityProofListResult> {
        let layout = runtime_layout(request.layout);
        let store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        Ok(super::port::ModelCapabilityProofListResult {
            layout,
            store: store.clone(),
            model: inspection(&store),
            proofs: vec![proof_fixture()],
        })
    }

    fn verify_model_capability(
        &self,
        request: ModelCapabilityVerifyRequest,
    ) -> KernelResult<super::port::ModelCapabilityProofRecordResult> {
        let layout = runtime_layout(request.layout);
        let store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        Ok(super::port::ModelCapabilityProofRecordResult {
            layout,
            store: store.clone(),
            model: inspection(&store),
            proof: proof_fixture(),
        })
    }

    fn record_model_capability_proof(
        &self,
        request: ModelCapabilityProofRecordRequest,
    ) -> KernelResult<super::port::ModelCapabilityProofRecordResult> {
        let layout = runtime_layout(request.layout);
        let store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        let mut proof = proof_fixture();
        proof.status = request.status;
        proof.source = request.source;
        proof.server_ref = request.server_ref;
        proof.error = request.error;
        Ok(super::port::ModelCapabilityProofRecordResult {
            layout,
            store: store.clone(),
            model: inspection(&store),
            proof,
        })
    }
}

fn import_outcome(store: &ModelStoreLayout) -> ModelImportOutcome {
    ModelImportOutcome {
        metadata: metadata_fixture(),
        store_path: store.model_dir(&model_ref()),
        source_index_path: store.local_index_path(&model_ref()),
        deduplicated: false,
    }
}

fn inspection(store: &ModelStoreLayout) -> ModelInspection {
    let metadata = metadata_fixture();
    ModelInspection {
        store_path: store.model_dir(&metadata.model_ref),
        manifest_path: store.manifest_path(&metadata.model_ref),
        variant_source_path: store.variant_source_dir(&metadata.model_ref, metadata.primary_format),
        metadata,
    }
}

fn proof_fixture() -> ModelCapabilityProof {
    ModelCapabilityProof {
        model_ref: model_ref(),
        capability: ModelCapability::Chat,
        status: ModelCapabilityProofStatus::Verified,
        source: ModelCapabilityProofSource::ManualProbe,
        primary_format: ModelFormat::Gguf,
        mlx_runtime_family: None,
        backend: "gguf".to_string(),
        runtime_version: None,
        server_ref: None,
        checked_at: STATIC_TIME.to_string(),
        error: None,
    }
}

fn metadata_fixture() -> ModelMetadata {
    let model_ref = model_ref();
    ModelMetadata {
        short_ref: model_ref.short_ref().to_string(),
        model_ref,
        source_kind: ModelSourceKind::Local,
        source_repo: None,
        source_revision: None,
        source_path: Some("/tmp/source-model".to_string()),
        primary_format: ModelFormat::Gguf,
        detected_formats: vec![ModelFormat::Gguf],
        mlx_runtime_family: None,
        model_capabilities: default_model_capabilities(),
        model_capability_source: default_model_capability_source(),
        file_count: 1,
        total_bytes: 42,
        imported_at: "2026-05-17T00:00:00Z".to_string(),
    }
}

fn model_ref() -> ModelRef {
    ModelRef::parse("a".repeat(64)).expect("model ref")
}

fn layout_input(home: &str) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode: LayoutResolveMode::ReadOnly,
        home_dir: Some(PathBuf::from(home)),
        data_root_dir: None,
    }
}

fn runtime_layout(input: RuntimeLayoutInput) -> RuntimeLayout {
    let home = input.home_dir.expect("test home");
    RuntimeLayout {
        home_dir: home.clone(),
        data_root_dir: home.clone(),
        config_path: home.join("config.toml"),
        models_dir: home.join("models"),
        adapters_dir: home.join("adapters"),
        datasets_dir: home.join("datasets"),
        sessions_dir: home.join("sessions"),
        servers_dir: home.join("servers"),
        train_dir: home.join("train"),
        cache_dir: home.join("cache"),
        runtime_dir: home.join("runtime"),
        logs_dir: home.join("logs"),
        locks_dir: home.join("locks"),
        python_env_dir: home.join("runtime/python-env"),
        bootstrap_dir: home.join("runtime/bootstrap"),
        bootstrap_uv_dir: home.join("runtime/bootstrap/uv"),
        bootstrap_uv_cache_dir: home.join("runtime/bootstrap/uv-cache"),
        capabilities_path: home.join("runtime/capabilities.toml"),
        auth_metadata_path: home.join("runtime/auth.toml"),
    }
}

fn unique_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("tentgent-{label}-{nanos}"))
}

const STATIC_TIME: &str = "2026-05-17T00:00:00Z";

struct StaticModelClock;

impl ModelClock for StaticModelClock {
    fn now_rfc3339(&self) -> KernelResult<String> {
        Ok(STATIC_TIME.to_string())
    }
}

struct FakeLayoutResolver;

impl RuntimeLayoutResolver for FakeLayoutResolver {
    fn resolve(&self, input: RuntimeLayoutInput) -> KernelResult<RuntimeLayout> {
        Ok(runtime_layout(input))
    }
}

struct FakeRuntimeResolver;

impl PythonRuntimeResolver for FakeRuntimeResolver {
    fn resolve_python_runtime(
        &self,
        layout: &RuntimeLayout,
        input: PythonRuntimeResolutionInput,
    ) -> KernelResult<PythonRuntimeLayout> {
        Ok(PythonRuntimeLayout {
            project_dir: input
                .project_dir
                .unwrap_or_else(|| layout.home_dir.join("python")),
            env_dir: input
                .python_env_dir
                .unwrap_or_else(|| layout.python_env_dir.clone()),
            source: PythonRuntimeSource::EnvironmentOverride,
        })
    }
}

struct FakeAuthResolver;

impl AuthSecretResolverUseCase for FakeAuthResolver {
    fn resolve_secret(
        &self,
        request: AuthSecretResolutionRequest,
    ) -> KernelResult<AuthSecretResolution> {
        Ok(AuthSecretResolution {
            provider: request.provider,
            secret: None,
            keychain_read_attempted: false,
        })
    }
}

#[derive(Default)]
struct FakeSnapshotFetcher {
    metadata: Option<HfModelMetadata>,
}

impl HfModelSnapshotFetcher for FakeSnapshotFetcher {
    fn fetch_hf_snapshot(
        &self,
        request: HfModelSnapshotRequest,
        progress: &mut dyn FnMut(HfModelPullProgress),
    ) -> KernelResult<HfModelSnapshot> {
        fs::create_dir_all(&request.destination_dir).expect("destination dir");
        fs::write(request.destination_dir.join("model.gguf"), b"hf model").expect("snapshot model");
        progress(HfModelPullProgress {
            description: request.repo_id.clone(),
            position: 1,
            total: Some(1),
            unit: "files".to_string(),
            finished: true,
        });

        Ok(HfModelSnapshot {
            repo_id: request.repo_id,
            resolved_revision: "resolved-sha".to_string(),
            local_dir: request.destination_dir,
            metadata: self.metadata.clone(),
        })
    }
}
