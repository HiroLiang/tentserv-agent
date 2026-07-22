use super::*;

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
            runtime_profile: Some("local-chat-mlx".to_string()),
            runtime_profile_version: Some(1),
            error: Some("boom".to_string()),
        })
        .expect("record proof");
    assert_eq!(
        recorded.proof.source,
        ModelCapabilityProofSource::ServerStart
    );
    assert_eq!(
        recorded.proof.runtime_profile.as_deref(),
        Some("local-chat-mlx")
    );
    assert_eq!(recorded.proof.runtime_profile_version, Some(1));

    let cleared = usecases
        .clear_model_capability_proofs(ModelCapabilityProofClearRequest {
            layout: layout_input("/tmp/tentgent-model-usecases"),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
            capability: ModelCapability::Chat,
        })
        .expect("clear proofs");
    assert_eq!(cleared.capability, ModelCapability::Chat);
    assert_eq!(cleared.removed_proof_count, 1);
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
fn standard_model_remove_rejects_model_referenced_by_cluster_route() {
    let home = unique_path("model-remove-cluster-blocker");
    let imported = import_local_for_test(&home, None, b"model");
    write_cluster_definition(
        &home,
        "local-assistant",
        "chat",
        &imported.outcome.metadata.model_ref,
    );

    let err = try_remove_model_for_test(&home, imported.outcome.metadata.short_ref.as_str())
        .expect_err("cluster route should block model removal");

    let message = err.to_string();
    assert!(message.contains("cluster-route local-assistant:chat"));
    assert!(imported.outcome.store_path.exists());

    let _ = fs::remove_dir_all(home);
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
fn standard_model_capability_update_rejects_removing_cluster_route_capability() {
    let home = unique_path("model-capability-update-cluster-blocker");
    let imported = import_local_for_test(&home, None, b"model");
    write_cluster_definition(
        &home,
        "local-assistant",
        "chat",
        &imported.outcome.metadata.model_ref,
    );

    let err = try_update_capabilities_for_test(
        &home,
        imported.outcome.metadata.short_ref.as_str(),
        ModelCapabilityMutation::Set(vec![ModelCapability::Embedding]),
    )
    .expect_err("cluster route should block removing chat");

    let message = err.to_string();
    assert!(message.contains("cluster-route local-assistant:chat"));
    assert!(message.contains("model capability `chat`"));

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
            runtime_profile: Some("local-chat-mlx".to_string()),
            runtime_profile_version: Some(1),
            error: Some("runtime failed".to_string()),
        })
        .expect("record proof");
    assert_eq!(recorded.proof.status, ModelCapabilityProofStatus::Failed);
    assert_eq!(recorded.proof.server_ref.as_deref(), Some("server-ref"));
    assert_eq!(
        recorded.proof.runtime_profile.as_deref(),
        Some("local-chat-mlx")
    );
    assert_eq!(recorded.proof.runtime_profile_version, Some(1));

    let listed = usecase
        .list_model_capability_proofs(ModelCapabilityProofListRequest {
            layout: layout_input(home.to_str().expect("home path")),
            selector,
        })
        .expect("list proofs");
    assert_eq!(listed.proofs.len(), 2);
    assert!(listed
        .proofs
        .iter()
        .any(|proof| proof.status == ModelCapabilityProofStatus::Verified
            && proof.source == ModelCapabilityProofSource::ManualProbe));
    assert!(listed
        .proofs
        .iter()
        .any(|proof| proof.status == ModelCapabilityProofStatus::Failed
            && proof.source == ModelCapabilityProofSource::ServerStart
            && proof.runtime_profile.as_deref() == Some("local-chat-mlx")
            && proof.runtime_profile_version == Some(1)
            && proof.error.as_deref() == Some("runtime failed")));

    let _ = fs::remove_dir_all(home);
}

