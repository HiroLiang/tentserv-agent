//! Model store identity, metadata, and pure serving-capability rules.

use std::path::{Path, PathBuf};

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

pub const MODEL_REF_HEX_LENGTH: usize = 64;
pub const SHORT_MODEL_REF_LENGTH: usize = 12;

pub const STORE_DIRNAME: &str = "store";
pub const BY_SOURCE_DIRNAME: &str = "by-source";
pub const HUGGINGFACE_SOURCE_DIRNAME: &str = "hf";
pub const LOCAL_SOURCE_DIRNAME: &str = "local";
pub const STAGING_DIRNAME: &str = "staging";
pub const VARIANTS_DIRNAME: &str = "variants";
pub const SOURCE_DIRNAME: &str = "source";

pub const MODEL_METADATA_FILENAME: &str = "model.toml";
pub const MODEL_MANIFEST_FILENAME: &str = "manifest.json";
pub const VARIANT_METADATA_FILENAME: &str = "variant.toml";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModelRef(String);

impl ModelRef {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ModelRefParseError> {
        let normalized = normalize_hex_ref(value.as_ref())?;
        if normalized.len() != MODEL_REF_HEX_LENGTH {
            return Err(ModelRefParseError::InvalidFullLength {
                actual: normalized.len(),
            });
        }

        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn short_ref(&self) -> &str {
        &self.0[..SHORT_MODEL_REF_LENGTH]
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for ModelRef {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for ModelRef {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<&str> for ModelRef {
    type Error = ModelRefParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl Serialize for ModelRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ModelRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModelRefSelector(String);

impl ModelRefSelector {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ModelRefParseError> {
        let normalized = normalize_hex_ref(value.as_ref())?;
        if normalized.len() > MODEL_REF_HEX_LENGTH {
            return Err(ModelRefParseError::PrefixTooLong {
                actual: normalized.len(),
            });
        }

        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_full_ref(&self) -> bool {
        self.0.len() == MODEL_REF_HEX_LENGTH
    }
}

impl AsRef<str> for ModelRefSelector {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for ModelRefSelector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for ModelRefSelector {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ModelRefSelector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelRefParseError {
    #[error("model reference is empty")]
    Empty,
    #[error("model reference must be exactly 64 hexadecimal characters; got {actual}")]
    InvalidFullLength { actual: usize },
    #[error("model reference prefix must be at most 64 hexadecimal characters; got {actual}")]
    PrefixTooLong { actual: usize },
    #[error("model reference must contain only hexadecimal characters")]
    NonHex,
}

fn normalize_hex_ref(value: &str) -> Result<String, ModelRefParseError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ModelRefParseError::Empty);
    }

    if !trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(ModelRefParseError::NonHex);
    }

    Ok(trimmed.to_ascii_lowercase())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelFormat {
    Safetensors,
    Gguf,
    Mlx,
}

impl ModelFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Safetensors => "safetensors",
            Self::Gguf => "gguf",
            Self::Mlx => "mlx",
        }
    }
}

impl std::fmt::Display for ModelFormat {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelCapability {
    Chat,
    Embedding,
    Rerank,
}

impl ModelCapability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Embedding => "embedding",
            Self::Rerank => "rerank",
        }
    }
}

impl std::fmt::Display for ModelCapability {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for ModelCapability {
    type Err = ModelCapabilityParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let normalized = value.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "" => Err(ModelCapabilityParseError::Empty),
            "chat" => Ok(Self::Chat),
            "embedding" => Ok(Self::Embedding),
            "rerank" => Ok(Self::Rerank),
            _ => Err(ModelCapabilityParseError::Unsupported {
                value: value.trim().to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelCapabilityParseError {
    #[error("model capability must not be blank; expected one of: chat, embedding, rerank")]
    Empty,
    #[error("unsupported model capability `{value}`; expected one of: chat, embedding, rerank")]
    Unsupported { value: String },
}

pub fn default_model_capabilities() -> Vec<ModelCapability> {
    vec![ModelCapability::Chat]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelCapabilitySource {
    DefaultChat,
    ExplicitUser,
    HuggingFaceMetadata,
    ManualUpdate,
}

impl ModelCapabilitySource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DefaultChat => "default-chat",
            Self::ExplicitUser => "explicit-user",
            Self::HuggingFaceMetadata => "huggingface-metadata",
            Self::ManualUpdate => "manual-update",
        }
    }
}

impl std::fmt::Display for ModelCapabilitySource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

pub const fn default_model_capability_source() -> ModelCapabilitySource {
    ModelCapabilitySource::DefaultChat
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelSourceKind {
    #[serde(rename = "huggingface")]
    HuggingFace,
    #[serde(rename = "local")]
    Local,
}

impl ModelSourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HuggingFace => "huggingface",
            Self::Local => "local",
        }
    }
}

impl std::fmt::Display for ModelSourceKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelImportMethod {
    Add,
    Pull,
}

impl ModelImportMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Pull => "pull",
        }
    }
}

