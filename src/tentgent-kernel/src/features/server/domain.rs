//! Server spec identity, runtime target, and process-state domain types.

use std::fmt;
use std::path::PathBuf;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use crate::features::model::domain::{ModelCapability, ModelFormat, ModelRef, ModelRefSelector};

pub const DEFAULT_SERVER_HOST: &str = "127.0.0.1";
pub const DEFAULT_SERVER_PORT: u16 = 8000;

pub const SERVER_REF_HEX_LENGTH: usize = 64;
pub const SHORT_SERVER_REF_LENGTH: usize = 12;

pub const SERVER_SPEC_FILENAME: &str = "server.toml";
pub const SERVER_PROCESS_FILENAME: &str = "process.toml";
pub const SERVER_STDOUT_LOG_FILENAME: &str = "stdout.log";
pub const SERVER_STDERR_LOG_FILENAME: &str = "stderr.log";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ServerRef(String);

impl ServerRef {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ServerRefParseError> {
        let normalized = normalize_hex_ref(value.as_ref())?;
        if normalized.len() != SERVER_REF_HEX_LENGTH {
            return Err(ServerRefParseError::InvalidFullLength {
                actual: normalized.len(),
            });
        }

        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn short_ref(&self) -> &str {
        &self.0[..SHORT_SERVER_REF_LENGTH]
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for ServerRef {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ServerRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for ServerRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ServerRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ServerRefSelector(String);

impl ServerRefSelector {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ServerRefParseError> {
        let normalized = normalize_hex_ref(value.as_ref())?;
        if normalized.len() > SERVER_REF_HEX_LENGTH {
            return Err(ServerRefParseError::PrefixTooLong {
                actual: normalized.len(),
            });
        }

        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_full_ref(&self) -> bool {
        self.0.len() == SERVER_REF_HEX_LENGTH
    }
}

impl AsRef<str> for ServerRefSelector {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ServerRefSelector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for ServerRefSelector {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ServerRefSelector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ServerRefParseError {
    #[error("server reference is empty")]
    Empty,
    #[error("server reference must be exactly 64 hexadecimal characters; got {actual}")]
    InvalidFullLength { actual: usize },
    #[error("server reference prefix must be at most 64 hexadecimal characters; got {actual}")]
    PrefixTooLong { actual: usize },
    #[error("server reference must contain only hexadecimal characters")]
    NonHex,
}

fn normalize_hex_ref(value: &str) -> Result<String, ServerRefParseError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ServerRefParseError::Empty);
    }

    if !trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(ServerRefParseError::NonHex);
    }

    Ok(trimmed.to_ascii_lowercase())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerRuntimeKind {
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "cloud")]
    Cloud,
}

impl Default for ServerRuntimeKind {
    fn default() -> Self {
        Self::Local
    }
}

impl ServerRuntimeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Cloud => "cloud",
        }
    }
}

impl fmt::Display for ServerRuntimeKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloudProvider {
    #[serde(rename = "openai")]
    OpenAI,
    #[serde(rename = "anthropic")]
    Anthropic,
}

impl CloudProvider {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::Anthropic => "anthropic",
        }
    }
}

impl fmt::Display for CloudProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaunchMode {
    #[serde(rename = "foreground")]
    Foreground,
    #[serde(rename = "background")]
    Background,
}

impl LaunchMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Foreground => "foreground",
            Self::Background => "background",
        }
    }
}

impl fmt::Display for LaunchMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerCapability {
    Chat,
    Embedding,
    Rerank,
}

impl ServerCapability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Embedding => "embedding",
            Self::Rerank => "rerank",
        }
    }

    pub const fn required_model_capability(self) -> ModelCapability {
        match self {
            Self::Chat => ModelCapability::Chat,
            Self::Embedding => ModelCapability::Embedding,
            Self::Rerank => ModelCapability::Rerank,
        }
    }
}

impl fmt::Display for ServerCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

