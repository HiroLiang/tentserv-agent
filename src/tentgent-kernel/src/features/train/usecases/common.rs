use std::{
    fs,
    io::{BufRead, BufReader},
    path::Path,
};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::features::dataset::domain::{DatasetInspection, DatasetStoreLayout};
use crate::features::model::domain::{ModelInspection, ModelStoreLayout};
use crate::features::train::domain::{
    default_plan_defaults, select_backend, select_profile, CheckpointConfig, LoraBackendConfig,
    LoraConfig, LoraTrainBackend, LoraTrainBackendRequest, LoraTrainDatasetConfig,
    LoraTrainModelConfig, LoraTrainOverrides, LoraTrainPlan, OutputConfig, TrainPlanStatus,
    TrainStoreLayout, LORA_TRAIN_SCHEMA_VERSION,
};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayout;
use crate::foundation::platform::PlatformFacts;

pub(super) fn train_store_layout(layout: &RuntimeLayout) -> TrainStoreLayout {
    TrainStoreLayout::from_train_dir(layout.train_dir.clone())
}

pub(super) fn model_store_layout(layout: &RuntimeLayout) -> ModelStoreLayout {
    ModelStoreLayout::from_models_dir(layout.models_dir.clone())
}

pub(super) fn dataset_store_layout(layout: &RuntimeLayout) -> DatasetStoreLayout {
    DatasetStoreLayout::from_datasets_dir(layout.datasets_dir.clone())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_lora_train_plan(
    platform: &PlatformFacts,
    model: &ModelInspection,
    dataset: &DatasetInspection,
    requested_backend: LoraTrainBackendRequest,
    name: Option<String>,
    overrides: LoraTrainOverrides,
    created_at: String,
) -> KernelResult<LoraTrainPlan> {
    let (backend, selection_reason, mut blockers) =
        select_backend(platform, model.metadata.primary_format, requested_backend);
    let mut warnings = dataset.metadata.package.warnings.clone();

    if !dataset.metadata.package.tuning_ready {
        blockers.push("dataset is not tuning-ready; expected a root `train.jsonl`".to_string());
    }
    if dataset.metadata.package.splits.train.is_none() {
        blockers.push("dataset has no train split".to_string());
    }
    if dataset.metadata.package.splits.validation.is_none() {
        warnings.push(
            "dataset has no validation split; validation loss will be unavailable".to_string(),
        );
    }

    let train_examples =
        count_split_lines(&dataset.source_path, &dataset.metadata.package.splits.train)?;
    let validation_examples = count_split_lines(
        &dataset.source_path,
        &dataset.metadata.package.splits.validation,
    )?;
    let profile = select_profile(model.metadata.total_bytes, train_examples);
    let defaults = default_plan_defaults(backend, profile);

    let mut plan = LoraTrainPlan {
        schema_version: LORA_TRAIN_SCHEMA_VERSION,
        plan_ref: String::new(),
        short_ref: String::new(),
        name,
        status: if blockers.is_empty() {
            TrainPlanStatus::Ready
        } else {
            TrainPlanStatus::Blocked
        },
        created_at,
        model_ref: model.metadata.model_ref.to_string(),
        model_short_ref: model.metadata.short_ref.clone(),
        dataset_ref: dataset.metadata.dataset_ref.to_string(),
        dataset_short_ref: dataset.metadata.short_ref.clone(),
        requested_backend,
        backend,
        profile: profile.to_string(),
        selection_reason,
        blockers,
        warnings,
        model: LoraTrainModelConfig {
            primary_format: model.metadata.primary_format.as_str().to_string(),
            total_bytes: model.metadata.total_bytes,
            source_path: model.variant_source_path.display().to_string(),
        },
        dataset: LoraTrainDatasetConfig {
            format: dataset.metadata.dataset_format.as_str().to_string(),
            train_split: dataset.metadata.package.splits.train.clone(),
            validation_split: dataset.metadata.package.splits.validation.clone(),
            train_examples,
            validation_examples,
            source_path: dataset.source_path.display().to_string(),
            max_seq_length: defaults.max_seq_length,
            mask_prompt: true,
        },
        lora: defaults.lora,
        optimization: defaults.optimization,
        checkpoint: defaults.checkpoint,
        output: OutputConfig {
            adapter_name: format!(
                "lora-{}-{}",
                model.metadata.short_ref, dataset.metadata.short_ref
            ),
            adapter_output_template: String::new(),
        },
        backend_config: defaults.backend_config,
        command_hint: String::new(),
    };
    overrides.apply_to(&mut plan);
    Ok(plan)
}

pub(super) fn finalize_plan_identity(
    store: &TrainStoreLayout,
    plan: &mut LoraTrainPlan,
) -> KernelResult<()> {
    let plan_ref = compute_plan_ref(plan)?;
    plan.plan_ref = plan_ref.clone();
    plan.short_ref = plan_ref.chars().take(12).collect();
    plan.output.adapter_output_template = store
        .plan_runs_dir(&plan_ref)
        .join("<RUN_REF>")
        .join("adapter-output")
        .display()
        .to_string();
    plan.command_hint = command_hint(plan);
    Ok(())
}

pub(super) fn train_store_error(message: impl Into<String>) -> KernelError {
    KernelError::TrainStoreUnavailable(message.into())
}

#[derive(Serialize)]
struct PlanRecipe<'a> {
    schema_version: u32,
    model_ref: &'a str,
    dataset_ref: &'a str,
    requested_backend: LoraTrainBackendRequest,
    backend: Option<LoraTrainBackend>,
    profile: &'a str,
    dataset: &'a LoraTrainDatasetConfig,
    lora: &'a LoraConfig,
    optimization: &'a crate::features::train::domain::OptimizationConfig,
    checkpoint: &'a CheckpointConfig,
    adapter_name: &'a str,
    backend_config: &'a LoraBackendConfig,
}