impl std::fmt::Display for ModelImportMethod {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelVariantStatus {
    Imported,
}

impl ModelVariantStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Imported => "imported",
        }
    }
}

impl std::fmt::Display for ModelVariantStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelManifestEntry {
    pub relative_path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelManifest {
    pub files: Vec<ModelManifestEntry>,
}

impl ModelManifest {
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub model_ref: ModelRef,
    pub short_ref: String,
    pub source_kind: ModelSourceKind,
    pub source_repo: Option<String>,
    pub source_revision: Option<String>,
    pub source_path: Option<String>,
    pub primary_format: ModelFormat,
    pub detected_formats: Vec<ModelFormat>,
    #[serde(default = "default_model_capabilities")]
    pub model_capabilities: Vec<ModelCapability>,
    #[serde(default = "default_model_capability_source")]
    pub model_capability_source: ModelCapabilitySource,
    pub file_count: usize,
    pub total_bytes: u64,
    pub imported_at: String,
}

impl ModelMetadata {
    pub fn expected_short_ref(&self) -> &str {
        self.model_ref.short_ref()
    }

    pub fn has_consistent_short_ref(&self) -> bool {
        self.short_ref == self.expected_short_ref()
    }

    pub fn supports_capability(&self, capability: ModelCapability) -> bool {
        self.model_capabilities.contains(&capability)
    }

