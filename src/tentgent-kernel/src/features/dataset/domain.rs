//! Dataset store identity, metadata, schema, and pure planning types.

use std::path::PathBuf;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

pub const DATASET_REF_HEX_LENGTH: usize = 64;
pub const SHORT_DATASET_REF_LENGTH: usize = 12;

pub const STORE_DIRNAME: &str = "store";
pub const BY_SOURCE_DIRNAME: &str = "by-source";
pub const LOCAL_SOURCE_DIRNAME: &str = "local";
pub const STAGING_DIRNAME: &str = "staging";
pub const SOURCE_DIRNAME: &str = "source";

pub const DATASET_METADATA_FILENAME: &str = "dataset.toml";
pub const DATASET_MANIFEST_FILENAME: &str = "manifest.json";

pub const CANONICAL_CHAT_SCHEMA: &str = "tentgent.chat.v1";
pub const TRAIN_SPLIT_FILENAME: &str = "train.jsonl";
pub const VALID_SPLIT_FILENAME: &str = "valid.jsonl";
pub const LEGACY_VALID_SPLIT_FILENAME: &str = "val.jsonl";
pub const TEST_SPLIT_FILENAME: &str = "test.jsonl";
pub const EVAL_CASES_SPLIT_FILENAME: &str = "eval_cases.jsonl";
pub const SOURCE_MANIFEST_FILENAME: &str = "manifest.json";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DatasetRef(String);

impl DatasetRef {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, DatasetRefParseError> {
        let normalized = normalize_hex_ref(value.as_ref())?;
        if normalized.len() != DATASET_REF_HEX_LENGTH {
            return Err(DatasetRefParseError::InvalidFullLength {
                actual: normalized.len(),
            });
        }

        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn short_ref(&self) -> &str {
        &self.0[..SHORT_DATASET_REF_LENGTH]
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for DatasetRef {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for DatasetRef {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<&str> for DatasetRef {
    type Error = DatasetRefParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl Serialize for DatasetRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for DatasetRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DatasetRefSelector(String);

impl DatasetRefSelector {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, DatasetRefParseError> {
        let normalized = normalize_hex_ref(value.as_ref())?;
        if normalized.len() > DATASET_REF_HEX_LENGTH {
            return Err(DatasetRefParseError::PrefixTooLong {
                actual: normalized.len(),
            });
        }

        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_full_ref(&self) -> bool {
        self.0.len() == DATASET_REF_HEX_LENGTH
    }
}

impl AsRef<str> for DatasetRefSelector {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for DatasetRefSelector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for DatasetRefSelector {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for DatasetRefSelector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DatasetRefParseError {
    #[error("dataset reference is empty")]
    Empty,
    #[error("dataset reference must be exactly 64 hexadecimal characters; got {actual}")]
    InvalidFullLength { actual: usize },
    #[error("dataset reference prefix must be at most 64 hexadecimal characters; got {actual}")]
    PrefixTooLong { actual: usize },
    #[error("dataset reference must contain only hexadecimal characters")]
    NonHex,
}

fn normalize_hex_ref(value: &str) -> Result<String, DatasetRefParseError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DatasetRefParseError::Empty);
    }

    if !trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(DatasetRefParseError::NonHex);
    }

    Ok(trimmed.to_ascii_lowercase())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DatasetFormat {
    #[serde(rename = "jsonl")]
    Jsonl,
    #[serde(rename = "directory")]
    Directory,
}

impl DatasetFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Jsonl => "jsonl",
            Self::Directory => "directory",
        }
    }
}

impl std::fmt::Display for DatasetFormat {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DatasetSourceKind {
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "generated")]
    Generated,
    #[serde(rename = "huggingface")]
    HuggingFace,
}

impl DatasetSourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Generated => "generated",
            Self::HuggingFace => "huggingface",
        }
    }
}

impl std::fmt::Display for DatasetSourceKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatasetSplitKind {
    Train,
    Valid,
    Test,
    EvalCases,
}

impl DatasetSplitKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Train => "train",
            Self::Valid => "valid",
            Self::Test => "test",
            Self::EvalCases => "eval_cases",
        }
    }

    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Train => TRAIN_SPLIT_FILENAME,
            Self::Valid => VALID_SPLIT_FILENAME,
            Self::Test => TEST_SPLIT_FILENAME,
            Self::EvalCases => EVAL_CASES_SPLIT_FILENAME,
        }
    }
}

