use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::features::adapter::domain::{
    AdapterBackendSupport, AdapterFormat, AdapterMetadata, AdapterRef, AdapterRefSelector,
    AdapterSourceKind, AdapterStoreLayout, AdapterType,
};
use crate::features::adapter::infra::FileAdapterCatalogStore;
use crate::features::adapter::ports::AdapterCatalogStore;
use crate::features::dataset::domain::{
    DatasetFormat, DatasetMetadata, DatasetPackageMetadata, DatasetRef, DatasetRefSelector,
    DatasetSourceKind, DatasetSplits, DatasetStoreLayout,
};
use crate::features::dataset::infra::FileDatasetCatalogStore;
use crate::features::dataset::ports::DatasetCatalogStore;
use crate::features::model::domain::{
    default_model_capabilities, default_model_capability_source, ModelCapability, ModelFormat,
    ModelMetadata, ModelRef, ModelRefSelector, ModelSourceKind, ModelStoreLayout,
};
use crate::features::model::infra::FileModelCatalogStore;
use crate::features::model::ports::ModelCatalogStore;
use crate::features::resource_guard::ResourceMutationOutcome;
use crate::features::train::domain::{
    LoraBackendConfig, LoraTrainBackend, LoraTrainBackendRequest, LoraTrainPlan,
    LoraTrainRunStatus, MlxBackendConfig, TrainRefSelector, TrainStoreLayout,
};
use crate::features::train::infra::{
    FileLoraTrainPlanStore, FileLoraTrainRunStore, StdTrainStoreLayoutInitializer,
};
use crate::features::train::ports::{
    LoraTrainPlanStore, LoraTrainRunRefGenerator, TrainClock, TrainProcessProbe,
};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
};
use crate::foundation::platform::{
    Architecture, CpuFacts, GpuFacts, OperatingSystem, PlatformFacts, PlatformProbe,
};

use super::{
    LoraTrainPlanBuildRequest, LoraTrainPlanInspectRequest, LoraTrainPlanListRequest,
    LoraTrainPlanRemoveRequest, LoraTrainPlanUseCase, LoraTrainRunDependencyCatalogs,
    LoraTrainRunFinishRequest, LoraTrainRunInspectRequest, LoraTrainRunStartRequest,
    LoraTrainRunUseCase, LoraTrainRunWorkerStartedRequest, StdLoraTrainPlanUseCase,
    StdLoraTrainRunUseCase,
};

