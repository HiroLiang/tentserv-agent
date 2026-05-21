use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::features::adapter::domain::{
    AdapterBackendSupport, AdapterBindOutcome, AdapterCompatibilityTarget, AdapterFormat,
    AdapterImportOutcome, AdapterInspection, AdapterMetadata, AdapterRef, AdapterRefSelector,
    AdapterRemovalOutcome, AdapterSourceKind, AdapterStoreLayout, AdapterSummary, AdapterType,
    HfAdapterPullProgress, PEFT_ADAPTER_MODEL_FILENAME, SHORT_ADAPTER_REF_LENGTH,
};
use crate::features::adapter::infra::{
    FileAdapterBaseIndexStore, FileAdapterCatalogStore, FileAdapterContentStore,
    FileAdapterServerReferenceProbe, FileAdapterSourceIndexStore, StdAdapterIdentityGenerator,
    StdAdapterManifestBuilder, StdAdapterSourceMetadataReader, StdAdapterSourceStager,
    StdAdapterStoreLayoutInitializer,
};
use crate::features::adapter::ports::{
    HfAdapterSnapshot, HfAdapterSnapshotFetcher, HfAdapterSnapshotRequest,
};
use crate::features::auth::domain::{AuthEnvLoadPolicy, Provider};
use crate::features::auth::usecases::{
    AuthSecretResolution, AuthSecretResolutionRequest, AuthSecretResolverUseCase,
};
use crate::features::model::domain::{
    default_model_capabilities, default_model_capability_source, ModelCapability, ModelFormat,
    ModelMetadata, ModelRef, ModelRefSelector, ModelSourceKind, ModelStoreLayout,
};
use crate::features::model::infra::FileModelCatalogStore;
use crate::features::model::ports::ModelCatalogStore;
use crate::features::runtime::domain::{
    PythonRuntimeLayout, PythonRuntimeResolutionInput, PythonRuntimeSource,
};
use crate::features::runtime::ports::PythonRuntimeResolver;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{
    LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput, RuntimeLayoutResolver,
};

use super::port::{
    AdapterBindRequest, AdapterBindResult, AdapterBindUseCase, AdapterCatalogReadUseCase,
    AdapterCompatibilityCheckRequest, AdapterCompatibilityCheckResult,
    AdapterCompatibilityCheckUseCase, AdapterHfPullRequest, AdapterHfPullResult,
    AdapterHfPullUseCase, AdapterImportOptions, AdapterInspectRequest, AdapterInspectResult,
    AdapterListRequest, AdapterListResult, AdapterLocalImportRequest, AdapterLocalImportResult,
    AdapterLocalImportUseCase, AdapterRemoveRequest, AdapterRemoveResult, AdapterRemoveUseCase,
    AdapterTrainRunImportRequest, AdapterTrainRunImportResult, AdapterTrainRunImportUseCase,
};
use super::{
    StdAdapterBindUseCase, StdAdapterCatalogReadUseCase, StdAdapterCompatibilityCheckUseCase,
    StdAdapterHfPullUseCase, StdAdapterLocalImportUseCase, StdAdapterRemoveUseCase,
    StdAdapterTrainRunImportUseCase,
};

