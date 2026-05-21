//! Adapter store identity, metadata, and pure compatibility rules.

use std::path::{Path, PathBuf};

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use crate::features::model::domain::{ModelCapability, ModelRef};

pub const ADAPTER_REF_HEX_LENGTH: usize = 64;
pub const SHORT_ADAPTER_REF_LENGTH: usize = 12;

pub const STORE_DIRNAME: &str = "store";
pub const BY_BASE_DIRNAME: &str = "by-base";
pub const BY_SOURCE_DIRNAME: &str = "by-source";
pub const HUGGINGFACE_SOURCE_DIRNAME: &str = "hf";
pub const LOCAL_SOURCE_DIRNAME: &str = "local";
pub const TRAIN_RUN_SOURCE_DIRNAME: &str = "train-run";
pub const STAGING_DIRNAME: &str = "staging";
pub const SOURCE_DIRNAME: &str = "source";

pub const ADAPTER_METADATA_FILENAME: &str = "adapter.toml";
pub const ADAPTER_MANIFEST_FILENAME: &str = "manifest.json";
pub const PEFT_ADAPTER_MODEL_FILENAME: &str = "adapter_model.safetensors";
pub const MLX_ADAPTERS_FILENAME: &str = "adapters.safetensors";
pub const DIFFUSERS_LORA_WEIGHTS_FILENAME: &str = "pytorch_lora_weights.safetensors";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AdapterRef(String);

impl AdapterRef {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, AdapterRefParseError> {
        let normalized = normalize_hex_ref(value.as_ref())?;
        if normalized.len() != ADAPTER_REF_HEX_LENGTH {
            return Err(AdapterRefParseError::InvalidFullLength {
                actual: normalized.len(),
            });
        }

        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn short_ref(&self) -> &str {
        &self.0[..SHORT_ADAPTER_REF_LENGTH]
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for AdapterRef {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for AdapterRef {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<&str> for AdapterRef {
    type Error = AdapterRefParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl Serialize for AdapterRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for AdapterRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AdapterRefSelector(String);

impl AdapterRefSelector {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, AdapterRefParseError> {
        let normalized = normalize_hex_ref(value.as_ref())?;
        if normalized.len() > ADAPTER_REF_HEX_LENGTH {
            return Err(AdapterRefParseError::PrefixTooLong {
                actual: normalized.len(),
            });
        }

        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_full_ref(&self) -> bool {
        self.0.len() == ADAPTER_REF_HEX_LENGTH
    }
}

impl AsRef<str> for AdapterRefSelector {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for AdapterRefSelector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for AdapterRefSelector {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for AdapterRefSelector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdapterRefParseError {
    #[error("adapter reference is empty")]
    Empty,
    #[error("adapter reference must be exactly 64 hexadecimal characters; got {actual}")]
    InvalidFullLength { actual: usize },
    #[error("adapter reference prefix must be at most 64 hexadecimal characters; got {actual}")]
    PrefixTooLong { actual: usize },
    #[error("adapter reference must contain only hexadecimal characters")]
    NonHex,
}

fn normalize_hex_ref(value: &str) -> Result<String, AdapterRefParseError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AdapterRefParseError::Empty);
    }

    if !trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(AdapterRefParseError::NonHex);
    }

    Ok(trimmed.to_ascii_lowercase())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdapterFormat {
    #[serde(rename = "peft")]
    Peft,
    #[serde(rename = "mlx")]
    Mlx,
    #[serde(rename = "diffusers-lora")]
    DiffusersLora,
    #[serde(rename = "mlx-diffusion-lora")]
    MlxDiffusionLora,
    #[serde(rename = "llama-cpp")]
    LlamaCpp,
}

impl AdapterFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Peft => "peft",
            Self::Mlx => "mlx",
            Self::DiffusersLora => "diffusers-lora",
            Self::MlxDiffusionLora => "mlx-diffusion-lora",
            Self::LlamaCpp => "llama-cpp",
        }
    }
}

impl std::fmt::Display for AdapterFormat {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for AdapterFormat {
    type Err = AdapterFormatParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" => Err(AdapterFormatParseError::Empty),
            "peft" => Ok(Self::Peft),
            "mlx" => Ok(Self::Mlx),
            "diffusers-lora" => Ok(Self::DiffusersLora),
            "mlx-diffusion-lora" => Ok(Self::MlxDiffusionLora),
            "llama-cpp" => Ok(Self::LlamaCpp),
            _ => Err(AdapterFormatParseError::Unsupported {
                value: value.trim().to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdapterFormatParseError {
    #[error("adapter format must not be blank")]
    Empty,
    #[error("unsupported adapter format `{value}`")]
    Unsupported { value: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdapterType {
    #[serde(rename = "lora")]
    Lora,
}

impl AdapterType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lora => "lora",
        }
    }
}

impl std::fmt::Display for AdapterType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdapterSourceKind {
    #[serde(rename = "huggingface")]
    HuggingFace,
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "train-run")]
    TrainRun,
}

impl AdapterSourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HuggingFace => "huggingface",
            Self::Local => "local",
            Self::TrainRun => "train-run",
        }
    }
}

impl std::fmt::Display for AdapterSourceKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdapterBackendSupport {
    #[serde(rename = "transformers-peft")]
    TransformersPeft,
    #[serde(rename = "mlx")]
    Mlx,
    #[serde(rename = "diffusers")]
    Diffusers,
    #[serde(rename = "mlx-diffusion")]
    MlxDiffusion,
    #[serde(rename = "llama-cpp")]
    LlamaCpp,
}

impl AdapterBackendSupport {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TransformersPeft => "transformers-peft",
            Self::Mlx => "mlx",
            Self::Diffusers => "diffusers",
            Self::MlxDiffusion => "mlx-diffusion",
            Self::LlamaCpp => "llama-cpp",
        }
    }
}

