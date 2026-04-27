use std::fmt;

use serde::{Deserialize, Serialize};

pub const LORA_TRAIN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoraTrainBackendRequest {
    Auto,
    Mlx,
    Peft,
}

impl LoraTrainBackendRequest {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Mlx => "mlx",
            Self::Peft => "peft",
        }
    }
}

impl fmt::Display for LoraTrainBackendRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoraTrainBackend {
    Mlx,
    Peft,
}

impl LoraTrainBackend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mlx => "mlx",
            Self::Peft => "peft",
        }
    }
}

impl fmt::Display for LoraTrainBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrainPlanStatus {
    Ready,
    Blocked,
}

impl TrainPlanStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoraTrainPlan {
    pub schema_version: u32,
    pub plan_ref: String,
    pub short_ref: String,
    pub name: Option<String>,
    pub status: TrainPlanStatus,
    pub created_at: String,
    pub model_ref: String,
    pub model_short_ref: String,
    pub dataset_ref: String,
    pub dataset_short_ref: String,
    pub requested_backend: LoraTrainBackendRequest,
    pub backend: Option<LoraTrainBackend>,
    pub profile: String,
    pub selection_reason: String,
    #[serde(default)]
    pub blockers: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub model: LoraTrainModelConfig,
    pub dataset: LoraTrainDatasetConfig,
    pub lora: LoraConfig,
    pub optimization: OptimizationConfig,
    pub checkpoint: CheckpointConfig,
    pub output: OutputConfig,
    pub backend_config: LoraBackendConfig,
    pub command_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoraTrainModelConfig {
    pub primary_format: String,
    pub total_bytes: u64,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoraTrainDatasetConfig {
    pub format: String,
    pub train_split: Option<String>,
    pub validation_split: Option<String>,
    pub train_examples: Option<usize>,
    pub validation_examples: Option<usize>,
    pub source_path: String,
    pub max_seq_length: u32,
    pub mask_prompt: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoraConfig {
    pub rank: u32,
    pub alpha: Option<u32>,
    pub dropout: f32,
    pub scale: Option<f32>,
    #[serde(default)]
    pub target_modules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationConfig {
    pub optimizer: String,
    pub learning_rate: f64,
    pub batch_size: u32,
    pub gradient_accumulation_steps: u32,
    pub max_steps: u32,
    pub warmup_steps: u32,
    pub weight_decay: f64,
    pub seed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointConfig {
    pub log_every_steps: u32,
    pub eval_every_steps: u32,
    pub save_every_steps: u32,
    pub save_total_limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    pub adapter_name: String,
    pub adapter_output_template: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoraBackendConfig {
    pub mlx: Option<MlxBackendConfig>,
    pub peft: Option<PeftBackendConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlxBackendConfig {
    pub fine_tune_type: String,
    pub num_layers: u32,
    pub grad_checkpoint: bool,
    pub val_batches: u32,
    pub test_batches: u32,
    pub resume_adapter_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeftBackendConfig {
    pub torch_dtype: String,
    pub device_map: String,
    pub load_in_4bit: bool,
    pub load_in_8bit: bool,
    pub gradient_checkpointing: bool,
    pub bf16: bool,
    pub fp16: bool,
    pub save_safetensors: bool,
}
