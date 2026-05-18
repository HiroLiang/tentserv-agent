//! Training use case ports.

use crate::features::dataset::domain::DatasetRefSelector;
use crate::features::model::domain::ModelRefSelector;
use crate::features::train::domain::{
    LoraTrainBackendRequest, LoraTrainMetricsTail, LoraTrainOverrides, LoraTrainPlanCreateOutcome,
    LoraTrainPlanInspection, LoraTrainPlanPreviewOutcome, LoraTrainPlanRemovalOutcome,
    LoraTrainPlanSummary, LoraTrainRun, LoraTrainRunInspection, LoraTrainRunStartOutcome,
    LoraTrainRunStatus, TrainRefSelector, TrainRunLogMetadata, TrainRunLogTail, TrainStoreLayout,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

/// Request for previewing or creating a LoRA training plan.
#[derive(Debug, Clone, PartialEq)]
pub struct LoraTrainPlanBuildRequest {
    pub layout: RuntimeLayoutInput,
    pub model_selector: ModelRefSelector,
    pub dataset_selector: DatasetRefSelector,
    pub requested_backend: LoraTrainBackendRequest,
    pub name: Option<String>,
    pub overrides: LoraTrainOverrides,
}

/// Request for listing managed LoRA train plans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoraTrainPlanListRequest {
    pub layout: RuntimeLayoutInput,
}

/// Result of listing managed LoRA train plans.
#[derive(Debug, Clone, PartialEq)]
pub struct LoraTrainPlanListResult {
    pub layout: RuntimeLayout,
    pub store: TrainStoreLayout,
    pub plans: Vec<LoraTrainPlanSummary>,
}

/// Request for inspecting one LoRA train plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoraTrainPlanInspectRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: TrainRefSelector,
}

/// Result of inspecting one LoRA train plan.
#[derive(Debug, Clone, PartialEq)]
pub struct LoraTrainPlanInspectResult {
    pub layout: RuntimeLayout,
    pub store: TrainStoreLayout,
    pub inspection: LoraTrainPlanInspection,
}

/// Request for removing one LoRA train plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoraTrainPlanRemoveRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: TrainRefSelector,
}

/// Result of removing one LoRA train plan.
#[derive(Debug, Clone, PartialEq)]
pub struct LoraTrainPlanRemoveResult {
    pub layout: RuntimeLayout,
    pub store: TrainStoreLayout,
    pub outcome: LoraTrainPlanRemovalOutcome,
}

/// Request for starting one LoRA run record from a saved plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoraTrainRunStartRequest {
    pub layout: RuntimeLayoutInput,
    pub plan_selector: TrainRefSelector,
}

/// Result of starting a LoRA run record.
#[derive(Debug, Clone, PartialEq)]
pub struct LoraTrainRunStartResult {
    pub layout: RuntimeLayout,
    pub store: TrainStoreLayout,
    pub outcome: LoraTrainRunStartOutcome,
}

/// Request for updating a run once the worker or foreground process starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoraTrainRunWorkerStartedRequest {
    pub layout: RuntimeLayoutInput,
    pub run_selector: TrainRefSelector,
    pub pid: u32,
}

/// Request for writing a fully owned run record.
#[derive(Debug, Clone, PartialEq)]
pub struct LoraTrainRunWriteRequest {
    pub layout: RuntimeLayoutInput,
    pub run: LoraTrainRun,
}

/// Request for terminal run status updates.
#[derive(Debug, Clone, PartialEq)]
pub struct LoraTrainRunFinishRequest {
    pub layout: RuntimeLayoutInput,
    pub run: LoraTrainRun,
    pub status: LoraTrainRunStatus,
    pub exit_code: Option<i32>,
}

/// Request for marking an existing run failed by selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoraTrainRunMarkFailedRequest {
    pub layout: RuntimeLayoutInput,
    pub run_selector: TrainRefSelector,
    pub phase: String,
    pub message: String,
    pub exit_code: Option<i32>,
}

/// Request for listing LoRA train runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoraTrainRunListRequest {
    All {
        layout: RuntimeLayoutInput,
    },
    Plan {
        layout: RuntimeLayoutInput,
        plan_selector: TrainRefSelector,
    },
}