#[test]
fn adapter_usecase_ports_cover_catalog_import_pull_train_bind_check_and_remove_workflows() {
    let usecases = FakeAdapterUseCases;
    let layout = layout_input("/tmp/tentgent-adapter-usecases");
    let adapter_selector = AdapterRefSelector::parse(adapter_ref().short_ref()).expect("selector");
    let base_model_selector = ModelRefSelector::parse(model_ref().short_ref()).expect("selector");

    let listed = usecases
        .list_adapters(AdapterListRequest {
            layout: layout.clone(),
        })
        .expect("list adapters");
    assert_eq!(listed.adapters.len(), 1);
    assert_eq!(listed.store.adapters_dir, listed.layout.adapters_dir);

    let inspected = usecases
        .inspect_adapter(AdapterInspectRequest {
            layout: layout.clone(),
            selector: adapter_selector.clone(),
        })
        .expect("inspect adapter");
    assert_eq!(inspected.adapter.metadata.adapter_ref, adapter_ref());

    let imported = usecases
        .import_local_adapter(AdapterLocalImportRequest {
            layout: layout.clone(),
            source_path: PathBuf::from("/tmp/source-adapter"),
            base_model_selector: Some(base_model_selector.clone()),
            options: AdapterImportOptions::default(),
        })
        .expect("import local adapter");
    assert!(!imported.outcome.deduplicated);
    assert_eq!(
        imported.outcome.base_index_path,
        Some(imported.store.base_index_path(&model_ref(), &adapter_ref()))
    );

    let mut progress_events = Vec::new();
    let pulled = usecases
        .pull_hf_adapter(
            AdapterHfPullRequest {
                layout: layout.clone(),
                runtime: PythonRuntimeResolutionInput {
                    project_dir: Some(PathBuf::from("/tmp/python-project")),
                    python_env_dir: Some(PathBuf::from("/tmp/python-env")),
                },
                repo_id: "org/adapter".to_string(),
                revision: Some("main".to_string()),
                base_model_selector: Some(base_model_selector.clone()),
                options: AdapterImportOptions::default(),
                auth: AuthSecretResolutionRequest::for_secret_use(
                    Provider::HuggingFace,
                    AuthEnvLoadPolicy::ProcessOnly,
                ),
            },
            &mut |event| progress_events.push(event),
        )
        .expect("pull hf adapter");
    assert_eq!(
        pulled.runtime.project_dir,
        PathBuf::from("/tmp/python-project")
    );
    assert_eq!(progress_events.len(), 1);

    let train_imported = usecases
        .import_train_run_adapter(AdapterTrainRunImportRequest {
            layout: layout.clone(),
            output_path: PathBuf::from("/tmp/train-output"),
            base_model_selector: base_model_selector.clone(),
            training_dataset_ref: "dataset-ref".to_string(),
            training_run_ref: "run-ref".to_string(),
            training_config_ref: "config-ref".to_string(),
            options: AdapterImportOptions::default(),
        })
        .expect("import train-run adapter");
    assert_eq!(
        train_imported.outcome.metadata.source_kind,
        AdapterSourceKind::TrainRun
    );

    let bound = usecases
        .bind_adapter(AdapterBindRequest {
            layout: layout.clone(),
            adapter_selector: adapter_selector.clone(),
            base_model_selector: base_model_selector.clone(),
        })
        .expect("bind adapter");
    assert_eq!(
        bound.outcome.base_index_path,
        bound.store.base_index_path(&model_ref(), &adapter_ref())
    );

    let compatible = usecases
        .check_adapter_compatibility(AdapterCompatibilityCheckRequest {
            layout: layout.clone(),
            adapter_selector: adapter_selector.clone(),
            target: AdapterCompatibilityTarget {
                base_model_ref: model_ref(),
                base_model_source_repo: Some("org/base".to_string()),
                base_model_source_revision: Some("base-sha".to_string()),
                base_model_capabilities: vec![ModelCapability::Chat],
                required_capability: ModelCapability::Chat,
                backend: AdapterBackendSupport::TransformersPeft,
            },
        })
        .expect("check adapter compatibility");
    assert_eq!(compatible.adapter.metadata.adapter_ref, adapter_ref());

    let removed = usecases
        .remove_adapter(AdapterRemoveRequest {
            layout,
            selector: adapter_selector,
        })
        .expect("remove adapter");
    assert_eq!(removed.outcome.metadata.adapter_ref, adapter_ref());
}

