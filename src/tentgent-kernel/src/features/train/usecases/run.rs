//! LoRA train run record use case.

use std::sync::Arc;

use crate::features::resource_coordination::{
    infra::FileResourceCoordinator, stabilize_resource_transition, ResourceCoordinator,
    ResourceKey, ResourceKind, ResourceLockMode, ResourceLockRequest, ResourcePermit,
    ResourceTransitionAuthorization, StableResourceTransition,
};
use crate::features::train::domain::{
    LoraTrainRun, LoraTrainRunStatus, TrainPlanStatus, LORA_TRAIN_SCHEMA_VERSION,
};
use crate::features::train::ports::{
    LoraTrainPlanStore, LoraTrainRunRefGenerator, LoraTrainRunStore, TrainClock,
    TrainStoreLayoutInitializer,
};
use crate::features::{
    adapter::ports::AdapterCatalogStore, dataset::ports::DatasetCatalogStore,
    model::ports::ModelCatalogStore,
};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::train_store_layout;
use super::dependencies::{
    busy_error, inspect_train_dependencies, train_dependency_locks, TrainDependencySnapshot,
};
use super::port::{
    LoraTrainMetricsTailRequest, LoraTrainRawLogMetadataRequest, LoraTrainRawLogTailRequest,
    LoraTrainRunFinishRequest, LoraTrainRunInspectRequest, LoraTrainRunInspectResult,
    LoraTrainRunListRequest, LoraTrainRunListResult, LoraTrainRunMarkFailedRequest,
    LoraTrainRunStartRequest, LoraTrainRunStartResult, LoraTrainRunUseCase,
    LoraTrainRunWorkerStartedRequest, LoraTrainRunWriteRequest,
};

/// Standard LoRA train run record orchestration.
pub struct StdLoraTrainRunUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    layout_initializer: &'a dyn TrainStoreLayoutInitializer,
    plan_store: &'a dyn LoraTrainPlanStore,
    run_store: &'a dyn LoraTrainRunStore,
    clock: &'a dyn TrainClock,
    run_refs: &'a dyn LoraTrainRunRefGenerator,
    model_catalog: &'a dyn ModelCatalogStore,
    dataset_catalog: &'a dyn DatasetCatalogStore,
    adapter_catalog: &'a dyn AdapterCatalogStore,
    coordinator: Arc<dyn ResourceCoordinator>,
}

pub struct LoraTrainRunDependencyCatalogs<'a> {
    pub model: &'a dyn ModelCatalogStore,
    pub dataset: &'a dyn DatasetCatalogStore,
    pub adapter: &'a dyn AdapterCatalogStore,
}

