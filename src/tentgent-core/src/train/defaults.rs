use crate::model::ModelFormat;

use super::config::{
    CheckpointConfig, LoraBackendConfig, LoraConfig, LoraTrainBackend, LoraTrainBackendRequest,
    MlxBackendConfig, OptimizationConfig, PeftBackendConfig,
};

#[derive(Debug)]
pub struct PlanDefaults {
    pub max_seq_length: u32,
    pub lora: LoraConfig,
    pub optimization: OptimizationConfig,
    pub checkpoint: CheckpointConfig,
    pub backend_config: LoraBackendConfig,
}

pub fn select_backend(
    model_format: ModelFormat,
    requested_backend: LoraTrainBackendRequest,
) -> (Option<LoraTrainBackend>, String, Vec<String>) {
    match requested_backend {
        LoraTrainBackendRequest::Auto => match model_format {
            ModelFormat::Mlx => (
                Some(LoraTrainBackend::Mlx),
                "model primary_format is mlx".to_string(),
                Vec::new(),
            ),
            ModelFormat::Safetensors => (
                Some(LoraTrainBackend::Peft),
                "model primary_format is safetensors".to_string(),
                Vec::new(),
            ),
            ModelFormat::Gguf => (
                None,
                "model primary_format is gguf".to_string(),
                vec!["GGUF models are inference artifacts and are not trainable by Tentgent LoRA MVP".to_string()],
            ),
        },
        LoraTrainBackendRequest::Mlx if model_format == ModelFormat::Mlx => (
            Some(LoraTrainBackend::Mlx),
            "`--backend mlx` requested and model primary_format is mlx".to_string(),
            Vec::new(),
        ),
        LoraTrainBackendRequest::Mlx => (
            None,
            "`--backend mlx` requested".to_string(),
            vec![format!(
                "`--backend mlx` requires model primary_format mlx; got {}",
                model_format.as_str()
            )],
        ),
        LoraTrainBackendRequest::Peft if model_format == ModelFormat::Safetensors => (
            Some(LoraTrainBackend::Peft),
            "`--backend peft` requested and model primary_format is safetensors".to_string(),
            Vec::new(),
        ),
        LoraTrainBackendRequest::Peft => (
            None,
            "`--backend peft` requested".to_string(),
            vec![format!(
                "`--backend peft` requires model primary_format safetensors; got {}",
                model_format.as_str()
            )],
        ),
    }
}

pub fn select_profile(model_total_bytes: u64, train_examples: Option<usize>) -> &'static str {
    if model_total_bytes >= 2 * 1024 * 1024 * 1024 {
        "auto-lowmem"
    } else if train_examples.is_some_and(|count| count <= 100) {
        "auto-small-data"
    } else {
        "auto-default"
    }
}

pub fn default_plan_defaults(backend: Option<LoraTrainBackend>, profile: &str) -> PlanDefaults {
    let lowmem = profile == "auto-lowmem";
    let conservative = lowmem || profile == "auto-small-data";
    let rank = if conservative { 8 } else { 16 };

    PlanDefaults {
        max_seq_length: if conservative { 1024 } else { 2048 },
        lora: LoraConfig {
            rank,
            alpha: backend
                .filter(|value| *value == LoraTrainBackend::Peft)
                .map(|_| rank * 2),
            dropout: if backend == Some(LoraTrainBackend::Mlx) {
                0.0
            } else {
                0.05
            },
            scale: backend
                .filter(|value| *value == LoraTrainBackend::Mlx)
                .map(|_| 20.0),
            target_modules: default_target_modules(backend),
        },
        optimization: OptimizationConfig {
            optimizer: "adamw".to_string(),
            learning_rate: if conservative { 0.00008 } else { 0.0002 },
            batch_size: if conservative { 1 } else { 4 },
            gradient_accumulation_steps: if conservative { 16 } else { 4 },
            max_steps: if conservative { 320 } else { 1200 },
            warmup_steps: if conservative { 30 } else { 100 },
            weight_decay: 0.0,
            seed: 42,
        },
        checkpoint: CheckpointConfig {
            log_every_steps: 10,
            eval_every_steps: if conservative { 40 } else { 100 },
            save_every_steps: if conservative { 80 } else { 100 },
            save_total_limit: 3,
        },
        backend_config: default_backend_config(backend, lowmem),
    }
}

fn default_target_modules(backend: Option<LoraTrainBackend>) -> Vec<String> {
    if backend != Some(LoraTrainBackend::Peft) {
        return Vec::new();
    }

    [
        "q_proj",
        "k_proj",
        "v_proj",
        "o_proj",
        "gate_proj",
        "up_proj",
        "down_proj",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn default_backend_config(backend: Option<LoraTrainBackend>, lowmem: bool) -> LoraBackendConfig {
    match backend {
        Some(LoraTrainBackend::Mlx) => LoraBackendConfig {
            mlx: Some(MlxBackendConfig {
                fine_tune_type: "lora".to_string(),
                num_layers: if lowmem { 8 } else { 16 },
                grad_checkpoint: lowmem,
                val_batches: 25,
                test_batches: 500,
                resume_adapter_ref: None,
            }),
            peft: None,
        },
        Some(LoraTrainBackend::Peft) => LoraBackendConfig {
            mlx: None,
            peft: Some(PeftBackendConfig {
                torch_dtype: "auto".to_string(),
                device_map: "auto".to_string(),
                load_in_4bit: false,
                load_in_8bit: false,
                gradient_checkpointing: true,
                bf16: false,
                fp16: false,
                save_safetensors: true,
            }),
        },
        None => LoraBackendConfig::default(),
    }
}