#[test]
fn standard_adapter_usecases_import_list_bind_check_and_remove_local_adapter() {
    let home = unique_path("adapter-local-usecase");
    let source_dir = home.join("adapter-source");
    fs::create_dir_all(&source_dir).expect("source dir");
    fs::write(
        source_dir.join(PEFT_ADAPTER_MODEL_FILENAME),
        b"local adapter bytes",
    )
    .expect("adapter weights");
    fs::write(
        source_dir.join("adapter_config.json"),
        r#"{"base_model_name_or_path":"org/base","revision":"base-sha","model_type":"llama"}"#,
    )
    .expect("adapter config");

    let layout_resolver = FakeLayoutResolver;
    let initializer = StdAdapterStoreLayoutInitializer;
    let stager = StdAdapterSourceStager;
    let manifest_builder = StdAdapterManifestBuilder;
    let identity = StdAdapterIdentityGenerator;
    let source_metadata = StdAdapterSourceMetadataReader;
    let adapter_catalog = FileAdapterCatalogStore;
    let source_indexes = FileAdapterSourceIndexStore;
    let base_indexes = FileAdapterBaseIndexStore;
    let content = FileAdapterContentStore;
    let server_refs = FileAdapterServerReferenceProbe;
    let model_catalog = FileModelCatalogStore;
    save_base_model(&home, &model_catalog);

    let importer = StdAdapterLocalImportUseCase::new(
        &layout_resolver,
        &initializer,
        &stager,
        &manifest_builder,
        &identity,
        &source_metadata,
        &adapter_catalog,
        &source_indexes,
        &base_indexes,
        &content,
        &model_catalog,
    );
    let imported = importer
        .import_local_adapter(AdapterLocalImportRequest {
            layout: layout_input(home.to_str().expect("home path")),
            source_path: source_dir.clone(),
            base_model_selector: Some(
                ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
            ),
            options: AdapterImportOptions::default(),
        })
        .expect("import local adapter");
    assert_eq!(
        imported.outcome.metadata.source_kind,
        AdapterSourceKind::Local
    );
    assert_eq!(imported.outcome.metadata.base_model_ref, Some(model_ref()));
    assert!(imported.outcome.store_path.is_dir());
    assert!(imported.outcome.base_index_path.is_some());

    let reader = StdAdapterCatalogReadUseCase::new(&layout_resolver, &adapter_catalog);
    let listed = reader
        .list_adapters(AdapterListRequest {
            layout: layout_input(home.to_str().expect("home path")),
        })
        .expect("list adapters");
    assert_eq!(listed.adapters.len(), 1);

    let selector = AdapterRefSelector::parse(imported.outcome.metadata.short_ref.as_str())
        .expect("adapter selector");
    let inspected = reader
        .inspect_adapter(AdapterInspectRequest {
            layout: layout_input(home.to_str().expect("home path")),
            selector: selector.clone(),
        })
        .expect("inspect adapter");
    assert_eq!(
        inspected.adapter.metadata.adapter_ref,
        imported.outcome.metadata.adapter_ref
    );

    let binder = StdAdapterBindUseCase::new(
        &layout_resolver,
        &adapter_catalog,
        &source_metadata,
        &base_indexes,
        &model_catalog,
    );
    let bound = binder
        .bind_adapter(AdapterBindRequest {
            layout: layout_input(home.to_str().expect("home path")),
            adapter_selector: selector.clone(),
            base_model_selector: ModelRefSelector::parse(model_ref().short_ref())
                .expect("model selector"),
        })
        .expect("bind adapter");
    assert_eq!(bound.outcome.metadata.base_model_ref, Some(model_ref()));

    let checker = StdAdapterCompatibilityCheckUseCase::new(&layout_resolver, &adapter_catalog);
    checker
        .check_adapter_compatibility(AdapterCompatibilityCheckRequest {
            layout: layout_input(home.to_str().expect("home path")),
            adapter_selector: selector.clone(),
            target: AdapterCompatibilityTarget {
                base_model_ref: model_ref(),
                base_model_source_repo: Some("org/base".to_string()),
                base_model_source_revision: Some("base-sha".to_string()),
                base_model_capabilities: vec![ModelCapability::Chat],
                required_capability: ModelCapability::Chat,
                backend: AdapterBackendSupport::TransformersPeft,
            },
        })
        .expect("compatible adapter");

    let remover = StdAdapterRemoveUseCase::new(
        &layout_resolver,
        &adapter_catalog,
        &source_indexes,
        &base_indexes,
        &content,
        &server_refs,
    );
    let removed = remover
        .remove_adapter(AdapterRemoveRequest {
            layout: layout_input(home.to_str().expect("home path")),
            selector,
        })
        .expect("remove adapter");
    assert_eq!(
        removed.outcome.metadata.adapter_ref,
        imported.outcome.metadata.adapter_ref
    );
    assert!(!removed.outcome.store_path.exists());
    assert!(!removed.outcome.removed_index_paths.is_empty());
}

