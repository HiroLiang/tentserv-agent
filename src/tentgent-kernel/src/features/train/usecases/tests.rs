use std::path::PathBuf;

use crate::features::dataset::domain::{
    DatasetFormat, DatasetMetadata, DatasetPackageMetadata, DatasetRef, DatasetRefSelector,
    DatasetSourceKind, DatasetSplits, DatasetStoreLayout,
};
use crate::features::dataset::infra::FileDatasetCatalogStore;
use crate::features::dataset::ports::DatasetCatalogStore;
use crate::features::model::domain::{
    default_model_capabilities, default_model_capability_source, ModelFormat, ModelMetadata,
    ModelRef, ModelRefSelector, ModelSourceKind, ModelStoreLayout,
};
use crate::features::model::infra::FileModelCatalogStore;
use crate::features::model::ports::ModelCatalogStore;
use crate::features::train::domain::{
    LoraTrainBackend, LoraTrainBackendRequest, LoraTrainRunStatus, TrainRefSelector,
};
use crate::features::train::infra::{
    FileLoraTrainPlanStore, FileLoraTrainRunStore, StdTrainStoreLayoutInitializer,
};
use crate::features::train::ports::{LoraTrainRunRefGenerator, TrainClock, TrainProcessProbe};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
};
use crate::foundation::platform::{
    Architecture, CpuFacts, GpuFacts, OperatingSystem, PlatformFacts, PlatformProbe,
};

use super::{
    LoraTrainPlanBuildRequest, LoraTrainPlanInspectRequest, LoraTrainPlanListRequest,
    LoraTrainPlanRemoveRequest, LoraTrainPlanUseCase, LoraTrainRunFinishRequest,
    LoraTrainRunInspectRequest, LoraTrainRunStartRequest, LoraTrainRunUseCase,
    LoraTrainRunWorkerStartedRequest, StdLoraTrainPlanUseCase, StdLoraTrainRunUseCase,
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
    );
    let started = runs
        .start_run(LoraTrainRunStartRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            plan_selector: plan_selector.clone(),
        })
        .expect("start run");
    assert_eq!(started.outcome.run.status, LoraTrainRunStatus::Starting);
    assert!(started.outcome.run_path.is_file());

    let run_selector =
        TrainRefSelector::parse(&started.outcome.run.short_ref).expect("run selector");
    let running = runs
        .record_worker_started(LoraTrainRunWorkerStartedRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            run_selector: run_selector.clone(),
            pid: 42,
        })
        .expect("record worker");
    assert_eq!(running.status, LoraTrainRunStatus::Running);

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
