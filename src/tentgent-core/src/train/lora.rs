use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{dataset::DatasetManager, model::ModelManager};

use super::{
    builder::build_plan,
    config::{LoraTrainBackendRequest, LoraTrainPlan},
    error::TrainError,
    overrides::LoraTrainOverrides,
    recipe::{command_hint, compute_plan_ref},
    records::{
        LoraTrainPlanCreateOutcome, LoraTrainPlanInspection, LoraTrainPlanPreviewOutcome,
        LoraTrainPlanRemovalOutcome, LoraTrainPlanSummary,
    },
    store::{read_lora_train_plan, write_lora_train_plan, LoraTrainStorePaths},
};

pub struct LoraTrainPlanManager {
    paths: LoraTrainStorePaths,
    model_manager: ModelManager,
    dataset_manager: DatasetManager,
}

impl LoraTrainPlanManager {
    pub fn new() -> Result<Self, TrainError> {
        let paths = LoraTrainStorePaths::resolve()?;
        Self::from_paths(paths, None, true)
    }

    pub fn new_with_home(home_override: Option<&Path>) -> Result<Self, TrainError> {
        let paths = LoraTrainStorePaths::resolve_with_home(home_override)?;
        Self::from_paths(paths, home_override, true)
    }

    pub fn open_readonly_with_home(home_override: Option<&Path>) -> Result<Self, TrainError> {
        let paths = LoraTrainStorePaths::resolve_with_home(home_override)?;
        Self::from_paths(paths, home_override, false)
    }

    fn from_paths(
        paths: LoraTrainStorePaths,
        home_override: Option<&Path>,
        ensure_layout: bool,
    ) -> Result<Self, TrainError> {
        if ensure_layout {
            paths.ensure_layout()?;
        }
        let model_manager = if ensure_layout {
            ModelManager::new_with_home(home_override)?
        } else {
            ModelManager::open_readonly_with_home(home_override)?
        };
        let dataset_manager = if ensure_layout {
            DatasetManager::new_with_home(home_override)?
        } else {
            DatasetManager::open_readonly_with_home(home_override)?
        };
        Ok(Self {
            model_manager,
            dataset_manager,
            paths,
        })
    }

    pub fn create_plan(
        &self,
        model_reference: &str,
        dataset_reference: &str,
        requested_backend: LoraTrainBackendRequest,
        name: Option<String>,
        overrides: LoraTrainOverrides,
    ) -> Result<LoraTrainPlanCreateOutcome, TrainError> {
        let PreparedPlan {
            plan,
            plan_dir,
            plan_path,
        } = self.prepare_plan(
            model_reference,
            dataset_reference,
            requested_backend,
            name,
            overrides,
        )?;

        if plan_path.exists() {
            let plan = read_lora_train_plan(&plan_path)?;
            return Ok(LoraTrainPlanCreateOutcome {
                run_count: self.count_runs(&plan.plan_ref)?,
                plan,
                plan_dir,
                plan_path,
                deduplicated: true,
            });
        }

        fs::create_dir_all(self.paths.plan_runs_dir(&plan.plan_ref))?;
        write_lora_train_plan(&plan_path, &plan)?;

        Ok(LoraTrainPlanCreateOutcome {
            plan,
            plan_dir,
            plan_path,
            deduplicated: false,
            run_count: 0,
        })
    }

    pub fn preview_plan(
        &self,
        model_reference: &str,
        dataset_reference: &str,
        requested_backend: LoraTrainBackendRequest,
        name: Option<String>,
        overrides: LoraTrainOverrides,
    ) -> Result<LoraTrainPlanPreviewOutcome, TrainError> {
        let PreparedPlan {
            plan,
            plan_dir,
            plan_path,
        } = self.prepare_plan(
            model_reference,
            dataset_reference,
            requested_backend,
            name,
            overrides,
        )?;

        if plan_path.exists() {
            let plan = read_lora_train_plan(&plan_path)?;
            return Ok(LoraTrainPlanPreviewOutcome {
                run_count: self.count_runs(&plan.plan_ref)?,
                plan,
                plan_dir,
                plan_path,
                would_reuse: true,
            });
        }

        Ok(LoraTrainPlanPreviewOutcome {
            plan,
            plan_dir,
            plan_path,
            would_reuse: false,
            run_count: 0,
        })
    }

