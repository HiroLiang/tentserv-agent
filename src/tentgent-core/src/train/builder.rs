use std::{
    fs,
    io::{BufRead, BufReader},
    path::Path,
};

use crate::{dataset::DatasetManager, model::ModelManager};

use super::{
    config::{
        LoraTrainBackendRequest, LoraTrainDatasetConfig, LoraTrainModelConfig, LoraTrainPlan,
        OutputConfig, TrainPlanStatus, LORA_TRAIN_SCHEMA_VERSION,
    },
    defaults::{default_plan_defaults, select_backend, select_profile},
    error::TrainError,
    overrides::LoraTrainOverrides,
    store::imported_at_now,
};

pub fn build_plan(
    model_manager: &ModelManager,
    dataset_manager: &DatasetManager,
    model_reference: &str,
    dataset_reference: &str,
    requested_backend: LoraTrainBackendRequest,
    name: Option<String>,
    overrides: LoraTrainOverrides,
) -> Result<LoraTrainPlan, TrainError> {
    let model = model_manager.inspect(model_reference)?;
    let dataset = dataset_manager.inspect(dataset_reference)?;
    let (backend, selection_reason, mut blockers) =
        select_backend(model.metadata.primary_format, requested_backend);
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
        created_at: imported_at_now()?,
        model_ref: model.metadata.model_ref.clone(),
        model_short_ref: model.metadata.short_ref.clone(),
        dataset_ref: dataset.metadata.dataset_ref.clone(),
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
            mask_prompt: false,
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

fn count_split_lines(
    source_path: &Path,
    split: &Option<String>,
) -> Result<Option<usize>, TrainError> {
    let Some(split) = split else {
        return Ok(None);
    };
    let path = source_path.join(split);
    if !path.exists() {
        return Ok(None);
    }

    let reader = BufReader::new(fs::File::open(path)?);
    let mut count = 0;
    for line in reader.lines() {
        if !line?.trim().is_empty() {
            count += 1;
        }
    }
    Ok(Some(count))
}