#[test]
fn standard_train_usecases_create_plan_and_run_records() {
    let fixture = Fixture::new("plan-run");
    fixture.write_model_and_dataset();

    let layout_resolver = StdRuntimeLayoutResolver;
    let platform_probe = StaticPlatformProbe {
        facts: linux_platform(),
    };
    let initializer = StdTrainStoreLayoutInitializer;
    let model_catalog = FileModelCatalogStore;
    let dataset_catalog = FileDatasetCatalogStore;
    let adapter_catalog = FileAdapterCatalogStore;
    let plan_store = FileLoraTrainPlanStore;
    let run_store = FileLoraTrainRunStore::new(StaticProcessProbe { running: true });
    let clock = StaticClock;
    let run_refs = StaticRunRefGenerator;

    let plans = StdLoraTrainPlanUseCase::new(
        &layout_resolver,
        &platform_probe,
        &initializer,
        &model_catalog,
        &dataset_catalog,
        &adapter_catalog,
        &plan_store,
        &clock,
    );
    let created = plans
        .create_plan(LoraTrainPlanBuildRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            model_selector: ModelRefSelector::parse(fixture.model_ref.short_ref())
                .expect("model selector"),
            dataset_selector: DatasetRefSelector::parse(fixture.dataset_ref.short_ref())
                .expect("dataset selector"),
            requested_backend: LoraTrainBackendRequest::Auto,
            name: Some("fixture".to_string()),
            overrides: Default::default(),
        })
        .expect("create train plan");

    assert_eq!(created.plan.backend, Some(LoraTrainBackend::Peft));
    assert_eq!(created.plan.dataset.train_examples, Some(1));
    assert!(created.plan_path.is_file());

    let listed = plans
        .list_plans(LoraTrainPlanListRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
        })
        .expect("list plans");
    assert_eq!(listed.plans.len(), 1);

    let plan_selector = TrainRefSelector::parse(&created.plan.short_ref).expect("plan selector");
    let inspected = plans
        .inspect_plan(LoraTrainPlanInspectRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            selector: plan_selector.clone(),
        })
        .expect("inspect plan");
    assert_eq!(inspected.inspection.plan.plan_ref, created.plan.plan_ref);

    let runs = StdLoraTrainRunUseCase::new(
        &layout_resolver,
        &initializer,
        &plan_store,
        &run_store,
        &clock,
        &run_refs,
        LoraTrainRunDependencyCatalogs {
            model: &model_catalog,
            dataset: &dataset_catalog,
            adapter: &adapter_catalog,
        },
    );
    let started = runs
        .start_run(LoraTrainRunStartRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            plan_selector: plan_selector.clone(),
        })
        .expect("start run");
    assert_eq!(started.outcome.run.status, LoraTrainRunStatus::Starting);
    assert!(started.outcome.run_path.is_file());
    assert!(matches!(
        plans
            .remove_plan_guarded(LoraTrainPlanRemoveRequest {
                layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
                selector: plan_selector.clone(),
            })
            .expect("starting run removal guard"),
        ResourceMutationOutcome::Blocked(_)
    ));

    let run_selector =
        TrainRefSelector::parse(&started.outcome.run.short_ref).expect("run selector");
    let running = runs
        .record_worker_started(LoraTrainRunWorkerStartedRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            run_selector: run_selector.clone(),
            pid: std::process::id(),
        })
        .expect("record worker");
    assert_eq!(running.status, LoraTrainRunStatus::Running);
    assert!(matches!(
        plans
            .remove_plan_guarded(LoraTrainPlanRemoveRequest {
                layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
                selector: plan_selector.clone(),
            })
            .expect("running run removal guard"),
        ResourceMutationOutcome::Blocked(_)
    ));

    let finished = runs
        .finish_run(LoraTrainRunFinishRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            run: running,
            status: LoraTrainRunStatus::Succeeded,
            exit_code: Some(0),
        })
        .expect("finish run");
    assert_eq!(finished.status, LoraTrainRunStatus::Succeeded);

    let inspected_run = runs
        .inspect_run(LoraTrainRunInspectRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            run_selector,
        })
        .expect("inspect run");
    assert_eq!(
        inspected_run.inspection.run.status,
        LoraTrainRunStatus::Succeeded
    );

    let removed = plans
        .remove_plan(LoraTrainPlanRemoveRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            selector: plan_selector,
        })
        .expect("remove plan");
    assert_eq!(removed.outcome.run_count, 1);
    assert!(!removed.outcome.plan_dir.exists());
}

#[test]
fn start_run_fails_closed_when_model_disappears_after_dependency_locking() {
    let fixture = Fixture::new("run-model-delete-race");
    fixture.write_model_and_dataset();
    let plan = create_fixture_plan(&fixture);
    let layout_resolver = StdRuntimeLayoutResolver;
    let initializer = StdTrainStoreLayoutInitializer;
    let plan_store = FileLoraTrainPlanStore;
    let run_store = FileLoraTrainRunStore::new(StaticProcessProbe { running: false });
    let clock = StaticClock;
    let run_refs = StaticRunRefGenerator;
    let model_catalog = MissingAfterFirstModelInspection::default();
    let dataset_catalog = FileDatasetCatalogStore;
    let adapter_catalog = FileAdapterCatalogStore;
    let runs = StdLoraTrainRunUseCase::new(
        &layout_resolver,
        &initializer,
        &plan_store,
        &run_store,
        &clock,
        &run_refs,
        LoraTrainRunDependencyCatalogs {
            model: &model_catalog,
            dataset: &dataset_catalog,
            adapter: &adapter_catalog,
        },
    );

    let error = runs
        .start_run(LoraTrainRunStartRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            plan_selector: TrainRefSelector::parse(&plan.short_ref).expect("plan selector"),
        })
        .expect_err("model deletion race must fail closed");

    assert!(matches!(error, KernelError::ModelStoreUnavailable(_)));
    assert_no_fixture_run(&fixture, &plan);
}

