//! Cluster definition domain types.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use crate::features::model::{
    domain::{ModelCapability, ModelRef},
    support_status::{ModelSupportEvidenceKind, ModelSupportStatus},
};
use crate::features::server::domain::{
    CloudProvider, ServerCapability, ServerRuntimeProfileSelection,
};

pub const CLUSTERS_DIRNAME: &str = "clusters";
pub const CLUSTER_DEFINITION_FILENAME: &str = "cluster.toml";
pub const CLUSTER_SCHEMA_VERSION: u32 = 1;
pub const MAX_CLUSTER_DEFINITION_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClusterRouteUpdatePolicy {
    #[default]
    Drain,
    Block,
}

impl ClusterRouteUpdatePolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Drain => "drain",
            Self::Block => "block",
        }
    }
}

impl fmt::Display for ClusterRouteUpdatePolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClusterRef(String);

impl ClusterRef {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ClusterRefParseError> {
        let value = value.as_ref();
        if value.is_empty() {
            return Err(ClusterRefParseError::Empty);
        }
        if value.len() > 64 {
            return Err(ClusterRefParseError::TooLong {
                actual: value.len(),
            });
        }

        let mut chars = value.chars();
        let Some(first) = chars.next() else {
            return Err(ClusterRefParseError::Empty);
        };
        if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
            return Err(ClusterRefParseError::InvalidStart);
        }
        if !chars.all(|ch| {
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-')
        }) {
            return Err(ClusterRefParseError::InvalidCharacter);
        }

        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for ClusterRef {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ClusterRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for ClusterRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ClusterRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClusterRefParseError {
    #[error("cluster_ref must not be blank")]
    Empty,
    #[error("cluster_ref must be at most 64 characters; got {actual}")]
    TooLong { actual: usize },
    #[error("cluster_ref must start with a lowercase ASCII letter or digit")]
    InvalidStart,
    #[error("cluster_ref may contain only lowercase ASCII letters, digits, '.', '_', and '-'")]
    InvalidCharacter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClusterRouteKey {
    Chat,
    Embedding,
    Rerank,
    AudioTranscription,
    VisionChat,
}

impl ClusterRouteKey {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ClusterRouteKeyParseError> {
        match value.as_ref() {
            "" => Err(ClusterRouteKeyParseError::Empty),
            "chat" => Ok(Self::Chat),
            "embedding" => Ok(Self::Embedding),
            "rerank" => Ok(Self::Rerank),
            "audio-transcription" => Ok(Self::AudioTranscription),
            "vision-chat" => Ok(Self::VisionChat),
            other => Err(ClusterRouteKeyParseError::Unsupported {
                value: other.to_string(),
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Embedding => "embedding",
            Self::Rerank => "rerank",
            Self::AudioTranscription => "audio-transcription",
            Self::VisionChat => "vision-chat",
        }
    }

    pub const fn server_capability(self) -> ServerCapability {
        match self {
            Self::Chat => ServerCapability::Chat,
            Self::Embedding => ServerCapability::Embedding,
            Self::Rerank => ServerCapability::Rerank,
            Self::AudioTranscription => ServerCapability::AudioTranscription,
            Self::VisionChat => ServerCapability::VisionChat,
        }
    }

    pub const fn model_capability(self) -> ModelCapability {
        self.server_capability().required_model_capability()
    }
}

impl fmt::Display for ClusterRouteKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for ClusterRouteKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ClusterRouteKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClusterRouteKeyParseError {
    #[error("cluster route key must not be blank")]
    Empty,
    #[error(
        "unsupported cluster route `{value}`; expected one of: chat, embedding, rerank, audio-transcription, vision-chat"
    )]
    Unsupported { value: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ClusterRouteTarget {
    LocalModel {
        model_ref: ModelRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        runtime_profile: Option<ServerRuntimeProfileSelection>,
    },
    Provider {
        provider: CloudProvider,
        provider_model: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterDefinition {
    pub schema_version: u32,
    pub cluster_ref: ClusterRef,
    #[serde(default)]
    pub route_update_policy: ClusterRouteUpdatePolicy,
    #[serde(default)]
    pub routes: BTreeMap<ClusterRouteKey, ClusterRouteTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterSummary {
    pub cluster_ref: ClusterRef,
    pub route_keys: Vec<ClusterRouteKey>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterInspection {
    pub definition: ClusterDefinition,
    pub home_dir: PathBuf,
    pub cluster_dir: PathBuf,
    pub definition_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterRemoveOutcome {
    pub inspection: ClusterInspection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClusterReadinessStatus {
    Ready,
    Partial,
    Blocked,
    Unknown,
}

impl ClusterReadinessStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Partial => "partial",
            Self::Blocked => "blocked",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for ClusterReadinessStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClusterRouteReadinessStatus {
    Ready,
    Verified,
    Supported,
    Unknown,
    Stale,
    Failed,
    Unsupported,
    AuthMissing,
    AuthAttention,
    Unavailable,
}

impl ClusterRouteReadinessStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Verified => "verified",
            Self::Supported => "supported",
            Self::Unknown => "unknown",
            Self::Stale => "stale",
            Self::Failed => "failed",
            Self::Unsupported => "unsupported",
            Self::AuthMissing => "auth-missing",
            Self::AuthAttention => "auth-attention",
            Self::Unavailable => "unavailable",
        }
    }

    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Ready | Self::Verified | Self::Supported)
    }

    pub const fn needs_attention(self) -> bool {
        !self.is_ready()
    }
}

impl fmt::Display for ClusterRouteReadinessStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClusterRuntimeProfileSource {
    Configured,
    Inferred,
    None,
    Unavailable,
}

impl ClusterRuntimeProfileSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Configured => "configured",
            Self::Inferred => "inferred",
            Self::None => "none",
            Self::Unavailable => "unavailable",
        }
    }
}

impl fmt::Display for ClusterRuntimeProfileSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterRuntimeProfileReadiness {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configured: Option<ServerRuntimeProfileSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective: Option<ServerRuntimeProfileSelection>,
    pub source: ClusterRuntimeProfileSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClusterReadinessActionCode {
    InspectCluster,
    InspectModel,
    VerifyModelCapability,
    ClearModelProof,
    SetProviderAuth,
    UpdateClusterDefinition,
    ChooseSupportedRouteTarget,
}

impl ClusterReadinessActionCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InspectCluster => "inspect-cluster",
            Self::InspectModel => "inspect-model",
            Self::VerifyModelCapability => "verify-model-capability",
            Self::ClearModelProof => "clear-model-proof",
            Self::SetProviderAuth => "set-provider-auth",
            Self::UpdateClusterDefinition => "update-cluster-definition",
            Self::ChooseSupportedRouteTarget => "choose-supported-route-target",
        }
    }
}

impl fmt::Display for ClusterReadinessActionCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterReadinessAction {
    pub code: ClusterReadinessActionCode,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterReadinessDetail {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterRouteReadiness {
    pub route: ClusterRouteKey,
    pub kind: String,
    pub target: String,
    pub capability: ModelCapability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    pub runtime_profile: ClusterRuntimeProfileReadiness,
    pub status: ClusterRouteReadinessStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_status: Option<ModelSupportStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<ModelSupportEvidenceKind>,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<ClusterReadinessDetail>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<ClusterReadinessAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterReadinessReport {
    pub status: ClusterReadinessStatus,
    pub ready_route_count: usize,
    pub attention_route_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    pub routes: Vec<ClusterRouteReadiness>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<ClusterReadinessDetail>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClusterRouteExecutionBlockerCode {
    MissingRoute,
    UnsupportedTarget,
    NotReady,
    ProofStale,
    ProofFailed,
    Unsupported,
    Unavailable,
}

impl ClusterRouteExecutionBlockerCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingRoute => "cluster_route_missing",
            Self::UnsupportedTarget => "cluster_route_target_unsupported",
            Self::NotReady => "cluster_route_not_ready",
            Self::ProofStale => "cluster_route_proof_stale",
            Self::ProofFailed => "cluster_route_proof_failed",
            Self::Unsupported => "cluster_route_unsupported",
            Self::Unavailable => "cluster_route_unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterLocalRouteExecutionTarget {
    pub route: ClusterRouteKey,
    pub model_ref: ModelRef,
    pub capability: ServerCapability,
    pub runtime_profile: Option<ServerRuntimeProfileSelection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClusterRouteExecutionDecision {
    Ready(ClusterLocalRouteExecutionTarget),
    Blocked {
        route: ClusterRouteKey,
        code: ClusterRouteExecutionBlockerCode,
        status: Option<ClusterRouteReadinessStatus>,
        description: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterStoreLayout {
    pub home_dir: PathBuf,
    pub clusters_dir: PathBuf,
}

impl ClusterStoreLayout {
    pub fn from_home_dir(home_dir: impl Into<PathBuf>) -> Self {
        let home_dir = home_dir.into();
        Self {
            clusters_dir: home_dir.join(CLUSTERS_DIRNAME),
            home_dir,
        }
    }

    pub fn cluster_dir(&self, cluster_ref: impl AsRef<str>) -> PathBuf {
        self.clusters_dir.join(cluster_ref.as_ref())
    }

    pub fn cluster_definition_path(&self, cluster_ref: impl AsRef<str>) -> PathBuf {
        self.cluster_dir(cluster_ref)
            .join(CLUSTER_DEFINITION_FILENAME)
    }
}