fn compute_plan_ref(plan: &LoraTrainPlan) -> KernelResult<String> {
    let recipe = PlanRecipe {
        schema_version: plan.schema_version,
        model_ref: &plan.model_ref,
        dataset_ref: &plan.dataset_ref,
        requested_backend: plan.requested_backend,
        backend: plan.backend,
        profile: &plan.profile,
        dataset: &plan.dataset,
        lora: &plan.lora,
        optimization: &plan.optimization,
        checkpoint: &plan.checkpoint,
        adapter_name: &plan.output.adapter_name,
        backend_config: &plan.backend_config,
    };
    let bytes = serde_json::to_vec(&recipe)
        .map_err(|err| train_store_error(format!("serialize train plan recipe failed: {err}")))?;
    Ok(sha256_bytes(&bytes))
}

fn command_hint(plan: &LoraTrainPlan) -> String {
    match plan.backend {
        Some(LoraTrainBackend::Mlx) => format!(
            "mlx_lm.lora --model {} --train --data {} --adapter-path {} --iters {} --batch-size {} --learning-rate {}",
            plan.model.source_path,
            plan.dataset.source_path,
            plan.output.adapter_output_template,
            plan.optimization.max_steps,
            plan.optimization.batch_size,
            plan.optimization.learning_rate
        ),
        Some(LoraTrainBackend::Peft) => format!(
            "transformers-peft training harness --model {} --data {} --adapter-output {} --max-steps {} --learning-rate {}",
            plan.model.source_path,
            plan.dataset.source_path,
            plan.output.adapter_output_template,
            plan.optimization.max_steps,
            plan.optimization.learning_rate
        ),
        None => "(blocked before backend command selection)".to_string(),
    }
}

fn count_split_lines(source_path: &Path, split: &Option<String>) -> KernelResult<Option<usize>> {
    let Some(split) = split else {
        return Ok(None);
    };
    let path = source_path.join(split);
    if !path.exists() {
        return Ok(None);
    }

    let file = fs::File::open(&path).map_err(|err| {
        train_store_error(format!("open split `{}` failed: {err}", path.display()))
    })?;
    let reader = BufReader::new(file);
    let mut count = 0;
    for line in reader.lines() {
        if !line
            .map_err(|err| {
                train_store_error(format!("read split `{}` failed: {err}", path.display()))
            })?
            .trim()
            .is_empty()
        {
            count += 1;
        }
    }
    Ok(Some(count))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(digest)
}