pub const fn default_server_capability() -> ServerCapability {
    ServerCapability::Chat
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "server capability `{server_capability}` requires model capability `{required_model_capability}`, but model `{model_ref}` advertises {advertised_model_capabilities}"
)]
pub struct ServerCapabilityCompatibilityError {
    pub server_capability: ServerCapability,
    pub required_model_capability: ModelCapability,
    pub model_ref: ModelRef,
    pub advertised_model_capabilities: String,
}

pub fn ensure_server_model_capability(
    server_capability: ServerCapability,
    model_ref: &ModelRef,
    model_capabilities: &[ModelCapability],
) -> Result<(), ServerCapabilityCompatibilityError> {
    let required_model_capability = server_capability.required_model_capability();
    if model_capabilities.contains(&required_model_capability) {
        return Ok(());
    }

    Err(ServerCapabilityCompatibilityError {
        server_capability,
        required_model_capability,
        model_ref: model_ref.clone(),
        advertised_model_capabilities: model_capabilities_label(model_capabilities),
    })
}

fn model_capabilities_label(capabilities: &[ModelCapability]) -> String {
    if capabilities.is_empty() {
        return "[]".to_string();
    }

    format!(
        "[{}]",
        capabilities
            .iter()
            .map(|capability| capability.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServerRuntimeBackend {
    TransformersPeft,
    Mlx,
    LlamaCpp,
}

impl ServerRuntimeBackend {
    pub const fn from_model_format(format: ModelFormat) -> Self {
        match format {
            ModelFormat::Safetensors => Self::TransformersPeft,
            ModelFormat::Mlx => Self::Mlx,
            ModelFormat::Gguf => Self::LlamaCpp,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TransformersPeft => "transformers-peft",
            Self::Mlx => "mlx",
            Self::LlamaCpp => "llama-cpp",
        }
    }
}

impl fmt::Display for ServerRuntimeBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerRuntimeSelection {
    LocalModel {
        selector: ModelRefSelector,
    },
    CloudProvider {
        provider: CloudProvider,
        provider_model: String,
    },
}

pub fn parse_server_runtime_selection(
    runtime_ref: impl AsRef<str>,
) -> Result<ServerRuntimeSelection, ServerRuntimeSelectionError> {
    let trimmed = runtime_ref.as_ref().trim();
    if trimmed.is_empty() {
        return Err(ServerRuntimeSelectionError::Empty);
    }

    if let Some(provider_model) = trimmed.strip_prefix("openai:") {
        return cloud_runtime_selection(CloudProvider::OpenAI, provider_model);
    }
    if let Some(provider_model) = trimmed
        .strip_prefix("anthropic:")
        .or_else(|| trimmed.strip_prefix("claude:"))
    {
        return cloud_runtime_selection(CloudProvider::Anthropic, provider_model);
    }

    let selector = ModelRefSelector::parse(trimmed)
        .map_err(|err| ServerRuntimeSelectionError::InvalidLocalModelRef(err.to_string()))?;
    Ok(ServerRuntimeSelection::LocalModel { selector })
}

fn cloud_runtime_selection(
    provider: CloudProvider,
    provider_model: &str,
) -> Result<ServerRuntimeSelection, ServerRuntimeSelectionError> {
    let provider_model = provider_model.trim();
    if provider_model.is_empty() {
        return Err(ServerRuntimeSelectionError::EmptyCloudProviderModel {
            provider: provider.to_string(),
        });
    }

    Ok(ServerRuntimeSelection::CloudProvider {
        provider,
        provider_model: provider_model.to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ServerRuntimeSelectionError {
    #[error("server runtime reference is empty")]
    Empty,
    #[error("cloud provider `{provider}` requires a non-empty model name")]
    EmptyCloudProviderModel { provider: String },
    #[error("invalid local model reference: {0}")]
    InvalidLocalModelRef(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerRuntimeTarget {
    LocalModel {
        model_ref: ModelRef,
        backend: ServerRuntimeBackend,
    },
    CloudProvider {
        provider: CloudProvider,
        provider_model: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerSpec {
    pub server_ref: ServerRef,
    pub short_ref: String,
    #[serde(default)]
    pub runtime_kind: ServerRuntimeKind,
    #[serde(default = "default_server_capability")]
    pub capability: ServerCapability,
    #[serde(default)]
    pub model_ref: Option<ModelRef>,
    #[serde(default)]
    pub provider: Option<CloudProvider>,
    #[serde(default)]
    pub provider_model: Option<String>,
    pub host: String,
    pub port: u16,
    pub lazy_load: bool,
    pub idle_seconds: Option<u64>,
    pub created_at: String,
}

impl ServerSpec {
    pub fn is_cloud(&self) -> bool {
        self.runtime_kind == ServerRuntimeKind::Cloud
    }

    pub fn local_model_ref(&self) -> Option<&ModelRef> {
        if self.runtime_kind == ServerRuntimeKind::Local {
            self.model_ref.as_ref()
        } else {
            None
        }
    }

    pub fn runtime_model_label(&self) -> String {
        match self.runtime_kind {
            ServerRuntimeKind::Local => self
                .model_ref
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "(missing)".to_string()),
            ServerRuntimeKind::Cloud => self
                .provider_model
                .clone()
                .unwrap_or_else(|| "(missing)".to_string()),
        }
    }

    pub fn provider_label(&self) -> &'static str {
        self.provider.map(CloudProvider::as_str).unwrap_or("-")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerProcessMetadata {
    pub pid: u32,
    pub launch_mode: LaunchMode,
    pub started_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSummary {
    pub spec: ServerSpec,
    pub running: bool,
    pub process: Option<ServerProcessMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInspection {
    pub spec: ServerSpec,
    pub home_dir: PathBuf,
    pub server_dir: PathBuf,
    pub spec_path: PathBuf,
    pub process_path: PathBuf,
    pub stdout_log_path: PathBuf,
    pub stderr_log_path: PathBuf,
    pub running: bool,
    pub process: Option<ServerProcessMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerPrepareOutcome {
    pub inspection: ServerInspection,
    pub created: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerStopOutcome {
    pub inspection: ServerInspection,
    pub stopped_pid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRemoveOutcome {
    pub inspection: ServerInspection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerStoreLayout {
    pub home_dir: PathBuf,
    pub servers_dir: PathBuf,
}

impl ServerStoreLayout {
    pub fn from_home_and_servers_dir(
        home_dir: impl Into<PathBuf>,
        servers_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            home_dir: home_dir.into(),
            servers_dir: servers_dir.into(),
        }
    }

    pub fn server_dir(&self, server_ref: impl AsRef<str>) -> PathBuf {
        self.servers_dir.join(server_ref.as_ref())
    }

    pub fn server_spec_path(&self, server_ref: impl AsRef<str>) -> PathBuf {
        self.server_dir(server_ref).join(SERVER_SPEC_FILENAME)
    }

    pub fn process_metadata_path(&self, server_ref: impl AsRef<str>) -> PathBuf {
        self.server_dir(server_ref).join(SERVER_PROCESS_FILENAME)
    }

    pub fn stdout_log_path(&self, server_ref: impl AsRef<str>) -> PathBuf {
        self.server_dir(server_ref).join(SERVER_STDOUT_LOG_FILENAME)
    }

    pub fn stderr_log_path(&self, server_ref: impl AsRef<str>) -> PathBuf {
        self.server_dir(server_ref).join(SERVER_STDERR_LOG_FILENAME)
    }
}

pub fn normalize_server_host(host: Option<&str>) -> Result<String, ServerHostError> {
    let host = host.unwrap_or(DEFAULT_SERVER_HOST).trim();
    if host.is_empty() {
        return Err(ServerHostError::Empty);
    }

    Ok(host.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ServerHostError {
    #[error("server host must not be empty")]
    Empty,
}