#[test]
fn start_run_fails_closed_when_dataset_disappears_after_dependency_locking() {
    let fixture = Fixture::new("run-dataset-delete-race");
    fixture.write_model_and_dataset();
    let plan = create_fixture_plan(&fixture);
    let layout_resolver = StdRuntimeLayoutResolver;
    let initializer = StdTrainStoreLayoutInitializer;
    let plan_store = FileLoraTrainPlanStore;
    let run_store = FileLoraTrainRunStore::new(StaticProcessProbe { running: false });
    let clock = StaticClock;
    let run_refs = StaticRunRefGenerator;
    let model_catalog = FileModelCatalogStore;
    let dataset_catalog = MissingAfterFirstDatasetInspection::default();
    let adapter_catalog = FileAdapterCatalogStore;
    let runs = StdLoraTrainRunUseCase::new(
        &layout_resolver,
        &initializer,
        &plan_store,
        &run_store,
        &clock,
        &run_refs,
        LoraTrainRunDependencyCatalogs {
            model: &model_catalog,
            dataset: &dataset_catalog,
            adapter: &adapter_catalog,
        },
    );

    let error = runs
        .start_run(LoraTrainRunStartRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            plan_selector: TrainRefSelector::parse(&plan.short_ref).expect("plan selector"),
        })
        .expect_err("dataset deletion race must fail closed");

    assert!(matches!(error, KernelError::DatasetStoreUnavailable(_)));
    assert_no_fixture_run(&fixture, &plan);
}

#[test]
fn start_run_fails_closed_when_resume_adapter_is_rebound_after_dependency_locking() {
    let fixture = Fixture::new("run-resume-adapter-rebind-race");
    fixture.write_model_and_dataset();
    let mut plan = create_fixture_plan(&fixture);
    let adapter_ref = write_resume_adapter(&fixture);
    plan.backend = Some(LoraTrainBackend::Mlx);
    plan.backend_config = LoraBackendConfig {
        mlx: Some(MlxBackendConfig {
            fine_tune_type: "lora".to_string(),
            num_layers: 8,
            grad_checkpoint: false,
            val_batches: 1,
            test_batches: 1,
            resume_adapter_ref: Some(adapter_ref.to_string()),
        }),
        peft: None,
    };
    let layout = StdRuntimeLayoutResolver
        .resolve(fixture.layout_input(LayoutResolveMode::ReadOnly))
        .expect("runtime layout");
    let train_store = TrainStoreLayout::from_train_dir(layout.train_dir);
    FileLoraTrainPlanStore
        .save_plan(&train_store, &plan)
        .expect("save resume plan");

    let layout_resolver = StdRuntimeLayoutResolver;
    let initializer = StdTrainStoreLayoutInitializer;
    let plan_store = FileLoraTrainPlanStore;
    let run_store = FileLoraTrainRunStore::new(StaticProcessProbe { running: false });
    let clock = StaticClock;
    let run_refs = StaticRunRefGenerator;
    let model_catalog = FileModelCatalogStore;
    let dataset_catalog = FileDatasetCatalogStore;
    let adapter_catalog = ReboundAfterFirstAdapterInspection::default();
    let runs = StdLoraTrainRunUseCase::new(
        &layout_resolver,
        &initializer,
        &plan_store,
        &run_store,
        &clock,
        &run_refs,
        LoraTrainRunDependencyCatalogs {
            model: &model_catalog,
            dataset: &dataset_catalog,
            adapter: &adapter_catalog,
        },
    );

    let error = runs
        .start_run(LoraTrainRunStartRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            plan_selector: TrainRefSelector::parse(&plan.short_ref).expect("plan selector"),
        })
        .expect_err("resume adapter rebind race must fail closed");

    assert!(matches!(error, KernelError::TrainStoreUnavailable(_)));
    assert_no_fixture_run(&fixture, &plan);
}

