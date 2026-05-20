//! Training feature domain types and pure LoRA planning rules.

use std::fmt;
use std::path::PathBuf;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use crate::features::model::domain::ModelFormat;
use crate::foundation::platform::{Architecture, OperatingSystem, PlatformFacts};

pub const LORA_TRAIN_SCHEMA_VERSION: u32 = 1;
pub const TRAIN_REF_HEX_LENGTH: usize = 64;
pub const SHORT_TRAIN_REF_LENGTH: usize = 12;

pub const LORA_DIRNAME: &str = "lora";
pub const PLANS_DIRNAME: &str = "plans";
pub const RUNS_DIRNAME: &str = "runs";
pub const STAGING_DIRNAME: &str = "staging";
pub const PLAN_FILENAME: &str = "plan.toml";
pub const RUN_FILENAME: &str = "run.toml";
pub const METRICS_FILENAME: &str = "metrics.jsonl";
pub const RAW_LOG_FILENAME: &str = "raw.log";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TrainRef(String);

impl TrainRef {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, TrainRefParseError> {
        let normalized = normalize_hex_ref(value.as_ref())?;
        if normalized.len() != TRAIN_REF_HEX_LENGTH {
            return Err(TrainRefParseError::InvalidFullLength {
                actual: normalized.len(),
            });
        }

        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn short_ref(&self) -> &str {
        &self.0[..SHORT_TRAIN_REF_LENGTH]
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for TrainRef {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for TrainRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for TrainRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TrainRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TrainRefSelector(String);

impl TrainRefSelector {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, TrainRefParseError> {
        let normalized = normalize_hex_ref(value.as_ref())?;
        if normalized.len() > TRAIN_REF_HEX_LENGTH {
            return Err(TrainRefParseError::PrefixTooLong {
                actual: normalized.len(),
            });
        }

        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_full_ref(&self) -> bool {
        self.0.len() == TRAIN_REF_HEX_LENGTH
    }
}

impl AsRef<str> for TrainRefSelector {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for TrainRefSelector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for TrainRefSelector {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TrainRefSelector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TrainRefParseError {
    #[error("train reference is empty")]
    Empty,
    #[error("train reference must be exactly 64 hexadecimal characters; got {actual}")]
    InvalidFullLength { actual: usize },
    #[error("train reference prefix must be at most 64 hexadecimal characters; got {actual}")]
    PrefixTooLong { actual: usize },
    #[error("train reference must contain only hexadecimal characters")]
    NonHex,
}

fn normalize_hex_ref(value: &str) -> Result<String, TrainRefParseError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(TrainRefParseError::Empty);
    }

    if !trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(TrainRefParseError::NonHex);
    }

    Ok(trimmed.to_ascii_lowercase())
}

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
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
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
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoraTrainModelConfig {
    pub primary_format: String,
    pub total_bytes: u64,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoraConfig {
    pub rank: u32,
    pub alpha: Option<u32>,
    pub dropout: f32,
    pub scale: Option<f32>,
    #[serde(default)]
    pub target_modules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointConfig {
    pub log_every_steps: u32,
    pub eval_every_steps: u32,
    pub save_every_steps: u32,
    pub save_total_limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutputConfig {
    pub adapter_name: String,
    pub adapter_output_template: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LoraBackendConfig {
    pub mlx: Option<MlxBackendConfig>,
    pub peft: Option<PeftBackendConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MlxBackendConfig {
    pub fine_tune_type: String,
    pub num_layers: u32,
    pub grad_checkpoint: bool,
    pub val_batches: u32,
    pub test_batches: u32,
    pub resume_adapter_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoraTrainOverrides {
    pub max_seq_length: Option<u32>,
    pub mask_prompt: Option<bool>,
    pub rank: Option<u32>,
    pub learning_rate: Option<f64>,
    pub batch_size: Option<u32>,
    pub gradient_accumulation_steps: Option<u32>,
    pub max_steps: Option<u32>,
    pub seed: Option<u64>,
    pub mlx_num_layers: Option<u32>,
    pub mlx_grad_checkpoint: Option<bool>,
    pub peft_load_in_4bit: Option<bool>,
    pub peft_load_in_8bit: Option<bool>,
}

impl LoraTrainOverrides {
    pub fn apply_to(self, plan: &mut LoraTrainPlan) {
        if let Some(value) = self.max_seq_length {
            plan.dataset.max_seq_length = value;
        }
        if let Some(value) = self.mask_prompt {
            plan.dataset.mask_prompt = value;
        }
        if let Some(value) = self.rank {
            plan.lora.rank = value;
            if plan.backend == Some(LoraTrainBackend::Peft) {
                plan.lora.alpha = Some(value * 2);
            }
        }
        if let Some(value) = self.learning_rate {
            plan.optimization.learning_rate = value;
        }
        if let Some(value) = self.batch_size {
            plan.optimization.batch_size = value;
        }
        if let Some(value) = self.gradient_accumulation_steps {
            plan.optimization.gradient_accumulation_steps = value;
        }
        if let Some(value) = self.max_steps {
            plan.optimization.max_steps = value;
        }
        if let Some(value) = self.seed {
            plan.optimization.seed = value;
        }

        let has_mlx_overrides = self.mlx_num_layers.is_some() || self.mlx_grad_checkpoint.is_some();
        let has_peft_overrides =
            self.peft_load_in_4bit.is_some() || self.peft_load_in_8bit.is_some();

        if let Some(mlx) = plan.backend_config.mlx.as_mut() {
            if let Some(value) = self.mlx_num_layers {
                mlx.num_layers = value;
            }
            if let Some(value) = self.mlx_grad_checkpoint {
                mlx.grad_checkpoint = value;
            }
        } else if has_mlx_overrides {
            plan.warnings
                .push("MLX override ignored because selected backend is not mlx".to_string());
        }

        if let Some(peft) = plan.backend_config.peft.as_mut() {
            if let Some(value) = self.peft_load_in_4bit {
                peft.load_in_4bit = value;
            }
            if let Some(value) = self.peft_load_in_8bit {
                peft.load_in_8bit = value;
            }
            if peft.load_in_4bit && peft.load_in_8bit {
                plan.status = TrainPlanStatus::Blocked;
                plan.blockers
                    .push("PEFT cannot load in both 4-bit and 8-bit modes".to_string());
            }
        } else if has_peft_overrides {
            plan.warnings
                .push("PEFT override ignored because selected backend is not peft".to_string());
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoraTrainPlanCreateOutcome {
    pub plan: LoraTrainPlan,
    pub plan_dir: PathBuf,
    pub plan_path: PathBuf,
    pub deduplicated: bool,
    pub run_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoraTrainPlanPreviewOutcome {
    pub plan: LoraTrainPlan,
    pub plan_dir: PathBuf,
    pub plan_path: PathBuf,
    pub would_reuse: bool,
    pub run_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoraTrainPlanSummary {
    pub plan: LoraTrainPlan,
    pub run_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoraTrainPlanInspection {
    pub plan: LoraTrainPlan,
    pub plan_dir: PathBuf,
    pub plan_path: PathBuf,
    pub runs_dir: PathBuf,
    pub run_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoraTrainPlanRemovalOutcome {
    pub plan: LoraTrainPlan,
    pub plan_dir: PathBuf,
    pub run_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoraTrainRunStatus {
    Starting,
    Running,
    Succeeded,
    Failed,
}

impl LoraTrainRunStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    pub const fn is_live(self) -> bool {
        matches!(self, Self::Starting | Self::Running)
    }
}

impl fmt::Display for LoraTrainRunStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoraTrainRun {
    pub schema_version: u32,
    pub run_ref: String,
    pub short_ref: String,
    pub status: LoraTrainRunStatus,
    pub phase: Option<String>,
    pub error: Option<String>,
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

#[derive(Debug, Clone, PartialEq)]
pub struct LoraTrainRunStartOutcome {
    pub plan: LoraTrainPlan,
    pub run: LoraTrainRun,
    pub run_dir: PathBuf,
    pub run_path: PathBuf,
    pub metrics_path: PathBuf,
    pub raw_log_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoraTrainRunInspection {
    pub plan: LoraTrainPlan,
    pub run: LoraTrainRun,
    pub run_dir: PathBuf,
    pub run_path: PathBuf,
    pub metrics_path: PathBuf,
    pub raw_log_path: PathBuf,
    pub process_running: bool,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoraTrainMetricsTail {
    pub metrics_path: PathBuf,
    pub tail: usize,
    pub total_events: usize,
    pub truncated: bool,
    pub events: Vec<IndexedMetricEvent>,
    pub warnings: Vec<TrainRunWarning>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedMetricEvent {
    pub index: usize,
    pub event: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainRunWarning {
    pub code: String,
    pub message: String,
    pub line: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainRunLogMetadata {
    pub path: PathBuf,
    pub exists: bool,
    pub total_bytes: u64,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainRunLogTail {
    pub metadata: TrainRunLogMetadata,
    pub tail_bytes: u64,
    pub truncated: bool,
    pub encoding: &'static str,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainStoreLayout {
    pub train_dir: PathBuf,
    pub lora_dir: PathBuf,
    pub plans_dir: PathBuf,
    pub staging_dir: PathBuf,
}

impl TrainStoreLayout {
    pub fn from_train_dir(train_dir: impl Into<PathBuf>) -> Self {
        let train_dir = train_dir.into();
        let lora_dir = train_dir.join(LORA_DIRNAME);

        Self {
            plans_dir: lora_dir.join(PLANS_DIRNAME),
            staging_dir: lora_dir.join(STAGING_DIRNAME),
            train_dir,
            lora_dir,
        }
    }

    pub fn plan_dir(&self, plan_ref: &str) -> PathBuf {
        self.plans_dir.join(plan_ref)
    }

    pub fn plan_toml_path(&self, plan_ref: &str) -> PathBuf {
        self.plan_dir(plan_ref).join(PLAN_FILENAME)
    }

    pub fn plan_runs_dir(&self, plan_ref: &str) -> PathBuf {
        self.plan_dir(plan_ref).join(RUNS_DIRNAME)
    }

    pub fn run_dir(&self, plan_ref: &str, run_ref: &str) -> PathBuf {
        self.plan_runs_dir(plan_ref).join(run_ref)
    }

    pub fn run_toml_path(&self, plan_ref: &str, run_ref: &str) -> PathBuf {
        self.run_dir(plan_ref, run_ref).join(RUN_FILENAME)
    }

    pub fn run_metrics_path(&self, plan_ref: &str, run_ref: &str) -> PathBuf {
        self.run_dir(plan_ref, run_ref).join(METRICS_FILENAME)
    }

    pub fn run_raw_log_path(&self, plan_ref: &str, run_ref: &str) -> PathBuf {
        self.run_dir(plan_ref, run_ref).join(RAW_LOG_FILENAME)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanDefaults {
    pub max_seq_length: u32,
    pub lora: LoraConfig,
    pub optimization: OptimizationConfig,
    pub checkpoint: CheckpointConfig,
    pub backend_config: LoraBackendConfig,
}

pub fn select_backend(
    platform: &PlatformFacts,
    model_format: ModelFormat,
    requested_backend: LoraTrainBackendRequest,
) -> (Option<LoraTrainBackend>, String, Vec<String>) {
    let mlx_supported = is_mlx_supported(platform);
    match requested_backend {
        LoraTrainBackendRequest::Auto => match model_format {
            ModelFormat::Mlx if !mlx_supported => (
                None,
                "model primary_format is mlx but current platform does not support MLX".to_string(),
                vec!["unsupported: MLX requires Apple Silicon macOS".to_string()],
            ),
            ModelFormat::Mlx => (
                Some(LoraTrainBackend::Mlx),
                "model primary_format is mlx and current platform supports MLX".to_string(),
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
                vec![
                    "GGUF models are inference artifacts and are not trainable by Tentgent LoRA MVP"
                        .to_string(),
                ],
            ),
            ModelFormat::Diffusers => (
                None,
                "model primary_format is diffusers".to_string(),
                vec![
                    "Diffusers image generation models are not trainable by Tentgent LoRA MVP yet"
                        .to_string(),
                ],
            ),
        },
        LoraTrainBackendRequest::Mlx if model_format != ModelFormat::Mlx => (
            None,
            "`--backend mlx` requested".to_string(),
            vec![format!(
                "`--backend mlx` requires model primary_format mlx; got {}",
                model_format.as_str()
            )],
        ),
        LoraTrainBackendRequest::Mlx if !mlx_supported => (
            None,
            "`--backend mlx` requested".to_string(),
            vec!["unsupported: MLX requires Apple Silicon macOS".to_string()],
        ),
        LoraTrainBackendRequest::Mlx => (
            Some(LoraTrainBackend::Mlx),
            "`--backend mlx` requested, model primary_format is mlx, and current platform supports MLX"
                .to_string(),
            Vec::new(),
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

fn is_mlx_supported(platform: &PlatformFacts) -> bool {
    platform.os == OperatingSystem::Macos && platform.arch == Architecture::Aarch64
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