/// Result of listing LoRA train runs.
#[derive(Debug, Clone, PartialEq)]
pub struct LoraTrainRunListResult {
    pub layout: RuntimeLayout,
    pub store: TrainStoreLayout,
    pub runs: Vec<LoraTrainRunInspection>,
}

/// Request for inspecting one LoRA train run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoraTrainRunInspectRequest {
    pub layout: RuntimeLayoutInput,
    pub run_selector: TrainRefSelector,
}

/// Result of inspecting one LoRA train run.
#[derive(Debug, Clone, PartialEq)]
pub struct LoraTrainRunInspectResult {
    pub layout: RuntimeLayout,
    pub store: TrainStoreLayout,
    pub inspection: LoraTrainRunInspection,
}

/// Request for reading a bounded metrics tail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoraTrainMetricsTailRequest {
    pub layout: RuntimeLayoutInput,
    pub run_selector: TrainRefSelector,
    pub tail: usize,
}

/// Request for reading raw log metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoraTrainRawLogMetadataRequest {
    pub layout: RuntimeLayoutInput,
    pub run_selector: TrainRefSelector,
}

/// Request for reading a bounded raw log tail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoraTrainRawLogTailRequest {
    pub layout: RuntimeLayoutInput,
    pub run_selector: TrainRefSelector,
    pub tail_bytes: u64,
}

/// Use-case boundary for LoRA train plan management.
pub trait LoraTrainPlanUseCase {
    /// Builds a normalized plan without writing it.
    fn preview_plan(
        &self,
        request: LoraTrainPlanBuildRequest,
    ) -> KernelResult<LoraTrainPlanPreviewOutcome>;

    /// Builds and stores or reuses a normalized plan.
    fn create_plan(
        &self,
        request: LoraTrainPlanBuildRequest,
    ) -> KernelResult<LoraTrainPlanCreateOutcome>;

    /// Lists saved LoRA train plans.
    fn list_plans(
        &self,
        request: LoraTrainPlanListRequest,
    ) -> KernelResult<LoraTrainPlanListResult>;

    /// Inspects one saved LoRA train plan.
    fn inspect_plan(
        &self,
        request: LoraTrainPlanInspectRequest,
    ) -> KernelResult<LoraTrainPlanInspectResult>;

    /// Removes one saved LoRA train plan and local run records.
    fn remove_plan(
        &self,
        request: LoraTrainPlanRemoveRequest,
    ) -> KernelResult<LoraTrainPlanRemoveResult>;
}

/// Use-case boundary for LoRA train run records and local run logs.
pub trait LoraTrainRunUseCase {
    /// Creates a durable starting run record from a saved plan.
    fn start_run(&self, request: LoraTrainRunStartRequest)
        -> KernelResult<LoraTrainRunStartResult>;

    /// Records a worker or foreground process id and marks the run running.
    fn record_worker_started(
        &self,
        request: LoraTrainRunWorkerStartedRequest,
    ) -> KernelResult<LoraTrainRun>;

    /// Writes one owned run record.
    fn write_run(&self, request: LoraTrainRunWriteRequest) -> KernelResult<()>;

    /// Finishes one owned run record with a terminal status.
    fn finish_run(&self, request: LoraTrainRunFinishRequest) -> KernelResult<LoraTrainRun>;

    /// Marks an existing run failed by selector.
    fn mark_run_failed(&self, request: LoraTrainRunMarkFailedRequest)
        -> KernelResult<LoraTrainRun>;

    /// Lists all runs or runs for one plan.
    fn list_runs(&self, request: LoraTrainRunListRequest) -> KernelResult<LoraTrainRunListResult>;

    /// Inspects one saved run.
    fn inspect_run(
        &self,
        request: LoraTrainRunInspectRequest,
    ) -> KernelResult<LoraTrainRunInspectResult>;

    /// Reads a bounded metrics tail.
    fn metrics_tail(
        &self,
        request: LoraTrainMetricsTailRequest,
    ) -> KernelResult<LoraTrainMetricsTail>;

    /// Reads raw log metadata.
    fn raw_log_metadata(
        &self,
        request: LoraTrainRawLogMetadataRequest,
    ) -> KernelResult<TrainRunLogMetadata>;

    /// Reads a bounded raw log tail.
    fn raw_log_tail(&self, request: LoraTrainRawLogTailRequest) -> KernelResult<TrainRunLogTail>;
}
