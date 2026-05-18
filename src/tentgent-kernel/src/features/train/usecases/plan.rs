//! LoRA train plan use case.

use crate::features::dataset::ports::DatasetCatalogStore;
use crate::features::model::ports::ModelCatalogStore;
use crate::features::train::domain::{
    LoraTrainPlanCreateOutcome, LoraTrainPlanPreviewOutcome, LoraTrainPlanRemovalOutcome,
};
use crate::features::train::ports::{LoraTrainPlanStore, TrainClock, TrainStoreLayoutInitializer};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;
use crate::foundation::platform::PlatformProbe;

use super::common::{
    build_lora_train_plan, dataset_store_layout, finalize_plan_identity, model_store_layout,
    train_store_layout,
};
use super::port::{
    LoraTrainPlanBuildRequest, LoraTrainPlanInspectRequest, LoraTrainPlanInspectResult,
    LoraTrainPlanListRequest, LoraTrainPlanListResult, LoraTrainPlanRemoveRequest,
    LoraTrainPlanRemoveResult, LoraTrainPlanUseCase,
};

/// Standard LoRA train plan orchestration.
pub struct StdLoraTrainPlanUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    platform_probe: &'a dyn PlatformProbe,
    layout_initializer: &'a dyn TrainStoreLayoutInitializer,
    model_catalog: &'a dyn ModelCatalogStore,
    dataset_catalog: &'a dyn DatasetCatalogStore,
    plan_store: &'a dyn LoraTrainPlanStore,
    clock: &'a dyn TrainClock,
}

impl<'a> StdLoraTrainPlanUseCase<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        platform_probe: &'a dyn PlatformProbe,
        layout_initializer: &'a dyn TrainStoreLayoutInitializer,
        model_catalog: &'a dyn ModelCatalogStore,
        dataset_catalog: &'a dyn DatasetCatalogStore,
        plan_store: &'a dyn LoraTrainPlanStore,
        clock: &'a dyn TrainClock,
    ) -> Self {
        Self {
            layout_resolver,
            platform_probe,
            layout_initializer,
            model_catalog,
            dataset_catalog,
            plan_store,
            clock,
        }
    }
}

impl LoraTrainPlanUseCase for StdLoraTrainPlanUseCase<'_> {
    fn preview_plan(
        &self,
        request: LoraTrainPlanBuildRequest,
    ) -> KernelResult<LoraTrainPlanPreviewOutcome> {
        let prepared = self.prepare_plan(request)?;
        let existing = if prepared.plan_path.exists() {
            Some(
                self.plan_store
                    .load_plan(&prepared.store, &prepared.plan.plan_ref)?,
            )
        } else {
            None
        };

        let (plan, would_reuse) = match existing {
            Some(plan) => (plan, true),
            None => (prepared.plan, false),
        };
        let run_count = self
            .plan_store
            .count_runs(&prepared.store, &plan.plan_ref)?;

        Ok(LoraTrainPlanPreviewOutcome {
            plan,
            plan_dir: prepared.plan_dir,
            plan_path: prepared.plan_path,
            would_reuse,
            run_count,
        })
    }

    fn create_plan(
        &self,
        request: LoraTrainPlanBuildRequest,
    ) -> KernelResult<LoraTrainPlanCreateOutcome> {
        let prepared = self.prepare_plan(request)?;
        self.layout_initializer
            .ensure_train_store_layout(&prepared.store)?;

        if prepared.plan_path.exists() {
            let plan = self
                .plan_store
                .load_plan(&prepared.store, &prepared.plan.plan_ref)?;
            return Ok(LoraTrainPlanCreateOutcome {
                run_count: self
                    .plan_store
                    .count_runs(&prepared.store, &plan.plan_ref)?,
                plan,
                plan_dir: prepared.plan_dir,
                plan_path: prepared.plan_path,
                deduplicated: true,
            });
        }

        std::fs::create_dir_all(prepared.store.plan_runs_dir(&prepared.plan.plan_ref)).map_err(
            |err| {
                crate::foundation::error::KernelError::TrainStoreUnavailable(format!(
                    "create train plan runs directory failed: {err}"
                ))
            },
        )?;
        self.plan_store.save_plan(&prepared.store, &prepared.plan)?;

        Ok(LoraTrainPlanCreateOutcome {
            plan: prepared.plan,
            plan_dir: prepared.plan_dir,
            plan_path: prepared.plan_path,
            deduplicated: false,
            run_count: 0,
        })
    }

    fn list_plans(
        &self,
        request: LoraTrainPlanListRequest,
    ) -> KernelResult<LoraTrainPlanListResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = train_store_layout(&layout);
        let plans = self.plan_store.list_plans(&store)?;

        Ok(LoraTrainPlanListResult {
            layout,
            store,
            plans,
        })
    }

    fn inspect_plan(
        &self,
        request: LoraTrainPlanInspectRequest,
    ) -> KernelResult<LoraTrainPlanInspectResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = train_store_layout(&layout);
        let inspection = self.plan_store.inspect_plan(&store, &request.selector)?;

        Ok(LoraTrainPlanInspectResult {
            layout,
            store,
            inspection,
        })
    }

    fn remove_plan(
        &self,
        request: LoraTrainPlanRemoveRequest,
    ) -> KernelResult<LoraTrainPlanRemoveResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = train_store_layout(&layout);
        let inspection = self.plan_store.inspect_plan(&store, &request.selector)?;
        let outcome = LoraTrainPlanRemovalOutcome {
            plan: inspection.plan.clone(),
            plan_dir: inspection.plan_dir.clone(),
            run_count: inspection.run_count,
        };
        self.plan_store
            .remove_plan(&store, &inspection.plan.plan_ref)?;

        Ok(LoraTrainPlanRemoveResult {
            layout,
            store,
            outcome,
        })
    }
}

impl StdLoraTrainPlanUseCase<'_> {
    fn prepare_plan(&self, request: LoraTrainPlanBuildRequest) -> KernelResult<PreparedPlan> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let platform = self.platform_probe.probe()?;
        let train_store = train_store_layout(&layout);
        let model_store = model_store_layout(&layout);
        let dataset_store = dataset_store_layout(&layout);
        let model = self
            .model_catalog
            .inspect_model(&model_store, &request.model_selector)?;
        let dataset = self
            .dataset_catalog
            .inspect_dataset(&dataset_store, &request.dataset_selector)?;
        let mut plan = build_lora_train_plan(
            &platform,
            &model,
            &dataset,
            request.requested_backend,
            request.name,
            request.overrides,
            self.clock.now_rfc3339()?,
        )?;
        finalize_plan_identity(&train_store, &mut plan)?;

        Ok(PreparedPlan {
            plan_dir: train_store.plan_dir(&plan.plan_ref),
            plan_path: train_store.plan_toml_path(&plan.plan_ref),
            plan,
            store: train_store,
        })
    }
}

struct PreparedPlan {
    plan: crate::features::train::domain::LoraTrainPlan,
    store: crate::features::train::domain::TrainStoreLayout,
    plan_dir: std::path::PathBuf,
    plan_path: std::path::PathBuf,
}