impl std::fmt::Display for DatasetSplitKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatasetEvalSplit {
    Train,
    Valid,
    Test,
    EvalCases,
    All,
}

impl DatasetEvalSplit {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Train => "train",
            Self::Valid => "valid",
            Self::Test => "test",
            Self::EvalCases => "eval_cases",
            Self::All => "all",
        }
    }
}

impl std::fmt::Display for DatasetEvalSplit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatasetProvider {
    OpenAI,
    Anthropic,
    Gemini,
}

impl DatasetProvider {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
        }
    }
}

impl std::fmt::Display for DatasetProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetManifestEntry {
    pub relative_path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetManifest {
    pub files: Vec<DatasetManifestEntry>,
}

impl DatasetManifest {
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

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DatasetPackageMetadata {
    pub tuning_ready: bool,
    pub splits: DatasetSplits,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DatasetSplits {
    pub train: Option<String>,
    pub validation: Option<String>,
    pub test: Option<String>,
    pub eval_cases: Option<String>,
    pub source_manifest: Option<String>,
}

impl DatasetSplits {
    pub fn split_names(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self.train.is_some() {
            names.push("train");
        }
        if self.validation.is_some() {
            names.push("valid");
        }
        if self.test.is_some() {
            names.push("test");
        }
        if self.eval_cases.is_some() {
            names.push("eval_cases");
        }
        names
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetMetadata {
    pub dataset_ref: DatasetRef,
    pub short_ref: String,
    pub source_kind: DatasetSourceKind,
    pub source_path: Option<String>,
    pub source_repo: Option<String>,
    pub source_revision: Option<String>,
    pub dataset_format: DatasetFormat,
    pub file_count: usize,
    pub total_bytes: u64,
    pub imported_at: String,
    #[serde(default)]
    pub package: DatasetPackageMetadata,
}

impl DatasetMetadata {
    pub fn expected_short_ref(&self) -> &str {
        self.dataset_ref.short_ref()
    }

    pub fn has_consistent_short_ref(&self) -> bool {
        self.short_ref == self.expected_short_ref()
    }

    pub fn source_summary(&self) -> String {
        match self.source_kind {
            DatasetSourceKind::Local => self
                .source_path
                .clone()
                .unwrap_or_else(|| "(local source not recorded)".to_string()),
            DatasetSourceKind::Generated => "generated".to_string(),
            DatasetSourceKind::HuggingFace => match (&self.source_repo, &self.source_revision) {
                (Some(repo), Some(revision)) => format!("{repo}@{revision}"),
                (Some(repo), None) => repo.clone(),
                _ => "(huggingface source not recorded)".to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalDatasetSourceIndex {
    pub dataset_ref: DatasetRef,
    pub short_ref: String,
    pub source_path: String,
    pub imported_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetSummary {
    pub metadata: DatasetMetadata,
    pub store_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetInspection {
    pub metadata: DatasetMetadata,
    pub store_path: PathBuf,
    pub manifest_path: PathBuf,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetImportOutcome {
    pub metadata: DatasetMetadata,
    pub store_path: PathBuf,
    pub source_index_path: PathBuf,
    pub deduplicated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetExportOutcome {
    pub metadata: DatasetMetadata,
    pub managed_source_path: PathBuf,
    pub destination_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetRemovalOutcome {
    pub metadata: DatasetMetadata,
    pub store_path: PathBuf,
    pub removed_index_paths: Vec<PathBuf>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatasetDiffStatus {
    Added,
    Removed,
    Modified,
    Unchanged,
}

impl DatasetDiffStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Modified => "modified",
            Self::Unchanged => "unchanged",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetDiffFile {
    pub status: DatasetDiffStatus,
    pub relative_path: String,
    pub left_size_bytes: Option<u64>,
    pub right_size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DatasetDiffSummary {
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
    pub unchanged: usize,
    pub left_total_bytes: u64,
    pub right_total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetManifestDiff {
    pub summary: DatasetDiffSummary,
    pub files: Vec<DatasetDiffFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetDiffSide {
    pub label: String,
    pub short_ref: Option<String>,
    pub tuning_ready: bool,
    pub splits: String,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetDiffOutcome {
    pub left: DatasetDiffSide,
    pub right: DatasetDiffSide,
    pub diff: DatasetManifestDiff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatasetValidationTargetKind {
    File,
    Directory,
}

impl DatasetValidationTargetKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetValidationSplit {
    pub name: String,
    pub path: PathBuf,
    pub records: usize,
    pub errors: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetValidationIssue {
    pub path: PathBuf,
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetValidationOutcome {
    pub path: PathBuf,
    pub target_kind: DatasetValidationTargetKind,
    pub tuning_ready: bool,
    pub splits: Vec<DatasetValidationSplit>,
    pub warnings: Vec<String>,
    pub errors: Vec<DatasetValidationIssue>,
}

impl DatasetValidationOutcome {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn record_count(&self) -> usize {
        self.splits.iter().map(|split| split.records).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetTemplateRequest {
    pub task: String,
    pub language: String,
}

impl DatasetTemplateRequest {
    pub fn new(task: Option<String>, language: Option<String>) -> Self {
        Self {
            task: normalize_hint(task, "chat"),
            language: normalize_hint(language, "en"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetRenderedTemplate {
    pub template_version: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatasetPromptSource {
    Brief(String),
    SpecPath(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DatasetSynthCounts {
    pub count: Option<u32>,
    pub train_count: Option<u32>,
    pub valid_count: Option<u32>,
    pub test_count: Option<u32>,
    pub eval_count: Option<u32>,
}

impl DatasetSynthCounts {
    pub fn expected_jobs(&self) -> u64 {
        let split_jobs = [
            self.train_count,
            self.valid_count,
            self.test_count,
            self.eval_count,
        ]
        .into_iter()
        .flatten()
        .count();
        if split_jobs == 0 {
            1
        } else {
            split_jobs as u64
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DatasetSynthRequest {
    pub provider: DatasetProvider,
    pub provider_model: String,
    pub output_dir: PathBuf,
    pub prompt_source: DatasetPromptSource,
    pub split: DatasetSplitKind,
    pub counts: DatasetSynthCounts,
    pub max_tokens: Option<u32>,
    pub temperature: f32,
    pub timeout_seconds: f32,
    pub retries: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetSynthPromptRequest {
    pub prompt_source: DatasetPromptSource,
    pub split: DatasetSplitKind,
    pub counts: DatasetSynthCounts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetSynthRuntimeOutput {
    pub outcome: serde_json::Value,
    pub progress_events: Vec<serde_json::Value>,
    pub progress_truncated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DatasetEvalRequest {
    pub provider: DatasetProvider,
    pub provider_model: String,
    pub input: PathBuf,
    pub output_dir: PathBuf,
    pub split: DatasetEvalSplit,
    pub max_records: u32,
    pub criteria: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: f32,
    pub timeout_seconds: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetRuntimeDebug {
    pub output_path: Option<PathBuf>,
    pub debug_dir: Option<PathBuf>,
    pub prompt_path: Option<PathBuf>,
    pub provider_output_path: Option<PathBuf>,
    pub error_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetStoreLayout {
    pub datasets_dir: PathBuf,
    pub store_dir: PathBuf,
    pub by_source_dir: PathBuf,
    pub local_index_dir: PathBuf,
    pub staging_dir: PathBuf,
}

impl DatasetStoreLayout {
    pub fn from_datasets_dir(datasets_dir: impl Into<PathBuf>) -> Self {
        let datasets_dir = datasets_dir.into();
        let by_source_dir = datasets_dir.join(BY_SOURCE_DIRNAME);

        Self {
            store_dir: datasets_dir.join(STORE_DIRNAME),
            local_index_dir: by_source_dir.join(LOCAL_SOURCE_DIRNAME),
            staging_dir: datasets_dir.join(STAGING_DIRNAME),
            datasets_dir,
            by_source_dir,
        }
    }

    pub fn dataset_dir(&self, dataset_ref: &DatasetRef) -> PathBuf {
        self.store_dir.join(dataset_ref.as_str())
    }

    pub fn dataset_metadata_path(&self, dataset_ref: &DatasetRef) -> PathBuf {
        self.dataset_dir(dataset_ref)
            .join(DATASET_METADATA_FILENAME)
    }

    pub fn manifest_path(&self, dataset_ref: &DatasetRef) -> PathBuf {
        self.dataset_dir(dataset_ref)
            .join(DATASET_MANIFEST_FILENAME)
    }

    pub fn source_dir(&self, dataset_ref: &DatasetRef) -> PathBuf {
        self.dataset_dir(dataset_ref).join(SOURCE_DIRNAME)
    }

    pub fn local_index_path(&self, dataset_ref: &DatasetRef) -> PathBuf {
        self.local_index_dir
            .join(format!("{}.toml", dataset_ref.as_str()))
    }
}

fn normalize_hint(value: Option<String>, default: &str) -> String {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}