fn create_fixture_plan(fixture: &Fixture) -> LoraTrainPlan {
    let layout_resolver = StdRuntimeLayoutResolver;
    let platform_probe = StaticPlatformProbe {
        facts: linux_platform(),
    };
    let initializer = StdTrainStoreLayoutInitializer;
    let model_catalog = FileModelCatalogStore;
    let dataset_catalog = FileDatasetCatalogStore;
    let adapter_catalog = FileAdapterCatalogStore;
    let plan_store = FileLoraTrainPlanStore;
    let clock = StaticClock;
    StdLoraTrainPlanUseCase::new(
        &layout_resolver,
        &platform_probe,
        &initializer,
        &model_catalog,
        &dataset_catalog,
        &adapter_catalog,
        &plan_store,
        &clock,
    )
    .create_plan(LoraTrainPlanBuildRequest {
        layout: fixture.layout_input(LayoutResolveMode::Create),
        model_selector: ModelRefSelector::parse(fixture.model_ref.short_ref())
            .expect("model selector"),
        dataset_selector: DatasetRefSelector::parse(fixture.dataset_ref.short_ref())
            .expect("dataset selector"),
        requested_backend: LoraTrainBackendRequest::Auto,
        name: Some("race fixture".to_string()),
        overrides: Default::default(),
    })
    .expect("create fixture plan")
    .plan
}

fn assert_no_fixture_run(fixture: &Fixture, plan: &LoraTrainPlan) {
    let layout = StdRuntimeLayoutResolver
        .resolve(fixture.layout_input(LayoutResolveMode::ReadOnly))
        .expect("runtime layout");
    let store = TrainStoreLayout::from_train_dir(layout.train_dir);
    assert!(!store
        .run_toml_path(&plan.plan_ref, &"c".repeat(64))
        .exists());
}

fn write_resume_adapter(fixture: &Fixture) -> AdapterRef {
    let layout = StdRuntimeLayoutResolver
        .resolve(fixture.layout_input(LayoutResolveMode::ReadOnly))
        .expect("runtime layout");
    let store = AdapterStoreLayout::from_adapters_dir(layout.adapters_dir);
    let adapter_ref = AdapterRef::parse("d".repeat(64)).expect("adapter ref");
    FileAdapterCatalogStore
        .save_adapter_metadata(
            &store,
            &AdapterMetadata {
                adapter_ref: adapter_ref.clone(),
                short_ref: adapter_ref.short_ref().to_string(),
                adapter_format: AdapterFormat::Mlx,
                adapter_type: AdapterType::Lora,
                target_capability: Some(ModelCapability::Chat),
                base_model_ref: Some(fixture.model_ref.clone()),
                base_model_source_repo: None,
                base_model_source_revision: None,
                model_family: None,
                backend_support: vec![AdapterBackendSupport::Mlx],
                control_kind: None,
                weight_file: None,
                trigger_words: Vec::new(),
                recommended_scale: None,
                source_kind: AdapterSourceKind::Local,
                source_repo: None,
                source_revision: None,
                source_path: Some("/tmp/resume-adapter".to_string()),
                training_dataset_ref: None,
                training_run_ref: None,
                training_config_ref: None,
                file_count: 1,
                total_bytes: 64,
                imported_at: "2026-05-17T00:00:00Z".to_string(),
            },
        )
        .expect("save resume adapter");
    adapter_ref
}

#[derive(Default)]
struct MissingAfterFirstModelInspection {
    inspections: AtomicUsize,
}

impl ModelCatalogStore for MissingAfterFirstModelInspection {
    fn list_models(
        &self,
        layout: &ModelStoreLayout,
    ) -> KernelResult<Vec<crate::features::model::domain::ModelSummary>> {
        FileModelCatalogStore.list_models(layout)
    }

    fn inspect_model(
        &self,
        layout: &ModelStoreLayout,
        selector: &ModelRefSelector,
    ) -> KernelResult<crate::features::model::domain::ModelInspection> {
        if self.inspections.fetch_add(1, Ordering::SeqCst) == 0 {
            FileModelCatalogStore.inspect_model(layout, selector)
        } else {
            Err(KernelError::ModelStoreUnavailable(
                "model disappeared during guarded train dependency revalidation".to_string(),
            ))
        }
    }

    fn load_model_metadata(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
    ) -> KernelResult<ModelMetadata> {
        FileModelCatalogStore.load_model_metadata(layout, model_ref)
    }

    fn save_model_metadata(
        &self,
        layout: &ModelStoreLayout,
        metadata: &ModelMetadata,
    ) -> KernelResult<()> {
        FileModelCatalogStore.save_model_metadata(layout, metadata)
    }

    fn save_model_manifest(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
        manifest: &crate::features::model::domain::ModelManifest,
    ) -> KernelResult<()> {
        FileModelCatalogStore.save_model_manifest(layout, model_ref, manifest)
    }

