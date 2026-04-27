use std::{
    env, fs,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::{config::LoraTrainPlan, error::TrainError};

const HOME_ENV: &str = "TENTGENT_HOME";
const TRAIN_ENV: &str = "TENTGENT_TRAIN_DIR";

#[derive(Debug, Clone)]
pub struct LoraTrainStorePaths {
    pub plans_dir: PathBuf,
    pub staging_dir: PathBuf,
}

impl LoraTrainStorePaths {
    pub fn resolve() -> Result<Self, TrainError> {
        let home_dir = read_env_path(HOME_ENV).unwrap_or(default_home_dir()?);
        let train_dir = read_env_path(TRAIN_ENV).unwrap_or_else(|| home_dir.join("train"));
        let lora_dir = train_dir.join("lora");

        Ok(Self {
            plans_dir: lora_dir.join("plans"),
            staging_dir: lora_dir.join("staging"),
        })
    }

    pub fn ensure_layout(&self) -> Result<(), TrainError> {
        fs::create_dir_all(&self.plans_dir)?;
        fs::create_dir_all(&self.staging_dir)?;
        Ok(())
    }

    pub fn plan_dir(&self, plan_ref: &str) -> PathBuf {
        self.plans_dir.join(plan_ref)
    }

    pub fn plan_toml_path(&self, plan_ref: &str) -> PathBuf {
        self.plan_dir(plan_ref).join("plan.toml")
    }

    pub fn plan_runs_dir(&self, plan_ref: &str) -> PathBuf {
        self.plan_dir(plan_ref).join("runs")
    }

    pub fn run_dir(&self, plan_ref: &str, run_ref: &str) -> PathBuf {
        self.plan_runs_dir(plan_ref).join(run_ref)
    }

    pub fn run_toml_path(&self, plan_ref: &str, run_ref: &str) -> PathBuf {
        self.run_dir(plan_ref, run_ref).join("run.toml")
    }

    pub fn run_metrics_path(&self, plan_ref: &str, run_ref: &str) -> PathBuf {
        self.run_dir(plan_ref, run_ref).join("metrics.jsonl")
    }

    pub fn run_raw_log_path(&self, plan_ref: &str, run_ref: &str) -> PathBuf {
        self.run_dir(plan_ref, run_ref).join("raw.log")
    }
}

pub fn imported_at_now() -> Result<String, TrainError> {
    Ok(OffsetDateTime::now_utc().format(&Rfc3339)?)
}

pub fn write_lora_train_plan(path: &Path, plan: &LoraTrainPlan) -> Result<(), TrainError> {
    let body = toml::to_string_pretty(plan)?;
    fs::write(path, body)?;
    Ok(())
}

pub fn read_lora_train_plan(path: &Path) -> Result<LoraTrainPlan, TrainError> {
    let body = fs::read_to_string(path)?;
    toml::from_str(&body).map_err(|err| TrainError::MetadataParse {
        path: path.to_path_buf(),
        message: err.to_string(),
    })
}

fn read_env_path(name: &str) -> Option<PathBuf> {
    let value = env::var(name).ok()?;
    let trimmed = value.trim();

    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

fn default_home_dir() -> Result<PathBuf, TrainError> {
    let project_dirs = ProjectDirs::from("com", "tentserv", "tentgent")
        .ok_or(TrainError::ProjectDirsUnavailable)?;
    Ok(project_dirs.data_local_dir().to_path_buf())
}
