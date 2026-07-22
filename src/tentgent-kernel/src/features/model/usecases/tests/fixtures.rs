use super::*;

pub(super) fn pull_hf_model_for_test(
    home: &Path,
    metadata: Option<HfModelMetadata>,
    capability: Option<ModelCapability>,
) -> super::super::port::ModelHfPullResult {
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

pub(super) fn import_local_for_test(
    home: &Path,
    capability: Option<ModelCapability>,
    model_bytes: &[u8],
) -> super::super::port::ModelLocalImportResult {
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

pub(super) fn try_remove_model_for_test(
    home: &Path,
    reference: &str,
) -> KernelResult<super::super::port::ModelRemoveResult> {
    let layout_resolver = FakeLayoutResolver;
    let catalog = FileModelCatalogStore;
    let indexes = FileModelSourceIndexStore;
    let content = FileModelContentStore;
    let refs = FileModelServerReferenceProbe;
    let remover = StdModelRemoveUseCase::new(&layout_resolver, &catalog, &indexes, &content, &refs);

    remover.remove_model(ModelRemoveRequest {
        layout: layout_input(home.to_str().expect("home path")),
        selector: ModelRefSelector::parse(reference).expect("selector"),
    })
}

pub(super) fn update_capability_for_test(
    home: &Path,
    reference: &str,
    capability: ModelCapability,
) -> super::super::port::ModelCapabilityUpdateResult {
    let layout_resolver = FakeLayoutResolver;
    let catalog = FileModelCatalogStore;
    let refs = crate::features::model::infra::FileModelServerReferenceProbe;
    let updater = StdModelCapabilityUpdateUseCase::new(&layout_resolver, &catalog, &refs);

    updater
        .update_model_capability(ModelCapabilityUpdateRequest {
            layout: layout_input(home.to_str().expect("home path")),
            selector: ModelRefSelector::parse(reference).expect("selector"),
            mutation: ModelCapabilityMutation::Set(vec![capability]),
        })
        .expect("update capability")
}

pub(super) fn update_capabilities_for_test(
    home: &Path,
    reference: &str,
    mutation: ModelCapabilityMutation,
) -> super::super::port::ModelCapabilityUpdateResult {
    try_update_capabilities_for_test(home, reference, mutation).expect("update capability")
}

pub(super) fn try_update_capabilities_for_test(
    home: &Path,
    reference: &str,
    mutation: ModelCapabilityMutation,
) -> KernelResult<super::super::port::ModelCapabilityUpdateResult> {
    let layout_resolver = FakeLayoutResolver;
    let catalog = FileModelCatalogStore;
    let refs = crate::features::model::infra::FileModelServerReferenceProbe;
    let updater = StdModelCapabilityUpdateUseCase::new(&layout_resolver, &catalog, &refs);

    updater.update_model_capability(ModelCapabilityUpdateRequest {
        layout: layout_input(home.to_str().expect("home path")),
        selector: ModelRefSelector::parse(reference).expect("selector"),
        mutation,
    })
}

pub(super) fn write_cluster_definition(
    home: &Path,
    cluster_ref: &str,
    route: &str,
    model_ref: &ModelRef,
) {
    let cluster_dir = home.join("clusters").join(cluster_ref);
    fs::create_dir_all(&cluster_dir).expect("cluster dir");
    fs::write(
        cluster_dir.join("cluster.toml"),
        format!(
            r#"
schema_version = 1
cluster_ref = "{cluster_ref}"

[routes.{route}]
kind = "local-model"
model_ref = "{model_ref}"
"#
        ),
    )
    .expect("cluster definition");
}

pub(super) struct FakeModelUseCases;

impl ModelCatalogReadUseCase for FakeModelUseCases {
    fn list_models(
        &self,
        request: ModelListRequest,
    ) -> KernelResult<super::super::port::ModelListResult> {
        let layout = runtime_layout(request.layout);
        let store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        Ok(super::super::port::ModelListResult {
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
    ) -> KernelResult<super::super::port::ModelInspectResult> {
        let layout = runtime_layout(request.layout);
        let store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        let metadata = metadata_fixture();
        Ok(super::super::port::ModelInspectResult {
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
    ) -> KernelResult<super::super::port::ModelLocalImportResult> {
        let layout = runtime_layout(request.layout);
        let store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        Ok(super::super::port::ModelLocalImportResult {
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
    ) -> KernelResult<super::super::port::ModelHfPullResult> {
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
        Ok(super::super::port::ModelHfPullResult {
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
    ) -> KernelResult<super::super::port::ModelRemoveResult> {
        let layout = runtime_layout(request.layout);
        let store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        Ok(super::super::port::ModelRemoveResult {
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
    ) -> KernelResult<super::super::port::ModelCapabilityUpdateResult> {
        let layout = runtime_layout(request.layout);
        let store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        let metadata = metadata_fixture();
        Ok(super::super::port::ModelCapabilityUpdateResult {
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
    ) -> KernelResult<super::super::port::ModelCapabilityProofListResult> {
        let layout = runtime_layout(request.layout);
        let store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        Ok(super::super::port::ModelCapabilityProofListResult {
            layout,
            store: store.clone(),
            model: inspection(&store),
            proofs: vec![proof_fixture()],
        })
    }

    fn verify_model_capability(
        &self,
        request: ModelCapabilityVerifyRequest,
    ) -> KernelResult<super::super::port::ModelCapabilityProofRecordResult> {
        let layout = runtime_layout(request.layout);
        let store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        Ok(super::super::port::ModelCapabilityProofRecordResult {
            layout,
            store: store.clone(),
            model: inspection(&store),
            proof: proof_fixture(),
        })
    }

    fn record_model_capability_proof(
        &self,
        request: ModelCapabilityProofRecordRequest,
    ) -> KernelResult<super::super::port::ModelCapabilityProofRecordResult> {
        let layout = runtime_layout(request.layout);
        let store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        let mut proof = proof_fixture();
        proof.status = request.status;
        proof.source = request.source;
        proof.server_ref = request.server_ref;
        proof.runtime_profile = request.runtime_profile;
        proof.runtime_profile_version = request.runtime_profile_version;
        proof.error = request.error;
        Ok(super::super::port::ModelCapabilityProofRecordResult {
            layout,
            store: store.clone(),
            model: inspection(&store),
            proof,
        })
    }

    fn clear_model_capability_proofs(
        &self,
        request: ModelCapabilityProofClearRequest,
    ) -> KernelResult<super::super::port::ModelCapabilityProofClearResult> {
        let layout = runtime_layout(request.layout);
        let store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        Ok(super::super::port::ModelCapabilityProofClearResult {
            layout,
            store: store.clone(),
            model: inspection(&store),
            capability: request.capability,
            removed_proof_count: 1,
        })
    }
}

pub(super) fn import_outcome(store: &ModelStoreLayout) -> ModelImportOutcome {
    ModelImportOutcome {
        metadata: metadata_fixture(),
        store_path: store.model_dir(&model_ref()),
        source_index_path: store.local_index_path(&model_ref()),
        deduplicated: false,
    }
}

pub(super) fn inspection(store: &ModelStoreLayout) -> ModelInspection {
    let metadata = metadata_fixture();
    ModelInspection {
        store_path: store.model_dir(&metadata.model_ref),
        manifest_path: store.manifest_path(&metadata.model_ref),
        variant_source_path: store.variant_source_dir(&metadata.model_ref, metadata.primary_format),
        metadata,
    }
}

pub(super) fn proof_fixture() -> ModelCapabilityProof {
    ModelCapabilityProof {
        model_ref: model_ref(),
        capability: ModelCapability::Chat,
        status: ModelCapabilityProofStatus::Verified,
        source: ModelCapabilityProofSource::ManualProbe,
        primary_format: ModelFormat::Gguf,
        mlx_runtime_family: None,
        backend: "gguf".to_string(),
        runtime_version: None,
        runtime_profile: None,
        runtime_profile_version: None,
        server_ref: None,
        checked_at: STATIC_TIME.to_string(),
        error: None,
    }
}

pub(super) fn metadata_fixture() -> ModelMetadata {
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

pub(super) fn model_ref() -> ModelRef {
    ModelRef::parse("a".repeat(64)).expect("model ref")
}

pub(super) fn layout_input(home: &str) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode: LayoutResolveMode::ReadOnly,
        home_dir: Some(PathBuf::from(home)),
        data_root_dir: None,
    }
}

pub(super) fn runtime_layout(input: RuntimeLayoutInput) -> RuntimeLayout {
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

pub(super) fn unique_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("tentgent-{label}-{nanos}"))
}

pub(super) const STATIC_TIME: &str = "2026-05-17T00:00:00Z";

pub(super) struct StaticModelClock;

impl ModelClock for StaticModelClock {
    fn now_rfc3339(&self) -> KernelResult<String> {
        Ok(STATIC_TIME.to_string())
    }
}

pub(super) struct FakeLayoutResolver;

impl RuntimeLayoutResolver for FakeLayoutResolver {
    fn resolve(&self, input: RuntimeLayoutInput) -> KernelResult<RuntimeLayout> {
        Ok(runtime_layout(input))
    }
}

pub(super) struct FakeRuntimeResolver;

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

pub(super) struct FakeAuthResolver;

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
pub(super) struct FakeSnapshotFetcher {
    pub(super) metadata: Option<HfModelMetadata>,
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
