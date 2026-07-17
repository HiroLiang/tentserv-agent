use walkdir::WalkDir;

use crate::{
    features::{
        resource_guard::{blocker, ResourceBlocker, ResourceBlockerCode, ResourceOperation},
        train::domain::{LoraTrainPlan, LoraTrainRun},
    },
    foundation::{
        error::{KernelError, KernelResult},
        layout::RuntimeLayout,
    },
};

pub(crate) fn train_reference_blockers(
    context: &super::ResourceGuardProbeContext<'_>,
    layout: &RuntimeLayout,
    operation: &ResourceOperation,
) -> KernelResult<Vec<ResourceBlocker>> {
    let mut blockers = Vec::new();
    if !layout.train_dir.exists() {
        return Ok(blockers);
    }
    for entry in WalkDir::new(&layout.train_dir).into_iter() {
        let entry = entry.map_err(guard_error)?;
        if !entry.file_type().is_file() {
            continue;
        }
        match entry.file_name().to_str() {
            Some("plan.toml") => {
                let plan: LoraTrainPlan =
                    toml::from_str(&std::fs::read_to_string(entry.path()).map_err(guard_error)?)
                        .map_err(guard_error)?;
                plan_blockers(operation, &plan, &mut blockers);
            }
            Some("run.toml") => {
                let run: LoraTrainRun =
                    toml::from_str(&std::fs::read_to_string(entry.path()).map_err(guard_error)?)
                        .map_err(guard_error)?;
                run_blockers(context, operation, &run, &mut blockers)?;
            }
            _ => {}
        }
    }
    Ok(blockers)
}

fn plan_blockers(
    operation: &ResourceOperation,
    plan: &LoraTrainPlan,
    blockers: &mut Vec<ResourceBlocker>,
) {
    let (code, field, reason) = match operation {
        ResourceOperation::DeleteModel { model_ref } if plan.model_ref == *model_ref => (
            ResourceBlockerCode::ModelInUse,
            "model_ref",
            "LoRA train plan references this model",
        ),
        ResourceOperation::DeleteDataset { dataset_ref } if plan.dataset_ref == *dataset_ref => (
            ResourceBlockerCode::DatasetInUse,
            "dataset_ref",
            "LoRA train plan references this dataset",
        ),
        ResourceOperation::DeleteAdapter { adapter_ref, .. }
        | ResourceOperation::RebindAdapter { adapter_ref, .. }
            if plan
                .backend_config
                .mlx
                .as_ref()
                .and_then(|mlx| mlx.resume_adapter_ref.as_deref())
                == Some(adapter_ref) =>
        {
            (
                if matches!(operation, ResourceOperation::DeleteAdapter { .. }) {
                    ResourceBlockerCode::AdapterInUse
                } else {
                    ResourceBlockerCode::AdapterRebindInUse
                },
                "resume_adapter_ref",
                "LoRA train plan resumes from this adapter",
            )
        }
        _ => return,
    };
    let mut value = blocker(operation, "train-plan", code, &plan.short_ref, reason);
    value.field = Some(field.to_string());
    blockers.push(value);
}

fn run_blockers(
    context: &super::ResourceGuardProbeContext<'_>,
    operation: &ResourceOperation,
    run: &LoraTrainRun,
    blockers: &mut Vec<ResourceBlocker>,
) -> KernelResult<()> {
    match operation {
        ResourceOperation::DeleteModel { model_ref } if run.model_ref == *model_ref => {
            let mut value = blocker(
                operation,
                "train-run",
                ResourceBlockerCode::ModelInUse,
                &run.short_ref,
                "LoRA train run references this model",
            );
            value.field = Some("model_ref".to_string());
            blockers.push(value);
        }
        ResourceOperation::DeleteDataset { dataset_ref } if run.dataset_ref == *dataset_ref => {
            let mut value = blocker(
                operation,
                "train-run",
                ResourceBlockerCode::DatasetInUse,
                &run.short_ref,
                "LoRA train run references this dataset",
            );
            value.field = Some("dataset_ref".to_string());
            blockers.push(value);
        }
        ResourceOperation::DeleteTrainPlan { plan_ref }
            if run.plan_ref == *plan_ref && run.status.is_live() =>
        {
            let live = match run.pid {
                Some(pid) => Some(context.process_probe.is_process_running(pid)?),
                None => None,
            };
            if live != Some(false) {
                let mut value = blocker(
                    operation,
                    "train-run",
                    ResourceBlockerCode::TrainRunActive,
                    &run.short_ref,
                    if live == Some(true) {
                        "training run process is still active"
                    } else {
                        "training run process state cannot be verified"
                    },
                );
                value.field = Some("status".to_string());
                value.next_actions =
                    vec![format!("tentgent train lora run inspect {}", run.short_ref)];
                blockers.push(value);
            }
        }
        _ => {}
    }
    Ok(())
}

fn guard_error(error: impl std::fmt::Display) -> KernelError {
    KernelError::ResourceCoordinationUnavailable(format!(
        "resource guard train probe failed: {error}"
    ))
}