#[test]
fn standard_model_capability_proof_usecase_sanitizes_recorded_errors() {
    let home = unique_path("model-capability-proof-sanitized-error");
    let imported = import_local_for_test(&home, Some(ModelCapability::Chat), b"model");
    let layout_resolver = FakeLayoutResolver;
    let catalog = FileModelCatalogStore;
    let proofs = FileModelCapabilityProofStore;
    let clock = StaticModelClock;
    let usecase = StdModelCapabilityProofUseCase::new(&layout_resolver, &catalog, &proofs, &clock);
    let selector =
        ModelRefSelector::parse(imported.outcome.metadata.short_ref.as_str()).expect("selector");

    let recorded = usecase
        .record_model_capability_proof(ModelCapabilityProofRecordRequest {
            layout: layout_input(home.to_str().expect("home path")),
            selector,
            capability: ModelCapability::Chat,
            status: ModelCapabilityProofStatus::Failed,
            source: ModelCapabilityProofSource::ServerStart,
            server_ref: Some("server-ref".to_string()),
            runtime_profile: Some("local-chat-mlx".to_string()),
            runtime_profile_version: Some(1),
            error: Some(format!(
                "runtime failed\nOPENAI_API_KEY\n{}",
                "x".repeat(600)
            )),
        })
        .expect("record proof");

    let error = recorded.proof.error.as_deref().expect("recorded error");
    assert!(!error.contains("OPENAI_API_KEY"));
    assert!(error.contains("[redacted-env]"));
    assert!(!error.contains('\n'));
    assert!(error.chars().count() <= 503);

    let _ = fs::remove_dir_all(home);
}

#[test]
fn standard_model_capability_proof_usecase_clears_capability_proofs() {
    let home = unique_path("model-capability-proof-clear");
    let imported = import_local_for_test(&home, Some(ModelCapability::Chat), b"model");
    let layout_resolver = FakeLayoutResolver;
    let catalog = FileModelCatalogStore;
    let proofs = FileModelCapabilityProofStore;
    let clock = StaticModelClock;
    let usecase = StdModelCapabilityProofUseCase::new(&layout_resolver, &catalog, &proofs, &clock);
    let selector =
        ModelRefSelector::parse(imported.outcome.metadata.short_ref.as_str()).expect("selector");

    usecase
        .record_model_capability_proof(ModelCapabilityProofRecordRequest {
            layout: layout_input(home.to_str().expect("home path")),
            selector: selector.clone(),
            capability: ModelCapability::Chat,
            status: ModelCapabilityProofStatus::Failed,
            source: ModelCapabilityProofSource::ServerStart,
            server_ref: Some("server-ref".to_string()),
            runtime_profile: Some("local-chat-mlx".to_string()),
            runtime_profile_version: Some(1),
            error: Some("runtime failed".to_string()),
        })
        .expect("record failed proof");

    let cleared = usecase
        .clear_model_capability_proofs(ModelCapabilityProofClearRequest {
            layout: layout_input(home.to_str().expect("home path")),
            selector: selector.clone(),
            capability: ModelCapability::Chat,
        })
        .expect("clear proofs");
    assert_eq!(cleared.capability, ModelCapability::Chat);
    assert_eq!(cleared.removed_proof_count, 1);

    let listed = usecase
        .list_model_capability_proofs(ModelCapabilityProofListRequest {
            layout: layout_input(home.to_str().expect("home path")),
            selector: selector.clone(),
        })
        .expect("list proofs");
    assert!(listed.proofs.is_empty());

    let reader = StdModelCatalogReadUseCase::new(&layout_resolver, &catalog);
    let inspected = reader
        .inspect_model(ModelInspectRequest {
            layout: layout_input(home.to_str().expect("home path")),
            selector,
        })
        .expect("inspect model after clear");
    assert!(inspected.model.store_path.is_dir());
    assert_eq!(
        inspected.model.metadata.model_capabilities,
        vec![ModelCapability::Chat]
    );

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
