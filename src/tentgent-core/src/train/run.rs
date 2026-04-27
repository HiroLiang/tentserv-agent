use std::{fmt, fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    config::{LoraTrainBackend, LoraTrainPlan, TrainPlanStatus, LORA_TRAIN_SCHEMA_VERSION},
    error::TrainError,
    store::{imported_at_now, read_lora_train_plan, LoraTrainStorePaths},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoraTrainRunStatus {
    Running,
    Succeeded,
    Failed,
}

impl LoraTrainRunStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

impl fmt::Display for LoraTrainRunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoraTrainRun {
    pub schema_version: u32,
    pub run_ref: String,
    pub short_ref: String,
    pub status: LoraTrainRunStatus,
    pub created_at: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub plan_ref: String,
    pub plan_short_ref: String,
    pub model_ref: String,
    pub dataset_ref: String,
    pub backend: Option<LoraTrainBackend>,
    pub recipe_hash: String,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub exit_signal: Option<String>,
    pub adapter_ref: Option<String>,
    pub adapter_path: Option<String>,
    pub adapter_output_path: Option<String>,
    pub adapter_store_path: Option<String>,
    pub run_dir: String,
    pub run_path: String,
    pub metrics_path: String,
    pub raw_log_path: String,
}

#[derive(Debug, Clone)]
pub struct LoraTrainRunStartOutcome {
    pub plan: LoraTrainPlan,
    pub run: LoraTrainRun,
    pub run_dir: PathBuf,
    pub run_path: PathBuf,
    pub metrics_path: PathBuf,
    pub raw_log_path: PathBuf,
}

pub struct LoraTrainRunManager {
    paths: LoraTrainStorePaths,
}

impl LoraTrainRunManager {
    pub fn new() -> Result<Self, TrainError> {
        let paths = LoraTrainStorePaths::resolve()?;
        paths.ensure_layout()?;
        Ok(Self { paths })
    }

    pub fn start_run(&self, plan_reference: &str) -> Result<LoraTrainRunStartOutcome, TrainError> {
        let plan = self.resolve_plan(plan_reference)?;
        if plan.status == TrainPlanStatus::Blocked {
            return Err(TrainError::PlanBlocked {
                plan_ref: plan.plan_ref,
                reasons: plan.blockers.join("; "),
            });
        }

        let created_at = imported_at_now()?;
        let run_ref = generate_run_ref(&plan.plan_ref, &created_at);
        let short_ref = run_ref.chars().take(12).collect::<String>();
        let run_dir = self.paths.run_dir(&plan.plan_ref, &run_ref);
        let run_path = self.paths.run_toml_path(&plan.plan_ref, &run_ref);
        let metrics_path = self.paths.run_metrics_path(&plan.plan_ref, &run_ref);
        let raw_log_path = self.paths.run_raw_log_path(&plan.plan_ref, &run_ref);

        fs::create_dir_all(&run_dir)?;
        fs::write(&metrics_path, "")?;
        fs::write(&raw_log_path, "")?;

        let run = LoraTrainRun {
            schema_version: LORA_TRAIN_SCHEMA_VERSION,
            run_ref,
            short_ref,
            status: LoraTrainRunStatus::Running,
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
            run_dir: run_dir.display().to_string(),
            run_path: run_path.display().to_string(),
            metrics_path: metrics_path.display().to_string(),
            raw_log_path: raw_log_path.display().to_string(),
        };
        self.write_run(&run)?;

        Ok(LoraTrainRunStartOutcome {
            plan,
            run,
            run_dir,
            run_path,
            metrics_path,
            raw_log_path,
        })
    }

    pub fn write_run(&self, run: &LoraTrainRun) -> Result<(), TrainError> {
        let run_path = self.paths.run_toml_path(&run.plan_ref, &run.run_ref);
        let body = toml::to_string_pretty(run)?;
        fs::write(run_path, body)?;
        Ok(())
    }

    pub fn finish_run(
        &self,
        run: &mut LoraTrainRun,
        status: LoraTrainRunStatus,
        exit_code: Option<i32>,
    ) -> Result<(), TrainError> {
        run.status = status;
        run.ended_at = Some(imported_at_now()?);
        run.exit_code = exit_code;
        self.write_run(run)
    }

    fn resolve_plan(&self, reference: &str) -> Result<LoraTrainPlan, TrainError> {
        let exact_path = self.paths.plan_toml_path(reference);
        if exact_path.exists() {
            return read_lora_train_plan(&exact_path);
        }

        let mut matches = Vec::new();
        for entry in fs::read_dir(&self.paths.plans_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let plan_ref = entry.file_name().to_string_lossy().into_owned();
            if plan_ref.starts_with(reference) {
                matches.push(read_lora_train_plan(&self.paths.plan_toml_path(&plan_ref))?);
            }
        }

        match matches.len() {
            0 => Err(TrainError::PlanNotFound(reference.to_string())),
            1 => Ok(matches.remove(0)),
            _ => Err(TrainError::AmbiguousPlanRef(reference.to_string())),
        }
    }
}

fn generate_run_ref(plan_ref: &str, created_at: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plan_ref.as_bytes());
    hasher.update(b"\0");
    hasher.update(created_at.as_bytes());
    hasher.update(b"\0");
    hasher.update(std::process::id().to_string().as_bytes());
    hex::encode(hasher.finalize())
}