impl<'a> StdLoraTrainRunUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        layout_initializer: &'a dyn TrainStoreLayoutInitializer,
        plan_store: &'a dyn LoraTrainPlanStore,
        run_store: &'a dyn LoraTrainRunStore,
        clock: &'a dyn TrainClock,
        run_refs: &'a dyn LoraTrainRunRefGenerator,
        dependencies: LoraTrainRunDependencyCatalogs<'a>,
    ) -> Self {
        Self::new_with_coordinator(
            layout_resolver,
            layout_initializer,
            plan_store,
            run_store,
            clock,
            run_refs,
            dependencies,
            Arc::new(FileResourceCoordinator),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_coordinator(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        layout_initializer: &'a dyn TrainStoreLayoutInitializer,
        plan_store: &'a dyn LoraTrainPlanStore,
        run_store: &'a dyn LoraTrainRunStore,
        clock: &'a dyn TrainClock,
        run_refs: &'a dyn LoraTrainRunRefGenerator,
        dependencies: LoraTrainRunDependencyCatalogs<'a>,
        coordinator: Arc<dyn ResourceCoordinator>,
    ) -> Self {
        Self {
            layout_resolver,
            layout_initializer,
            plan_store,
            run_store,
            clock,
            run_refs,
            model_catalog: dependencies.model,
            dataset_catalog: dependencies.dataset,
            adapter_catalog: dependencies.adapter,
            coordinator,
        }
    }
}

impl LoraTrainRunUseCase for StdLoraTrainRunUseCase<'_> {
    fn start_run(
        &self,
        request: LoraTrainRunStartRequest,
    ) -> KernelResult<LoraTrainRunStartResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = train_store_layout(&layout);
        self.layout_initializer.ensure_train_store_layout(&store)?;

        let created_at = self.clock.now_rfc3339()?;
        let transition = stabilize_resource_transition(
            "start-train-run",
            || {
                let plan = self
                    .plan_store
                    .inspect_plan(&store, &request.plan_selector)?
                    .plan;
                let dependencies = inspect_train_dependencies(
                    &layout,
                    &plan,
                    self.model_catalog,
                    self.dataset_catalog,
                    self.adapter_catalog,
                )?;
                let run_ref = self
                    .run_refs
                    .generate_run_ref(&plan.plan_ref, &created_at)?;
                let mut locks = train_dependency_locks(&plan, ResourceLockMode::Shared);
                locks.push((
                    ResourceKey::new(ResourceKind::TrainingSlot, "lora"),
                    ResourceLockMode::Exclusive,
                ));
                locks.push((
                    ResourceKey::new(ResourceKind::TrainRun, &run_ref),
                    ResourceLockMode::Exclusive,
                ));
                Ok((
                    TrainRunDependencyToken {
                        plan: plan.clone(),
                        dependencies,
                        run_ref: run_ref.clone(),
                        locks: locks.clone(),
                    },
                    TrainRunStartState {
                        plan,
                        run_ref,
                        locks,
                    },
                ))
            },
            |state| {
                Ok(match self.coordinator.acquire(
                    &layout,
                    ResourceLockRequest::new("start-train-run", state.locks.clone()),
                )? {
                    Ok(permit) => ResourceTransitionAuthorization::<_, std::convert::Infallible>::Permitted(permit),
                    Err(busy) => ResourceTransitionAuthorization::Busy(busy),
                })
            },
            |state| ResourceKey::new(ResourceKind::TrainPlan, &state.plan.plan_ref),
        )?;
        let (state, _permit) = match transition {
            StableResourceTransition::Stable { state, permit } => (state, permit),
            StableResourceTransition::Busy(busy) => return Err(busy_error(busy)),
            StableResourceTransition::Rejected(never) => match never {},
        };

        if let Some(run) = self.live_running_run(&store)? {
            return Err(KernelError::TrainRuntimeUnavailable(format!(
                "another LoRA train run is already running: {}",
                run.run_ref
            )));
        }

        let TrainRunStartState { plan, run_ref, .. } = state;
        if plan.status == TrainPlanStatus::Blocked {
            return Err(KernelError::TrainStoreUnavailable(format!(
                "LoRA train plan `{}` is blocked: {}",
                plan.plan_ref,
                plan.blockers.join("; ")
            )));
        }

        let short_ref = run_ref.chars().take(12).collect::<String>();
        let artifacts =
            self.run_store
                .initialize_run_artifacts(&store, &plan.plan_ref, &run_ref)?;

        let run = LoraTrainRun {
            schema_version: LORA_TRAIN_SCHEMA_VERSION,
            run_ref,
            short_ref,
            status: LoraTrainRunStatus::Starting,
            phase: Some("worker_spawn".to_string()),
            error: None,
            created_at: created_at.clone(),
            started_at: Some(created_at),
            ended_at: None,
            plan_ref: plan.plan_ref.clone(),
            plan_short_ref: plan.short_ref.clone(),
            model_ref: plan.model_ref.clone(),
            dataset_ref: plan.dataset_ref.clone(),
            backend: plan.backend,
            recipe_hash: plan.plan_ref.clone(),
            pid: None,
            exit_code: None,
            exit_signal: None,
            adapter_ref: None,
            adapter_path: None,
            adapter_output_path: None,
            adapter_store_path: None,
            run_dir: artifacts.run_dir.display().to_string(),
            run_path: artifacts.run_path.display().to_string(),
            metrics_path: artifacts.metrics_path.display().to_string(),
            raw_log_path: artifacts.raw_log_path.display().to_string(),
        };
        self.run_store.save_run(&store, &run)?;

        Ok(LoraTrainRunStartResult {
            layout,
            store,
            outcome: crate::features::train::domain::LoraTrainRunStartOutcome {
                plan,
                run,
                run_dir: artifacts.run_dir,
                run_path: artifacts.run_path,
                metrics_path: artifacts.metrics_path,
                raw_log_path: artifacts.raw_log_path,
            },
        })
    }

    fn record_worker_started(
        &self,
        request: LoraTrainRunWorkerStartedRequest,
    ) -> KernelResult<LoraTrainRun> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = train_store_layout(&layout);
        let preflight = self
            .run_store
            .inspect_run(&store, &request.run_selector)?
            .run;
        let _permit = self.acquire_run_write_permit(&layout, &store, &preflight)?;
        let mut run = self
            .run_store
            .inspect_run(&store, &request.run_selector)?
            .run;
        run.pid = Some(request.pid);
        run.status = LoraTrainRunStatus::Running;
        run.phase = Some("train".to_string());
        run.error = None;
        if run.started_at.is_none() {
            run.started_at = Some(self.clock.now_rfc3339()?);
        }
        self.run_store.save_run(&store, &run)?;
        Ok(run)
    }

    fn write_run(&self, request: LoraTrainRunWriteRequest) -> KernelResult<()> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = train_store_layout(&layout);
        let _permit = self.acquire_run_write_permit(&layout, &store, &request.run)?;
        self.run_store.save_run(&store, &request.run)
    }

    fn finish_run(&self, request: LoraTrainRunFinishRequest) -> KernelResult<LoraTrainRun> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = train_store_layout(&layout);
        let mut run = request.run;
        let _permit = self.acquire_run_write_permit(&layout, &store, &run)?;
        run.status = request.status;
        run.phase = Some(
            match request.status {
                LoraTrainRunStatus::Succeeded => "done",
                LoraTrainRunStatus::Failed => "failed",
                LoraTrainRunStatus::Starting => "worker_spawn",
                LoraTrainRunStatus::Running => "train",
            }
            .to_string(),
        );
        run.ended_at = Some(self.clock.now_rfc3339()?);
        run.exit_code = request.exit_code;
        self.run_store.save_run(&store, &run)?;
        Ok(run)
    }

    fn mark_run_failed(
        &self,
        request: LoraTrainRunMarkFailedRequest,
    ) -> KernelResult<LoraTrainRun> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = train_store_layout(&layout);
        let preflight = self
            .run_store
            .inspect_run(&store, &request.run_selector)?
            .run;
        let _permit = self.acquire_run_write_permit(&layout, &store, &preflight)?;
        let mut run = self
            .run_store
            .inspect_run(&store, &request.run_selector)?
            .run;
        run.status = LoraTrainRunStatus::Failed;
        run.phase = Some(request.phase);
        run.error = Some(request.message);
        run.ended_at = Some(self.clock.now_rfc3339()?);
        run.exit_code = request.exit_code;
        self.run_store.save_run(&store, &run)?;
        Ok(run)
    }

    fn list_runs(&self, request: LoraTrainRunListRequest) -> KernelResult<LoraTrainRunListResult> {
        let (layout_input, plan_selector) = match request {
            LoraTrainRunListRequest::All { layout } => (layout, None),
            LoraTrainRunListRequest::Plan {
                layout,
                plan_selector,
            } => (layout, Some(plan_selector)),
        };
        let layout = self.layout_resolver.resolve(layout_input)?;
        let store = train_store_layout(&layout);
        let runs = match plan_selector {
            Some(plan_selector) => self.run_store.list_plan_runs(&store, &plan_selector)?,
            None => self.run_store.list_runs(&store)?,
        };

        Ok(LoraTrainRunListResult {
            layout,
            store,
            runs,
        })
    }

    fn inspect_run(
        &self,
        request: LoraTrainRunInspectRequest,
    ) -> KernelResult<LoraTrainRunInspectResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = train_store_layout(&layout);
        let inspection = self.run_store.inspect_run(&store, &request.run_selector)?;

        Ok(LoraTrainRunInspectResult {
            layout,
            store,
            inspection,
        })
    }

    fn metrics_tail(
        &self,
        request: LoraTrainMetricsTailRequest,
    ) -> KernelResult<crate::features::train::domain::LoraTrainMetricsTail> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = train_store_layout(&layout);
        self.run_store
            .metrics_tail(&store, &request.run_selector, request.tail)
    }

    fn raw_log_metadata(
        &self,
        request: LoraTrainRawLogMetadataRequest,
    ) -> KernelResult<crate::features::train::domain::TrainRunLogMetadata> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = train_store_layout(&layout);
        self.run_store
            .raw_log_metadata(&store, &request.run_selector)
    }

    fn raw_log_tail(
        &self,
        request: LoraTrainRawLogTailRequest,
    ) -> KernelResult<crate::features::train::domain::TrainRunLogTail> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = train_store_layout(&layout);
        self.run_store
            .raw_log_tail(&store, &request.run_selector, request.tail_bytes)
    }
}

