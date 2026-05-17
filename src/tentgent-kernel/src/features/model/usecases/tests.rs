use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::features::auth::domain::{AuthEnvLoadPolicy, Provider};
use crate::features::auth::usecases::{
    AuthSecretResolution, AuthSecretResolutionRequest, AuthSecretResolverUseCase,
};
use crate::features::model::domain::{
    default_model_capabilities, default_model_capability_source, HfModelPullProgress, ModelFormat,
    ModelImportOutcome, ModelInspection, ModelMetadata, ModelRef, ModelRefSelector,
    ModelRemovalOutcome, ModelSourceKind, ModelStoreLayout, ModelSummary,
};
use crate::features::model::infra::{
    FileModelCatalogStore, FileModelContentStore, FileModelServerReferenceProbe,
    FileModelSourceIndexStore, StdModelIdentityGenerator, StdModelManifestBuilder,
    StdModelSourceStager, StdModelStoreLayoutInitializer,
};
use crate::features::model::ports::{
    HfModelSnapshot, HfModelSnapshotFetcher, HfModelSnapshotRequest,
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
    ModelCatalogReadUseCase, ModelHfPullRequest, ModelHfPullUseCase, ModelInspectRequest,
    ModelListRequest, ModelLocalImportRequest, ModelLocalImportUseCase, ModelRemoveRequest,
    ModelRemoveUseCase,
};
use super::{
    StdModelCatalogReadUseCase, StdModelHfPullUseCase, StdModelLocalImportUseCase,
    StdModelRemoveUseCase,
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
        })
        .expect("import local model");
    assert!(!imported.outcome.deduplicated);
    assert_eq!(
        imported.outcome.metadata.source_kind,
        ModelSourceKind::Local
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
fn standard_hf_pull_usecase_resolves_runtime_auth_fetches_snapshot_and_imports() {
    let home = unique_path("model-hf-usecase");
    let layout_resolver = FakeLayoutResolver;
    let runtime_resolver = FakeRuntimeResolver;
    let auth_resolver = FakeAuthResolver;
    let initializer = StdModelStoreLayoutInitializer;
    let stager = StdModelSourceStager;
    let snapshot_fetcher = FakeSnapshotFetcher;
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
    assert!(result.outcome.store_path.is_dir());
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

fn import_outcome(store: &ModelStoreLayout) -> ModelImportOutcome {
    ModelImportOutcome {
        metadata: metadata_fixture(),
        store_path: store.model_dir(&model_ref()),
        source_index_path: store.local_index_path(&model_ref()),
        deduplicated: false,
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

struct FakeSnapshotFetcher;

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
        })
    }
}
