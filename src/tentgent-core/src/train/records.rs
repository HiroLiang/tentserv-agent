use std::path::PathBuf;

use super::config::LoraTrainPlan;

#[derive(Debug, Clone)]
pub struct LoraTrainPlanCreateOutcome {
    pub plan: LoraTrainPlan,
    pub plan_dir: PathBuf,
    pub plan_path: PathBuf,
    pub deduplicated: bool,
    pub run_count: usize,
}

#[derive(Debug, Clone)]
pub struct LoraTrainPlanPreviewOutcome {
    pub plan: LoraTrainPlan,
    pub plan_dir: PathBuf,
    pub plan_path: PathBuf,
    pub would_reuse: bool,
    pub run_count: usize,
}

#[derive(Debug, Clone)]
pub struct LoraTrainPlanSummary {
    pub plan: LoraTrainPlan,
    pub run_count: usize,
}

#[derive(Debug, Clone)]
pub struct LoraTrainPlanInspection {
    pub plan: LoraTrainPlan,
    pub plan_dir: PathBuf,
    pub plan_path: PathBuf,
    pub runs_dir: PathBuf,
    pub run_count: usize,
}

#[derive(Debug, Clone)]
pub struct LoraTrainPlanRemovalOutcome {
    pub plan: LoraTrainPlan,
    pub plan_dir: PathBuf,
    pub run_count: usize,
}