    pub fn source_summary(&self) -> String {
        match self.source_kind {
            ModelSourceKind::HuggingFace => match (&self.source_repo, &self.source_revision) {
                (Some(repo), Some(revision)) => format!("{repo}@{revision}"),
                (Some(repo), None) => repo.clone(),
                _ => "unknown".to_string(),
            },
            ModelSourceKind::Local => self
                .source_path
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelVariantMetadata {
    pub format: ModelFormat,
    pub status: ModelVariantStatus,
    pub import_method: ModelImportMethod,
    pub relative_source_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalModelSourceIndex {
    pub model_ref: ModelRef,
    pub short_ref: String,
    pub source_path: String,
    pub imported_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HfModelSourceIndex {
    pub model_ref: ModelRef,
    pub short_ref: String,
    pub source_repo: String,
    pub source_revision: String,
    pub imported_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSummary {
    pub metadata: ModelMetadata,
    pub store_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInspection {
    pub metadata: ModelMetadata,
    pub store_path: PathBuf,
    pub manifest_path: PathBuf,
    pub variant_source_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelImportOutcome {
    pub metadata: ModelMetadata,
    pub store_path: PathBuf,
    pub source_index_path: PathBuf,
    pub deduplicated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRemovalOutcome {
    pub metadata: ModelMetadata,
    pub store_path: PathBuf,
    pub removed_index_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HfModelPullProgress {
    pub description: String,
    pub position: u64,
    pub total: Option<u64>,
    pub unit: String,
    pub finished: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelStoreLayout {
    pub models_dir: PathBuf,
    pub store_dir: PathBuf,
    pub by_source_dir: PathBuf,
    pub hf_index_dir: PathBuf,
    pub local_index_dir: PathBuf,
    pub staging_dir: PathBuf,
}

impl ModelStoreLayout {
    pub fn from_models_dir(models_dir: impl Into<PathBuf>) -> Self {
        let models_dir = models_dir.into();
        let by_source_dir = models_dir.join(BY_SOURCE_DIRNAME);

        Self {
            store_dir: models_dir.join(STORE_DIRNAME),
            hf_index_dir: by_source_dir.join(HUGGINGFACE_SOURCE_DIRNAME),
            local_index_dir: by_source_dir.join(LOCAL_SOURCE_DIRNAME),
            staging_dir: models_dir.join(STAGING_DIRNAME),
            models_dir,
            by_source_dir,
        }
    }

    pub fn model_dir(&self, model_ref: &ModelRef) -> PathBuf {
        self.store_dir.join(model_ref.as_str())
    }

    pub fn model_metadata_path(&self, model_ref: &ModelRef) -> PathBuf {
        self.model_dir(model_ref).join(MODEL_METADATA_FILENAME)
    }

    pub fn manifest_path(&self, model_ref: &ModelRef) -> PathBuf {
        self.model_dir(model_ref).join(MODEL_MANIFEST_FILENAME)
    }

    pub fn variant_dir(&self, model_ref: &ModelRef, format: ModelFormat) -> PathBuf {
        self.model_dir(model_ref)
            .join(VARIANTS_DIRNAME)
            .join(format.as_str())
    }

    pub fn variant_metadata_path(&self, model_ref: &ModelRef, format: ModelFormat) -> PathBuf {
        self.variant_dir(model_ref, format)
            .join(VARIANT_METADATA_FILENAME)
    }

    pub fn variant_source_dir(&self, model_ref: &ModelRef, format: ModelFormat) -> PathBuf {
        self.variant_dir(model_ref, format).join(SOURCE_DIRNAME)
    }

    pub fn local_index_path(&self, model_ref: &ModelRef) -> PathBuf {
        self.local_index_dir
            .join(format!("{}.toml", model_ref.as_str()))
    }

    pub fn hf_index_dir_for_repo(&self, repo_id: &str) -> PathBuf {
        self.hf_index_dir.join(escape_huggingface_repo_id(repo_id))
    }

    pub fn hf_index_path(&self, repo_id: &str, resolved_revision: &str) -> PathBuf {
        self.hf_index_dir_for_repo(repo_id)
            .join(format!("{resolved_revision}.toml"))
    }
}

pub fn detect_model_formats(
    manifest: &ModelManifest,
    source_repo: Option<&str>,
) -> Vec<ModelFormat> {
    let mut formats = Vec::new();

    if source_repo.is_some_and(is_mlx_huggingface_repo) {
        formats.push(ModelFormat::Mlx);
    }

    let has_safetensors = manifest.files.iter().any(|entry| {
        entry.relative_path.ends_with(".safetensors")
            || Path::new(&entry.relative_path)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == "model.safetensors.index.json")
    });
    if has_safetensors {
        formats.push(ModelFormat::Safetensors);
    }

    if manifest
        .files
        .iter()
        .any(|entry| entry.relative_path.ends_with(".gguf"))
    {
        formats.push(ModelFormat::Gguf);
    }

    formats
}

pub fn select_primary_model_format(
    detected_formats: &[ModelFormat],
    source_repo: Option<&str>,
) -> Result<ModelFormat, ModelFormatSelectionError> {
    if source_repo.is_some_and(is_mlx_huggingface_repo) {
        return Ok(ModelFormat::Mlx);
    }

    if detected_formats.contains(&ModelFormat::Safetensors) {
        return Ok(ModelFormat::Safetensors);
    }

    if detected_formats.contains(&ModelFormat::Gguf) {
        return Ok(ModelFormat::Gguf);
    }

    Err(ModelFormatSelectionError::UnsupportedLayout)
}

pub fn is_mlx_huggingface_repo(repo_id: &str) -> bool {
    repo_id.starts_with("mlx-community/")
}

pub fn escape_huggingface_repo_id(repo_id: &str) -> String {
    repo_id.replace('/', "--")
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelFormatSelectionError {
    #[error(
        "unsupported model layout; expected safetensors files, model.safetensors.index.json, gguf files, or an mlx-community Hugging Face repository"
    )]
    UnsupportedLayout,
}