    pub fn list_plans(&self) -> Result<Vec<LoraTrainPlanSummary>, TrainError> {
        let mut plans = Vec::new();
        if !self.paths.plans_dir.exists() {
            return Ok(plans);
        }

        for entry in fs::read_dir(&self.paths.plans_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let plan_ref = entry.file_name().to_string_lossy().into_owned();
            let plan_path = self.paths.plan_toml_path(&plan_ref);
            if !plan_path.exists() {
                continue;
            }

            let plan = read_lora_train_plan(&plan_path)?;
            plans.push(LoraTrainPlanSummary {
                run_count: self.count_runs(&plan.plan_ref)?,
                plan,
            });
        }

        plans.sort_by(|left, right| left.plan.short_ref.cmp(&right.plan.short_ref));
        Ok(plans)
    }

    pub fn inspect_plan(&self, reference: &str) -> Result<LoraTrainPlanInspection, TrainError> {
        let plan = self.resolve_plan(reference)?;
        let plan_dir = self.paths.plan_dir(&plan.plan_ref);
        let plan_path = self.paths.plan_toml_path(&plan.plan_ref);
        let runs_dir = self.paths.plan_runs_dir(&plan.plan_ref);

        Ok(LoraTrainPlanInspection {
            run_count: self.count_runs(&plan.plan_ref)?,
            plan,
            plan_dir,
            plan_path,
            runs_dir,
        })
    }

    pub fn remove_plan(&self, reference: &str) -> Result<LoraTrainPlanRemovalOutcome, TrainError> {
        let plan = self.resolve_plan(reference)?;
        let plan_dir = self.paths.plan_dir(&plan.plan_ref);
        let run_count = self.count_runs(&plan.plan_ref)?;

        if plan_dir.exists() {
            fs::remove_dir_all(&plan_dir)?;
        }

        Ok(LoraTrainPlanRemovalOutcome {
            plan,
            plan_dir,
            run_count,
        })
    }

    fn prepare_plan(
        &self,
        model_reference: &str,
        dataset_reference: &str,
        requested_backend: LoraTrainBackendRequest,
        name: Option<String>,
        overrides: LoraTrainOverrides,
    ) -> Result<PreparedPlan, TrainError> {
        let mut plan = build_plan(
            &self.model_manager,
            &self.dataset_manager,
            model_reference,
            dataset_reference,
            requested_backend,
            name,
            overrides,
        )?;
        let plan_ref = compute_plan_ref(&plan)?;
        plan.plan_ref = plan_ref.clone();
        plan.short_ref = plan_ref.chars().take(12).collect();
        plan.output.adapter_output_template = self
            .paths
            .plan_runs_dir(&plan_ref)
            .join("<RUN_REF>")
            .join("adapter-output")
            .display()
            .to_string();
        plan.command_hint = command_hint(&plan);

        Ok(PreparedPlan {
            plan_dir: self.paths.plan_dir(&plan_ref),
            plan_path: self.paths.plan_toml_path(&plan_ref),
            plan,
        })
    }

    fn resolve_plan(&self, reference: &str) -> Result<LoraTrainPlan, TrainError> {
        let exact_path = self.paths.plan_toml_path(reference);
        if exact_path.exists() {
            return read_lora_train_plan(&exact_path);
        }

        let mut matches = Vec::new();
        if !self.paths.plans_dir.exists() {
            return Err(TrainError::PlanNotFound(reference.to_string()));
        }
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

    fn count_runs(&self, plan_ref: &str) -> Result<usize, TrainError> {
        let runs_dir = self.paths.plan_runs_dir(plan_ref);
        if !runs_dir.exists() {
            return Ok(0);
        }

        let mut count = 0;
        for entry in fs::read_dir(runs_dir)? {
            if entry?.file_type()?.is_dir() {
                count += 1;
            }
        }
        Ok(count)
    }
}

struct PreparedPlan {
    plan: LoraTrainPlan,
    plan_dir: PathBuf,
    plan_path: PathBuf,
}
