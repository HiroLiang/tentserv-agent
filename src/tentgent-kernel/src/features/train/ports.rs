//! Training feature package ports.

use std::path::Path;
use std::path::PathBuf;

use crate::foundation::error::KernelResult;

use super::domain::{
    LoraTrainMetricsTail, LoraTrainPlan, LoraTrainPlanInspection, LoraTrainPlanSummary,
    LoraTrainRun, LoraTrainRunInspection, TrainRefSelector, TrainRunLogMetadata, TrainRunLogTail,
    TrainStoreLayout,
};

/// Ensures training-store directories exist for mutating train operations.
pub trait TrainStoreLayoutInitializer {
    /// Creates LoRA train plan and staging directories for the layout.
    fn ensure_train_store_layout(&self, layout: &TrainStoreLayout) -> KernelResult<()>;
}

/// Reads and writes managed LoRA train plans.
pub trait LoraTrainPlanStore {
    /// Returns stored plan summaries sorted for stable display.
    fn list_plans(&self, layout: &TrainStoreLayout) -> KernelResult<Vec<LoraTrainPlanSummary>>;

    /// Resolves a full train plan ref or unique prefix and returns inspection paths.
    fn inspect_plan(
        &self,
        layout: &TrainStoreLayout,
        selector: &TrainRefSelector,
    ) -> KernelResult<LoraTrainPlanInspection>;

    /// Loads a plan after the caller has an exact plan ref.
    fn load_plan(&self, layout: &TrainStoreLayout, plan_ref: &str) -> KernelResult<LoraTrainPlan>;

    /// Stores or replaces a plan record.
    fn save_plan(&self, layout: &TrainStoreLayout, plan: &LoraTrainPlan) -> KernelResult<()>;

    /// Removes one plan directory including any run records under it.
    fn remove_plan(&self, layout: &TrainStoreLayout, plan_ref: &str) -> KernelResult<()>;

    /// Counts run directories stored under one exact plan ref.
    fn count_runs(&self, layout: &TrainStoreLayout, plan_ref: &str) -> KernelResult<usize>;
}

/// Reads and writes managed LoRA training run records.
pub trait LoraTrainRunStore {
    /// Creates one run directory and empty metrics/raw-log artifacts.
    fn initialize_run_artifacts(
        &self,
        layout: &TrainStoreLayout,
        plan_ref: &str,
        run_ref: &str,
    ) -> KernelResult<LoraTrainRunArtifactPaths>;

    /// Writes or replaces one run record.
    fn save_run(&self, layout: &TrainStoreLayout, run: &LoraTrainRun) -> KernelResult<()>;

    /// Lists runs across all plans.
    fn list_runs(&self, layout: &TrainStoreLayout) -> KernelResult<Vec<LoraTrainRunInspection>>;

    /// Lists runs under one selected plan.
    fn list_plan_runs(
        &self,
        layout: &TrainStoreLayout,
        plan_selector: &TrainRefSelector,
    ) -> KernelResult<Vec<LoraTrainRunInspection>>;

    /// Resolves a full run ref or unique prefix.
    fn inspect_run(
        &self,
        layout: &TrainStoreLayout,
        run_selector: &TrainRefSelector,
    ) -> KernelResult<LoraTrainRunInspection>;

    /// Reads a bounded tail of JSON metric events.
    fn metrics_tail(
        &self,
        layout: &TrainStoreLayout,
        run_selector: &TrainRefSelector,
        tail: usize,
    ) -> KernelResult<LoraTrainMetricsTail>;

    /// Reads raw backend log metadata for one run.
    fn raw_log_metadata(
        &self,
        layout: &TrainStoreLayout,
        run_selector: &TrainRefSelector,
    ) -> KernelResult<TrainRunLogMetadata>;

    /// Reads a bounded tail of raw backend log content for one run.
    fn raw_log_tail(
        &self,
        layout: &TrainStoreLayout,
        run_selector: &TrainRefSelector,
        tail_bytes: u64,
    ) -> KernelResult<TrainRunLogTail>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoraTrainRunArtifactPaths {
    pub run_dir: PathBuf,
    pub run_path: PathBuf,
    pub metrics_path: PathBuf,
    pub raw_log_path: PathBuf,
}

/// Probes local process liveness from a persisted process id.
pub trait TrainProcessProbe {
    /// Returns true when the operating system reports the process is still running.
    fn is_process_running(&self, pid: u32) -> KernelResult<bool>;
}

/// Supplies timestamps for durable train records.
pub trait TrainClock {
    /// Returns the current UTC timestamp formatted as RFC3339.
    fn now_rfc3339(&self) -> KernelResult<String>;
}

/// Generates run refs from plan identity and start time.
pub trait LoraTrainRunRefGenerator {
    /// Derives a run ref for a new run record.
    fn generate_run_ref(&self, plan_ref: &str, created_at: &str) -> KernelResult<String>;
}

/// Launches hidden detached training workers.
pub trait LoraTrainWorkerLauncher {
    /// Starts the hidden `train lora run-worker` command and returns its pid.
    fn launch_worker(&self, home_dir: &Path, run_ref: &str) -> KernelResult<u32>;
}