impl std::fmt::Display for AdapterBackendSupport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for AdapterBackendSupport {
    type Err = AdapterBackendSupportParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" => Err(AdapterBackendSupportParseError::Empty),
            "transformers-peft" => Ok(Self::TransformersPeft),
            "mlx" => Ok(Self::Mlx),
            "diffusers" => Ok(Self::Diffusers),
            "mlx-diffusion" => Ok(Self::MlxDiffusion),
            "llama-cpp" => Ok(Self::LlamaCpp),
            _ => Err(AdapterBackendSupportParseError::Unsupported {
                value: value.trim().to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdapterBackendSupportParseError {
    #[error("adapter backend support must not be blank")]
    Empty,
    #[error("unsupported adapter backend support `{value}`")]
    Unsupported { value: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LoraScale(u32);

impl LoraScale {
    pub const DEFAULT: Self = Self(1000);
    pub const MIN: f32 = 0.0;
    pub const MAX: f32 = 4.0;
    const FACTOR: f32 = 1000.0;

    pub fn new(value: f32) -> Result<Self, LoraScaleError> {
        if !value.is_finite() || !(Self::MIN..=Self::MAX).contains(&value) {
            return Err(LoraScaleError::OutOfRange { value });
        }
        Ok(Self((value * Self::FACTOR).round() as u32))
    }

    pub fn as_f32(self) -> f32 {
        self.0 as f32 / Self::FACTOR
    }
}

impl Default for LoraScale {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl std::fmt::Display for LoraScale {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.as_f32())
    }
}

impl Serialize for LoraScale {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f32(self.as_f32())
    }
}

impl<'de> Deserialize<'de> for LoraScale {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = f32::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LoraScaleError {
    #[error("LoRA scale must be between 0 and 4; got {value}")]
    OutOfRange { value: f32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterManifestEntry {
    pub relative_path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterManifest {
    pub files: Vec<AdapterManifestEntry>,
}

impl AdapterManifest {
    pub fn sorted(mut self) -> Self {
        self.files
            .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        self
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|entry| entry.size_bytes).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn contains_path(&self, expected: &str) -> bool {
        self.files
            .iter()
            .any(|entry| entry.relative_path == expected)
    }

    pub fn safetensors_paths(&self) -> Vec<String> {
        self.files
            .iter()
            .filter(|entry| entry.relative_path.ends_with(".safetensors"))
            .map(|entry| entry.relative_path.clone())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterMetadata {
    pub adapter_ref: AdapterRef,
    pub short_ref: String,
    pub adapter_format: AdapterFormat,
    pub adapter_type: AdapterType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_capability: Option<ModelCapability>,
    pub base_model_ref: Option<ModelRef>,
    pub base_model_source_repo: Option<String>,
    pub base_model_source_revision: Option<String>,
    pub model_family: Option<String>,
    pub backend_support: Vec<AdapterBackendSupport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight_file: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trigger_words: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_scale: Option<LoraScale>,
    pub source_kind: AdapterSourceKind,
    pub source_repo: Option<String>,
    pub source_revision: Option<String>,
    pub source_path: Option<String>,
    pub training_dataset_ref: Option<String>,
    pub training_run_ref: Option<String>,
    pub training_config_ref: Option<String>,
    pub file_count: usize,
    pub total_bytes: u64,
    pub imported_at: String,
}

impl AdapterMetadata {
    pub fn expected_short_ref(&self) -> &str {
        self.adapter_ref.short_ref()
    }

    pub fn has_consistent_short_ref(&self) -> bool {
        self.short_ref == self.expected_short_ref()
    }

    pub fn supports_backend(&self, backend: AdapterBackendSupport) -> bool {
        self.backend_support.contains(&backend)
    }

    pub fn source_summary(&self) -> String {
        match self.source_kind {
            AdapterSourceKind::HuggingFace => match (&self.source_repo, &self.source_revision) {
                (Some(repo), Some(revision)) => format!("{repo}@{revision}"),
                (Some(repo), None) => repo.clone(),
                _ => "unknown".to_string(),
            },
            AdapterSourceKind::Local => self
                .source_path
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            AdapterSourceKind::TrainRun => self
                .training_run_ref
                .as_deref()
                .map(|run_ref| format!("run:{}", short_display_ref(run_ref)))
                .unwrap_or_else(|| "run:unknown".to_string()),
        }
    }

    pub fn base_model_summary(&self) -> String {
        if let Some(model_ref) = &self.base_model_ref {
            return model_ref.short_ref().to_string();
        }

        match (
            &self.base_model_source_repo,
            &self.base_model_source_revision,
        ) {
            (Some(repo), Some(revision)) => format!("{repo}@{revision}"),
            (Some(repo), None) => repo.clone(),
            _ => self
                .model_family
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalAdapterSourceIndex {
    pub adapter_ref: AdapterRef,
    pub short_ref: String,
    pub source_path: String,
    pub imported_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HfAdapterSourceIndex {
    pub adapter_ref: AdapterRef,
    pub short_ref: String,
    pub source_repo: String,
    pub source_revision: String,
    pub imported_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseModelAdapterIndex {
    pub adapter_ref: AdapterRef,
    pub short_ref: String,
    pub base_model_ref: ModelRef,
    pub adapter_format: AdapterFormat,
    pub imported_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainRunAdapterSourceIndex {
    pub adapter_ref: AdapterRef,
    pub short_ref: String,
    pub training_run_ref: String,
    pub training_dataset_ref: String,
    pub training_config_ref: String,
    pub imported_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterSummary {
    pub metadata: AdapterMetadata,
    pub store_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterInspection {
    pub metadata: AdapterMetadata,
    pub store_path: PathBuf,
    pub manifest_path: PathBuf,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterImportOutcome {
    pub metadata: AdapterMetadata,
    pub store_path: PathBuf,
    pub source_index_path: PathBuf,
    pub base_index_path: Option<PathBuf>,
    pub deduplicated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterBindOutcome {
    pub metadata: AdapterMetadata,
    pub store_path: PathBuf,
    pub base_index_path: PathBuf,
    pub removed_base_index_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterRemovalOutcome {
    pub metadata: AdapterMetadata,
    pub store_path: PathBuf,
    pub removed_index_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HfAdapterPullProgress {
    pub description: String,
    pub position: u64,
    pub total: Option<u64>,
    pub unit: String,
    pub finished: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterStoreLayout {
    pub adapters_dir: PathBuf,
    pub store_dir: PathBuf,
    pub by_base_dir: PathBuf,
    pub by_source_dir: PathBuf,
    pub hf_index_dir: PathBuf,
    pub local_index_dir: PathBuf,
    pub train_run_index_dir: PathBuf,
    pub staging_dir: PathBuf,
}

impl AdapterStoreLayout {
    pub fn from_adapters_dir(adapters_dir: impl Into<PathBuf>) -> Self {
        let adapters_dir = adapters_dir.into();
        let by_source_dir = adapters_dir.join(BY_SOURCE_DIRNAME);

        Self {
            store_dir: adapters_dir.join(STORE_DIRNAME),
            by_base_dir: adapters_dir.join(BY_BASE_DIRNAME),
            hf_index_dir: by_source_dir.join(HUGGINGFACE_SOURCE_DIRNAME),
            local_index_dir: by_source_dir.join(LOCAL_SOURCE_DIRNAME),
            train_run_index_dir: by_source_dir.join(TRAIN_RUN_SOURCE_DIRNAME),
            staging_dir: adapters_dir.join(STAGING_DIRNAME),
            adapters_dir,
            by_source_dir,
        }
    }

    pub fn adapter_dir(&self, adapter_ref: &AdapterRef) -> PathBuf {
        self.store_dir.join(adapter_ref.as_str())
    }

    pub fn adapter_metadata_path(&self, adapter_ref: &AdapterRef) -> PathBuf {
        self.adapter_dir(adapter_ref)
            .join(ADAPTER_METADATA_FILENAME)
    }

    pub fn manifest_path(&self, adapter_ref: &AdapterRef) -> PathBuf {
        self.adapter_dir(adapter_ref)
            .join(ADAPTER_MANIFEST_FILENAME)
    }

    pub fn source_dir(&self, adapter_ref: &AdapterRef) -> PathBuf {
        self.adapter_dir(adapter_ref).join(SOURCE_DIRNAME)
    }

    pub fn base_index_dir(&self, base_model_ref: &ModelRef) -> PathBuf {
        self.by_base_dir.join(base_model_ref.as_str())
    }

    pub fn base_index_path(&self, base_model_ref: &ModelRef, adapter_ref: &AdapterRef) -> PathBuf {
        self.base_index_dir(base_model_ref)
            .join(format!("{}.toml", adapter_ref.as_str()))
    }

    pub fn local_index_path(&self, adapter_ref: &AdapterRef) -> PathBuf {
        self.local_index_dir
            .join(format!("{}.toml", adapter_ref.as_str()))
    }

    pub fn hf_index_dir_for_repo(&self, repo_id: &str) -> PathBuf {
        self.hf_index_dir.join(escape_huggingface_repo_id(repo_id))
    }

    pub fn hf_index_path(&self, repo_id: &str, resolved_revision: &str) -> PathBuf {
        self.hf_index_dir_for_repo(repo_id)
            .join(format!("{resolved_revision}.toml"))
    }

    pub fn train_run_index_path(&self, run_ref: &str) -> PathBuf {
        self.train_run_index_dir.join(format!("{run_ref}.toml"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterCompatibilityTarget {
    pub base_model_ref: ModelRef,
    pub base_model_source_repo: Option<String>,
    pub base_model_source_revision: Option<String>,
    pub base_model_capabilities: Vec<ModelCapability>,
    pub required_capability: ModelCapability,
    pub backend: AdapterBackendSupport,
}

pub fn validate_adapter_compatibility(
    metadata: &AdapterMetadata,
    target: &AdapterCompatibilityTarget,
) -> Result<(), AdapterCompatibilityError> {
    if !metadata.supports_backend(target.backend) {
        return Err(AdapterCompatibilityError::UnsupportedBackend {
            backend: target.backend.as_str().to_string(),
        });
    }

    if let Some(adapter_capability) = metadata.target_capability {
        if adapter_capability != target.required_capability {
            return Err(AdapterCompatibilityError::TargetCapabilityMismatch {
                adapter_capability: adapter_capability.as_str().to_string(),
                required_capability: target.required_capability.as_str().to_string(),
            });
        }
    }

    if !target
        .base_model_capabilities
        .contains(&target.required_capability)
    {
        return Err(AdapterCompatibilityError::UnsupportedBaseModelCapability {
            required: target.required_capability.as_str().to_string(),
        });
    }

    if let Some(adapter_base_ref) = &metadata.base_model_ref {
        if adapter_base_ref == &target.base_model_ref {
            return Ok(());
        }

        return Err(AdapterCompatibilityError::BaseModelRefMismatch {
            adapter_base_model_ref: adapter_base_ref.to_string(),
            target_base_model_ref: target.base_model_ref.to_string(),
        });
    }

    let Some(adapter_repo) = metadata
        .base_model_source_repo
        .as_deref()
        .filter(|value| !is_local_path_hint(value))
    else {
        return Err(AdapterCompatibilityError::MissingBaseModelProof);
    };

    let Some(target_repo) = target.base_model_source_repo.as_deref() else {
        return Err(AdapterCompatibilityError::MissingBaseModelProof);
    };

    if adapter_repo != target_repo {
        return Err(AdapterCompatibilityError::BaseModelSourceMismatch {
            adapter_base: adapter_repo.to_string(),
            target_base: target_repo.to_string(),
        });
    }

    if let Some(adapter_revision) = metadata.base_model_source_revision.as_deref() {
        let Some(target_revision) = target.base_model_source_revision.as_deref() else {
            return Err(AdapterCompatibilityError::MissingBaseModelProof);
        };

        if adapter_revision != target_revision {
            return Err(AdapterCompatibilityError::BaseRevisionMismatch {
                adapter_revision: adapter_revision.to_string(),
                target_revision: target_revision.to_string(),
            });
        }
    }

    Ok(())
}

pub fn detect_adapter_format(
    manifest: &AdapterManifest,
) -> Result<AdapterFormat, AdapterFormatSelectionError> {
    if manifest.contains_path(DIFFUSERS_LORA_WEIGHTS_FILENAME) {
        return Ok(AdapterFormat::DiffusersLora);
    }

    if manifest.contains_path(PEFT_ADAPTER_MODEL_FILENAME) {
        return Ok(AdapterFormat::Peft);
    }

    if manifest.contains_path(MLX_ADAPTERS_FILENAME) {
        return Ok(AdapterFormat::Mlx);
    }

    Err(AdapterFormatSelectionError::UnsupportedLayout)
}

pub fn detect_image_adapter_format(
    manifest: &AdapterManifest,
    explicit_format: Option<AdapterFormat>,
    explicit_backend_support: &[AdapterBackendSupport],
) -> Result<AdapterFormat, AdapterFormatSelectionError> {
    if let Some(format) = explicit_format {
        return Ok(format);
    }

    if explicit_backend_support.contains(&AdapterBackendSupport::MlxDiffusion) {
        return Ok(AdapterFormat::MlxDiffusionLora);
    }
    if explicit_backend_support.contains(&AdapterBackendSupport::Diffusers) {
        return Ok(AdapterFormat::DiffusersLora);
    }

    if manifest.contains_path(DIFFUSERS_LORA_WEIGHTS_FILENAME) {
        return Ok(AdapterFormat::DiffusersLora);
    }

    let safetensors = manifest.safetensors_paths();
    match safetensors.as_slice() {
        [_] => Ok(AdapterFormat::DiffusersLora),
        [] => Err(AdapterFormatSelectionError::UnsupportedLayout),
        _ => Err(AdapterFormatSelectionError::AmbiguousImageWeights {
            candidates: safetensors.join(", "),
        }),
    }
}

pub fn select_adapter_weight_file(
    manifest: &AdapterManifest,
    explicit_weight_file: Option<&str>,
    adapter_format: AdapterFormat,
) -> Result<Option<String>, AdapterFormatSelectionError> {
    let image_adapter = matches!(
        adapter_format,
        AdapterFormat::DiffusersLora | AdapterFormat::MlxDiffusionLora
    );
    if !image_adapter {
        return Ok(None);
    }

    if let Some(weight_file) = explicit_weight_file
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if !manifest.contains_path(weight_file) {
            return Err(AdapterFormatSelectionError::WeightFileMissing {
                weight_file: weight_file.to_string(),
            });
        }
        if !weight_file.ends_with(".safetensors") {
            return Err(AdapterFormatSelectionError::UnsupportedWeightFile {
                weight_file: weight_file.to_string(),
            });
        }
        return Ok(Some(weight_file.to_string()));
    }

    if manifest.contains_path(DIFFUSERS_LORA_WEIGHTS_FILENAME) {
        return Ok(Some(DIFFUSERS_LORA_WEIGHTS_FILENAME.to_string()));
    }

    let safetensors = manifest.safetensors_paths();
    match safetensors.as_slice() {
        [weight_file] => Ok(Some(weight_file.clone())),
        [] => Err(AdapterFormatSelectionError::UnsupportedLayout),
        _ => Err(AdapterFormatSelectionError::AmbiguousImageWeights {
            candidates: safetensors.join(", "),
        }),
    }
}

pub fn backend_support_for_format(format: AdapterFormat) -> Vec<AdapterBackendSupport> {
    match format {
        AdapterFormat::Peft => vec![AdapterBackendSupport::TransformersPeft],
        AdapterFormat::Mlx => vec![AdapterBackendSupport::Mlx],
        AdapterFormat::DiffusersLora => vec![AdapterBackendSupport::Diffusers],
        AdapterFormat::MlxDiffusionLora => vec![AdapterBackendSupport::MlxDiffusion],
        AdapterFormat::LlamaCpp => vec![AdapterBackendSupport::LlamaCpp],
    }
}

pub fn escape_huggingface_repo_id(repo_id: &str) -> String {
    repo_id.replace('/', "--")
}

pub fn is_local_path_hint(value: &str) -> bool {
    let trimmed = value.trim();
    Path::new(trimmed).is_absolute() || trimmed.starts_with("./") || trimmed.starts_with("../")
}

fn short_display_ref(value: &str) -> String {
    value.chars().take(SHORT_ADAPTER_REF_LENGTH).collect()
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdapterFormatSelectionError {
    #[error("unsupported adapter layout; expected PEFT adapter_model.safetensors, MLX adapters.safetensors, or one image LoRA .safetensors file")]
    UnsupportedLayout,
    #[error("ambiguous image LoRA weights; specify one weight file from: {candidates}")]
    AmbiguousImageWeights { candidates: String },
    #[error("adapter weight file `{weight_file}` was not found in the adapter source")]
    WeightFileMissing { weight_file: String },
    #[error("adapter weight file `{weight_file}` is unsupported; expected a .safetensors file")]
    UnsupportedWeightFile { weight_file: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdapterCompatibilityError {
    #[error("adapter does not support backend {backend}")]
    UnsupportedBackend { backend: String },
    #[error("adapter targets {adapter_capability}, but request requires {required_capability}")]
    TargetCapabilityMismatch {
        adapter_capability: String,
        required_capability: String,
    },
    #[error("adapter base model must support {required} capability")]
    UnsupportedBaseModelCapability { required: String },
    #[error("adapter base model ref {adapter_base_model_ref} does not match target base model ref {target_base_model_ref}")]
    BaseModelRefMismatch {
        adapter_base_model_ref: String,
        target_base_model_ref: String,
    },
    #[error("adapter base model source {adapter_base} does not match target base model source {target_base}")]
    BaseModelSourceMismatch {
        adapter_base: String,
        target_base: String,
    },
    #[error("adapter base revision {adapter_revision} does not match target base revision {target_revision}")]
    BaseRevisionMismatch {
        adapter_revision: String,
        target_revision: String,
    },
    #[error("adapter compatibility cannot be proven from local model ref or source metadata")]
    MissingBaseModelProof,
}