#[test]
fn standard_hf_pull_usecase_resolves_runtime_auth_fetches_snapshot_and_imports() {
    let home = unique_path("adapter-hf-usecase");
    let layout_resolver = FakeLayoutResolver;
    let runtime_resolver = FakeRuntimeResolver;
    let auth_resolver = FakeAuthResolver;
    let initializer = StdAdapterStoreLayoutInitializer;
    let stager = StdAdapterSourceStager;
    let snapshot_fetcher = FakeAdapterSnapshotFetcher;
    let manifest_builder = StdAdapterManifestBuilder;
    let identity = StdAdapterIdentityGenerator;
    let source_metadata = StdAdapterSourceMetadataReader;
    let adapter_catalog = FileAdapterCatalogStore;
    let source_indexes = FileAdapterSourceIndexStore;
    let base_indexes = FileAdapterBaseIndexStore;
    let content = FileAdapterContentStore;
    let model_catalog = FileModelCatalogStore;
    let usecase = StdAdapterHfPullUseCase::new(
        &layout_resolver,
        &runtime_resolver,
        &auth_resolver,
        &initializer,
        &stager,
        &snapshot_fetcher,
        &manifest_builder,
        &identity,
        &source_metadata,
        &adapter_catalog,
        &source_indexes,
        &base_indexes,
        &content,
        &model_catalog,
    );

    let mut progress = Vec::new();
    let result = usecase
        .pull_hf_adapter(
            AdapterHfPullRequest {
                layout: layout_input(home.to_str().expect("home path")),
                runtime: PythonRuntimeResolutionInput {
                    project_dir: Some(home.join("python")),
                    python_env_dir: Some(home.join("python-env")),
                },
                repo_id: "org/adapter".to_string(),
                revision: Some("main".to_string()),
                base_model_selector: None,
                options: AdapterImportOptions::default(),
                auth: AuthSecretResolutionRequest::for_secret_use(
                    Provider::HuggingFace,
                    AuthEnvLoadPolicy::ProcessOnly,
                ),
            },
            &mut |event| progress.push(event),
        )
        .expect("pull hf adapter");

    assert_eq!(progress.len(), 1);
    assert_eq!(result.runtime.project_dir, home.join("python"));
    assert_eq!(
        result.outcome.metadata.source_kind,
        AdapterSourceKind::HuggingFace
    );
    assert_eq!(
        result.outcome.metadata.source_repo.as_deref(),
        Some("org/adapter")
    );
    assert_eq!(
        result.outcome.metadata.source_revision.as_deref(),
        Some("resolved-sha")
    );
    assert!(result.outcome.store_path.is_dir());
}