impl StdLoraTrainRunUseCase<'_> {
    fn acquire_run_write_permit(
        &self,
        layout: &crate::foundation::layout::RuntimeLayout,
        store: &crate::features::train::domain::TrainStoreLayout,
        run: &LoraTrainRun,
    ) -> KernelResult<ResourcePermit> {
        let permit = match self.coordinator.acquire(
            layout,
            ResourceLockRequest::new(
                "write-train-run",
                vec![
                    (ResourceKey::maintenance(), ResourceLockMode::Shared),
                    (
                        ResourceKey::new(ResourceKind::TrainPlan, &run.plan_ref),
                        ResourceLockMode::Shared,
                    ),
                    (
                        ResourceKey::new(ResourceKind::TrainRun, &run.run_ref),
                        ResourceLockMode::Exclusive,
                    ),
                ],
            ),
        )? {
            Ok(permit) => permit,
            Err(busy) => {
                return Err(KernelError::ResourceCoordinationUnavailable(format!(
                    "{}; retry after {} ms",
                    busy.description, busy.retry_after_millis
                )));
            }
        };
        let selector = crate::features::train::domain::TrainRefSelector::parse(&run.plan_ref)
            .map_err(|error| KernelError::TrainStoreUnavailable(error.to_string()))?;
        self.plan_store.inspect_plan(store, &selector)?;
        Ok(permit)
    }

    fn live_running_run(
        &self,
        store: &crate::features::train::domain::TrainStoreLayout,
    ) -> KernelResult<Option<LoraTrainRun>> {
        for inspection in self.run_store.list_runs(store)? {
            match inspection.run.status {
                LoraTrainRunStatus::Starting => return Ok(Some(inspection.run)),
                LoraTrainRunStatus::Running
                    if inspection.run.pid.is_none() || inspection.process_running =>
                {
                    return Ok(Some(inspection.run));
                }
                LoraTrainRunStatus::Running
                | LoraTrainRunStatus::Succeeded
                | LoraTrainRunStatus::Failed => {}
            }
        }
        Ok(None)
    }
}

#[derive(PartialEq)]
struct TrainRunDependencyToken {
    plan: crate::features::train::domain::LoraTrainPlan,
    dependencies: TrainDependencySnapshot,
    run_ref: String,
    locks: Vec<(ResourceKey, ResourceLockMode)>,
}

struct TrainRunStartState {
    plan: crate::features::train::domain::LoraTrainPlan,
    run_ref: String,
    locks: Vec<(ResourceKey, ResourceLockMode)>,
}