    fn save_variant_metadata(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
        variant: &crate::features::model::domain::ModelVariantMetadata,
    ) -> KernelResult<()> {
        FileModelCatalogStore.save_variant_metadata(layout, model_ref, variant)
    }
}

#[derive(Default)]
struct MissingAfterFirstDatasetInspection {
    inspections: AtomicUsize,
}

impl DatasetCatalogStore for MissingAfterFirstDatasetInspection {
    fn list_datasets(
        &self,
        layout: &DatasetStoreLayout,
    ) -> KernelResult<Vec<crate::features::dataset::domain::DatasetSummary>> {
        FileDatasetCatalogStore.list_datasets(layout)
    }

    fn inspect_dataset(
        &self,
        layout: &DatasetStoreLayout,
        selector: &DatasetRefSelector,
    ) -> KernelResult<crate::features::dataset::domain::DatasetInspection> {
        if self.inspections.fetch_add(1, Ordering::SeqCst) == 0 {
            FileDatasetCatalogStore.inspect_dataset(layout, selector)
        } else {
            Err(KernelError::DatasetStoreUnavailable(
                "dataset disappeared during guarded train dependency revalidation".to_string(),
            ))
        }
    }

    fn load_dataset_metadata(
        &self,
        layout: &DatasetStoreLayout,
        dataset_ref: &DatasetRef,
    ) -> KernelResult<DatasetMetadata> {
        FileDatasetCatalogStore.load_dataset_metadata(layout, dataset_ref)
    }

    fn save_dataset_metadata(
        &self,
        layout: &DatasetStoreLayout,
        metadata: &DatasetMetadata,
    ) -> KernelResult<()> {
        FileDatasetCatalogStore.save_dataset_metadata(layout, metadata)
    }

    fn save_dataset_manifest(
        &self,
        layout: &DatasetStoreLayout,
        dataset_ref: &DatasetRef,
        manifest: &crate::features::dataset::domain::DatasetManifest,
    ) -> KernelResult<()> {
        FileDatasetCatalogStore.save_dataset_manifest(layout, dataset_ref, manifest)
    }
}

#[derive(Default)]
struct ReboundAfterFirstAdapterInspection {
    inspections: AtomicUsize,
}

impl AdapterCatalogStore for ReboundAfterFirstAdapterInspection {
    fn list_adapters(
        &self,
        layout: &AdapterStoreLayout,
    ) -> KernelResult<Vec<crate::features::adapter::domain::AdapterSummary>> {
        FileAdapterCatalogStore.list_adapters(layout)
    }

    fn inspect_adapter(
        &self,
        layout: &AdapterStoreLayout,
        selector: &AdapterRefSelector,
    ) -> KernelResult<crate::features::adapter::domain::AdapterInspection> {
        let mut inspection = FileAdapterCatalogStore.inspect_adapter(layout, selector)?;
        if self.inspections.fetch_add(1, Ordering::SeqCst) > 0 {
            inspection.metadata.base_model_ref =
                Some(ModelRef::parse("e".repeat(64)).expect("rebound model ref"));
        }
        Ok(inspection)
    }

    fn load_adapter_metadata(
        &self,
        layout: &AdapterStoreLayout,
        adapter_ref: &AdapterRef,
    ) -> KernelResult<AdapterMetadata> {
        FileAdapterCatalogStore.load_adapter_metadata(layout, adapter_ref)
    }

    fn save_adapter_metadata(
        &self,
        layout: &AdapterStoreLayout,
        metadata: &AdapterMetadata,
    ) -> KernelResult<()> {
        FileAdapterCatalogStore.save_adapter_metadata(layout, metadata)
    }

    fn save_adapter_manifest(
        &self,
        layout: &AdapterStoreLayout,
        adapter_ref: &AdapterRef,
        manifest: &crate::features::adapter::domain::AdapterManifest,
    ) -> KernelResult<()> {
        FileAdapterCatalogStore.save_adapter_manifest(layout, adapter_ref, manifest)
    }
}