#[test]
fn standard_train_run_import_records_training_provenance() {
    let home = unique_path("adapter-train-run-usecase");
    let output_dir = home.join("train-output");
    fs::create_dir_all(&output_dir).expect("output dir");
    fs::write(
        output_dir.join(PEFT_ADAPTER_MODEL_FILENAME),
        b"trained adapter bytes",
    )
    .expect("adapter weights");
    fs::write(
        output_dir.join("adapter_config.json"),
        r#"{"base_model_name_or_path":"org/base","revision":"base-sha"}"#,
    )
    .expect("adapter config");

    let layout_resolver = FakeLayoutResolver;
    let initializer = StdAdapterStoreLayoutInitializer;
    let stager = StdAdapterSourceStager;
    let manifest_builder = StdAdapterManifestBuilder;
    let identity = StdAdapterIdentityGenerator;
    let source_metadata = StdAdapterSourceMetadataReader;
    let adapter_catalog = FileAdapterCatalogStore;
    let source_indexes = FileAdapterSourceIndexStore;
    let base_indexes = FileAdapterBaseIndexStore;
    let content = FileAdapterContentStore;
    let model_catalog = FileModelCatalogStore;
    save_base_model(&home, &model_catalog);

    let usecase = StdAdapterTrainRunImportUseCase::new(
        &layout_resolver,
        &initializer,
        &stager,
        &manifest_builder,
        &identity,
        &source_metadata,
        &adapter_catalog,
        &source_indexes,
        &base_indexes,
        &content,
        &model_catalog,
    );
    let result = usecase
        .import_train_run_adapter(AdapterTrainRunImportRequest {
            layout: layout_input(home.to_str().expect("home path")),
            output_path: output_dir,
            base_model_selector: ModelRefSelector::parse(model_ref().short_ref())
                .expect("model selector"),
            training_dataset_ref: "dataset-ref".to_string(),
            training_run_ref: "run-ref".to_string(),
            training_config_ref: "config-ref".to_string(),
            options: AdapterImportOptions::default(),
        })
        .expect("import train-run adapter");

    assert_eq!(
        result.outcome.metadata.source_kind,
        AdapterSourceKind::TrainRun
    );
    assert_eq!(
        result.outcome.metadata.training_run_ref.as_deref(),
        Some("run-ref")
    );
    assert_eq!(
        result.outcome.source_index_path,
        result.store.train_run_index_path("run-ref")
    );
}

struct FakeAdapterUseCases;

impl AdapterCatalogReadUseCase for FakeAdapterUseCases {
    fn list_adapters(&self, request: AdapterListRequest) -> KernelResult<AdapterListResult> {
        let layout = runtime_layout(request.layout);
        let store = AdapterStoreLayout::from_adapters_dir(layout.adapters_dir.clone());
        Ok(AdapterListResult {
            layout,
            store: store.clone(),
            adapters: vec![AdapterSummary {
                metadata: metadata_fixture(AdapterSourceKind::Local),
                store_path: store.adapter_dir(&adapter_ref()),
            }],
        })
    }

    fn inspect_adapter(
        &self,
        request: AdapterInspectRequest,
    ) -> KernelResult<AdapterInspectResult> {
        let layout = runtime_layout(request.layout);
        let store = AdapterStoreLayout::from_adapters_dir(layout.adapters_dir.clone());
        Ok(AdapterInspectResult {
            layout,
            adapter: inspection(&store),
            store,
        })
    }
}

impl AdapterLocalImportUseCase for FakeAdapterUseCases {
    fn import_local_adapter(
        &self,
        request: AdapterLocalImportRequest,
    ) -> KernelResult<AdapterLocalImportResult> {
        let layout = runtime_layout(request.layout);
        let store = AdapterStoreLayout::from_adapters_dir(layout.adapters_dir.clone());
        Ok(AdapterLocalImportResult {
            layout,
            outcome: import_outcome(&store, AdapterSourceKind::Local),
            store,
        })
    }
}

impl AdapterHfPullUseCase for FakeAdapterUseCases {
    fn pull_hf_adapter(
        &self,
        request: AdapterHfPullRequest,
        progress: &mut dyn FnMut(HfAdapterPullProgress),
    ) -> KernelResult<AdapterHfPullResult> {
        progress(HfAdapterPullProgress {
            description: request.repo_id,
            position: 1,
            total: Some(1),
            unit: "files".to_string(),
            finished: true,
        });

        let layout = runtime_layout(request.layout);
        let store = AdapterStoreLayout::from_adapters_dir(layout.adapters_dir.clone());
        let runtime = PythonRuntimeLayout {
            project_dir: request.runtime.project_dir.unwrap_or_default(),
            env_dir: request.runtime.python_env_dir.unwrap_or_default(),
            source: PythonRuntimeSource::EnvironmentOverride,
        };
        Ok(AdapterHfPullResult {
            layout,
            store: store.clone(),
            runtime,
            outcome: import_outcome(&store, AdapterSourceKind::HuggingFace),
        })
    }
}

