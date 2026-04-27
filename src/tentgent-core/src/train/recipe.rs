use serde::Serialize;

use super::{
    config::{
        CheckpointConfig, LoraBackendConfig, LoraConfig, LoraTrainBackend, LoraTrainBackendRequest,
        LoraTrainDatasetConfig, LoraTrainPlan,
    },
    error::TrainError,
    hash,
};

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
    optimization: &'a super::config::OptimizationConfig,
    checkpoint: &'a CheckpointConfig,
    adapter_name: &'a str,
    backend_config: &'a LoraBackendConfig,
}

pub fn compute_plan_ref(plan: &LoraTrainPlan) -> Result<String, TrainError> {
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
    Ok(hash::sha256_bytes(&serde_json::to_vec(&recipe)?))
}

pub fn command_hint(plan: &LoraTrainPlan) -> String {
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