struct Fixture {
    home: PathBuf,
    data: PathBuf,
    model_ref: ModelRef,
    dataset_ref: DatasetRef,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "tentgent-kernel-train-usecase-{label}-{}-{nanos}",
            std::process::id(),
        ));
        Self {
            home: root.join("home"),
            data: root.join("data"),
            model_ref: ModelRef::parse(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            )
            .expect("model ref"),
            dataset_ref: DatasetRef::parse(
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            )
            .expect("dataset ref"),
        }
    }

    fn layout_input(&self, mode: LayoutResolveMode) -> RuntimeLayoutInput {
        RuntimeLayoutInput {
            mode,
            home_dir: Some(self.home.clone()),
            data_root_dir: Some(self.data.clone()),
        }
    }

    fn write_model_and_dataset(&self) {
        let layout = StdRuntimeLayoutResolver
            .resolve(self.layout_input(LayoutResolveMode::Create))
            .expect("layout");
        let model_store = ModelStoreLayout::from_models_dir(layout.models_dir);
        let dataset_store = DatasetStoreLayout::from_datasets_dir(layout.datasets_dir);
        let model_catalog = FileModelCatalogStore;
        let dataset_catalog = FileDatasetCatalogStore;

        model_catalog
            .save_model_metadata(
                &model_store,
                &ModelMetadata {
                    model_ref: self.model_ref.clone(),
                    short_ref: self.model_ref.short_ref().to_string(),
                    source_kind: ModelSourceKind::Local,
                    source_repo: None,
                    source_revision: None,
                    source_path: Some("/tmp/model".to_string()),
                    primary_format: ModelFormat::Safetensors,
                    detected_formats: vec![ModelFormat::Safetensors],
                    mlx_runtime_family: None,
                    model_capabilities: default_model_capabilities(),
                    model_capability_source: default_model_capability_source(),
                    file_count: 1,
                    total_bytes: 1024,
                    imported_at: "2026-05-17T00:00:00Z".to_string(),
                },
            )
            .expect("save model");

        let source_dir = dataset_store.source_dir(&self.dataset_ref);
        std::fs::create_dir_all(&source_dir).expect("dataset source dir");
        std::fs::write(
            source_dir.join("train.jsonl"),
            r#"{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"hi"},{"role":"assistant","content":"hello"}]}"#,
        )
        .expect("train split");

        dataset_catalog
            .save_dataset_metadata(
                &dataset_store,
                &DatasetMetadata {
                    dataset_ref: self.dataset_ref.clone(),
                    short_ref: self.dataset_ref.short_ref().to_string(),
                    source_kind: DatasetSourceKind::Local,
                    source_path: Some("/tmp/dataset".to_string()),
                    source_repo: None,
                    source_revision: None,
                    dataset_format: DatasetFormat::Directory,
                    file_count: 1,
                    total_bytes: 128,
                    imported_at: "2026-05-17T00:00:00Z".to_string(),
                    package: DatasetPackageMetadata {
                        tuning_ready: true,
                        splits: DatasetSplits {
                            train: Some("train.jsonl".to_string()),
                            validation: None,
                            test: None,
                            eval_cases: None,
                            source_manifest: None,
                        },
                        warnings: Vec::new(),
                    },
                },
            )
            .expect("save dataset");
    }
}

struct StaticClock;

impl TrainClock for StaticClock {
    fn now_rfc3339(&self) -> KernelResult<String> {
        Ok("2026-05-17T00:00:00Z".to_string())
    }
}

struct StaticRunRefGenerator;

impl LoraTrainRunRefGenerator for StaticRunRefGenerator {
    fn generate_run_ref(&self, _plan_ref: &str, _created_at: &str) -> KernelResult<String> {
        Ok("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string())
    }
}

struct StaticProcessProbe {
    running: bool,
}

impl TrainProcessProbe for StaticProcessProbe {
    fn is_process_running(&self, _pid: u32) -> KernelResult<bool> {
        Ok(self.running)
    }
}

struct StaticPlatformProbe {
    facts: PlatformFacts,
}

impl PlatformProbe for StaticPlatformProbe {
    fn probe(&self) -> KernelResult<PlatformFacts> {
        Ok(self.facts.clone())
    }
}

fn linux_platform() -> PlatformFacts {
    PlatformFacts {
        os: OperatingSystem::Linux,
        arch: Architecture::X86_64,
        libc: None,
        cpu: CpuFacts {
            vendor: None,
            brand: None,
            features: Vec::new(),
        },
        gpu: GpuFacts {
            devices: Vec::new(),
            cuda: None,
            metal: None,
        },
    }
}