impl AdapterTrainRunImportUseCase for FakeAdapterUseCases {
    fn import_train_run_adapter(
        &self,
        request: AdapterTrainRunImportRequest,
    ) -> KernelResult<AdapterTrainRunImportResult> {
        let layout = runtime_layout(request.layout);
        let store = AdapterStoreLayout::from_adapters_dir(layout.adapters_dir.clone());
        Ok(AdapterTrainRunImportResult {
            layout,
            store: store.clone(),
            outcome: import_outcome(&store, AdapterSourceKind::TrainRun),
        })
    }
}

impl AdapterBindUseCase for FakeAdapterUseCases {
    fn bind_adapter(&self, request: AdapterBindRequest) -> KernelResult<AdapterBindResult> {
        let layout = runtime_layout(request.layout);
        let store = AdapterStoreLayout::from_adapters_dir(layout.adapters_dir.clone());
        Ok(AdapterBindResult {
            layout,
            store: store.clone(),
            outcome: AdapterBindOutcome {
                metadata: metadata_fixture(AdapterSourceKind::Local),
                store_path: store.adapter_dir(&adapter_ref()),
                base_index_path: store.base_index_path(&model_ref(), &adapter_ref()),
                removed_base_index_path: None,
            },
        })
    }
}

impl AdapterCompatibilityCheckUseCase for FakeAdapterUseCases {
    fn check_adapter_compatibility(
        &self,
        request: AdapterCompatibilityCheckRequest,
    ) -> KernelResult<AdapterCompatibilityCheckResult> {
        let layout = runtime_layout(request.layout);
        let store = AdapterStoreLayout::from_adapters_dir(layout.adapters_dir.clone());
        Ok(AdapterCompatibilityCheckResult {
            layout,
            adapter: inspection(&store),
            store,
        })
    }
}

impl AdapterRemoveUseCase for FakeAdapterUseCases {
    fn remove_adapter(&self, request: AdapterRemoveRequest) -> KernelResult<AdapterRemoveResult> {
        let layout = runtime_layout(request.layout);
        let store = AdapterStoreLayout::from_adapters_dir(layout.adapters_dir.clone());
        Ok(AdapterRemoveResult {
            layout,
            store: store.clone(),
            outcome: AdapterRemovalOutcome {
                metadata: metadata_fixture(AdapterSourceKind::Local),
                store_path: store.adapter_dir(&adapter_ref()),
                removed_index_paths: vec![store.local_index_path(&adapter_ref())],
            },
        })
    }
}

fn inspection(store: &AdapterStoreLayout) -> AdapterInspection {
    AdapterInspection {
        metadata: metadata_fixture(AdapterSourceKind::Local),
        store_path: store.adapter_dir(&adapter_ref()),
        manifest_path: store.manifest_path(&adapter_ref()),
        source_path: store.source_dir(&adapter_ref()),
    }
}

fn import_outcome(
    store: &AdapterStoreLayout,
    source_kind: AdapterSourceKind,
) -> AdapterImportOutcome {
    AdapterImportOutcome {
        metadata: metadata_fixture(source_kind),
        store_path: store.adapter_dir(&adapter_ref()),
        source_index_path: match source_kind {
            AdapterSourceKind::HuggingFace => store.hf_index_path("org/adapter", "resolved-sha"),
            AdapterSourceKind::Local => store.local_index_path(&adapter_ref()),
            AdapterSourceKind::TrainRun => store.train_run_index_path("run-ref"),
        },
        base_index_path: Some(store.base_index_path(&model_ref(), &adapter_ref())),
        deduplicated: false,
    }
}

fn metadata_fixture(source_kind: AdapterSourceKind) -> AdapterMetadata {
    AdapterMetadata {
        adapter_ref: adapter_ref(),
        short_ref: "a".repeat(SHORT_ADAPTER_REF_LENGTH),
        adapter_format: AdapterFormat::Peft,
        adapter_type: AdapterType::Lora,
        target_capability: Some(ModelCapability::Chat),
        base_model_ref: Some(model_ref()),
        base_model_source_repo: Some("org/base".to_string()),
        base_model_source_revision: Some("base-sha".to_string()),
        model_family: Some("llama".to_string()),
        backend_support: vec![AdapterBackendSupport::TransformersPeft],
        weight_file: None,
        trigger_words: Vec::new(),
        recommended_scale: None,
        source_kind,
        source_repo: (source_kind == AdapterSourceKind::HuggingFace)
            .then(|| "org/adapter".to_string()),
        source_revision: (source_kind == AdapterSourceKind::HuggingFace)
            .then(|| "resolved-sha".to_string()),
        source_path: (source_kind == AdapterSourceKind::Local)
            .then(|| "/tmp/source-adapter".to_string()),
        training_dataset_ref: (source_kind == AdapterSourceKind::TrainRun)
            .then(|| "dataset-ref".to_string()),
        training_run_ref: (source_kind == AdapterSourceKind::TrainRun)
            .then(|| "run-ref".to_string()),
        training_config_ref: (source_kind == AdapterSourceKind::TrainRun)
            .then(|| "config-ref".to_string()),
        file_count: 1,
        total_bytes: 42,
        imported_at: "2026-05-17T00:00:00Z".to_string(),
    }
}

fn adapter_ref() -> AdapterRef {
    AdapterRef::parse("a".repeat(64)).expect("adapter ref")
}

fn model_ref() -> ModelRef {
    ModelRef::parse("b".repeat(64)).expect("model ref")
}

fn model_metadata_fixture() -> ModelMetadata {
    let model_ref = model_ref();
    ModelMetadata {
        short_ref: model_ref.short_ref().to_string(),
        model_ref,
        source_kind: ModelSourceKind::HuggingFace,
        source_repo: Some("org/base".to_string()),
        source_revision: Some("base-sha".to_string()),
        source_path: None,
        primary_format: ModelFormat::Safetensors,
        detected_formats: vec![ModelFormat::Safetensors],
        mlx_runtime_family: None,
        model_capabilities: default_model_capabilities(),
        model_capability_source: default_model_capability_source(),
        file_count: 1,
        total_bytes: 42,
        imported_at: "2026-05-17T00:00:00Z".to_string(),
    }
}

fn save_base_model(home: &Path, catalog: &dyn ModelCatalogStore) {
    let layout = runtime_layout(layout_input(home.to_str().expect("home path")));
    let store = ModelStoreLayout::from_models_dir(layout.models_dir);
    catalog
        .save_model_metadata(&store, &model_metadata_fixture())
        .expect("save base model metadata");
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

struct FakeAdapterSnapshotFetcher;

impl HfAdapterSnapshotFetcher for FakeAdapterSnapshotFetcher {
    fn fetch_hf_snapshot(
        &self,
        request: HfAdapterSnapshotRequest,
        progress: &mut dyn FnMut(HfAdapterPullProgress),
    ) -> KernelResult<HfAdapterSnapshot> {
        fs::create_dir_all(&request.destination_dir).expect("destination dir");
        fs::write(
            request.destination_dir.join(PEFT_ADAPTER_MODEL_FILENAME),
            b"hf adapter bytes",
        )
        .expect("snapshot adapter");
        fs::write(
            request.destination_dir.join("adapter_config.json"),
            r#"{"base_model_name_or_path":"org/base","revision":"base-sha"}"#,
        )
        .expect("snapshot config");
        progress(HfAdapterPullProgress {
            description: request.repo_id.clone(),
            position: 1,
            total: Some(1),
            unit: "files".to_string(),
            finished: true,
        });

        Ok(HfAdapterSnapshot {
            repo_id: request.repo_id,
            resolved_revision: "resolved-sha".to_string(),
            local_dir: request.destination_dir,
        })
    }
}
