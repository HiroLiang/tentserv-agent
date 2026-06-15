//! Model support catalog data types.

use serde::{Deserialize, Serialize};

use super::super::domain::{MlxRuntimeFamily, ModelCapability, ModelFormat, ModelSourceKind};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSupportCatalogDocument {
    pub schema_version: u32,
    pub models: Vec<ModelSupportCatalogEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSupportCatalogEntry {
    pub source_kind: ModelSourceKind,
    #[serde(default)]
    pub source_repos: Vec<String>,
    #[serde(default)]
    pub source_repo_patterns: Vec<String>,
    pub publisher: String,
    pub family: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameter_scale: Option<String>,
    pub capabilities: Vec<ModelCapability>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub recommended_for: Vec<String>,
    #[serde(default)]
    pub primary_formats: Vec<ModelFormat>,
    #[serde(default)]
    pub mlx_runtime_families: Vec<MlxRuntimeFamily>,
    #[serde(default)]
    pub backends: Vec<String>,
    pub support_level: ModelSupportCatalogLevel,
    #[serde(default)]
    pub runtime_notes: Vec<String>,
    pub evidence: ModelSupportCatalogEvidence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl ModelSupportCatalogEntry {
    pub fn source_label(&self) -> String {
        self.source_repos
            .first()
            .cloned()
            .or_else(|| self.source_repo_patterns.first().cloned())
            .unwrap_or_else(|| "unknown".to_string())
    }

    pub fn support_hint_reason(&self) -> String {
        self.reason.clone().unwrap_or_else(|| {
            format!(
                "{} {} is marked {} by the built-in model support catalog",
                self.publisher,
                self.family,
                self.support_level.as_str()
            )
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelSupportCatalogLevel {
    FixtureSupported,
    LocalRuntimeSupported,
    CatalogKnown,
    RequiresExternalRuntime,
    KnownUnsupported,
    Deprecated,
}

impl ModelSupportCatalogLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FixtureSupported => "fixture-supported",
            Self::LocalRuntimeSupported => "local-runtime-supported",
            Self::CatalogKnown => "catalog-known",
            Self::RequiresExternalRuntime => "requires-external-runtime",
            Self::KnownUnsupported => "known-unsupported",
            Self::Deprecated => "deprecated",
        }
    }

    pub(super) const fn hint_status(self) -> Option<ModelCatalogHintStatus> {
        match self {
            Self::FixtureSupported | Self::LocalRuntimeSupported => {
                Some(ModelCatalogHintStatus::Supported)
            }
            Self::KnownUnsupported => Some(ModelCatalogHintStatus::Unsupported),
            Self::CatalogKnown | Self::RequiresExternalRuntime | Self::Deprecated => None,
        }
    }
}

impl std::fmt::Display for ModelSupportCatalogLevel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelSupportCatalogEvidence {
    FixtureDocs,
    #[serde(rename = "huggingface-model-card")]
    HuggingFaceModelCard,
    PublisherModelCard,
    NvidiaCatalog,
    MlxCommunityConversion,
    TentgentCurated,
}

impl ModelSupportCatalogEvidence {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FixtureDocs => "fixture-docs",
            Self::HuggingFaceModelCard => "huggingface-model-card",
            Self::PublisherModelCard => "publisher-model-card",
            Self::NvidiaCatalog => "nvidia-catalog",
            Self::MlxCommunityConversion => "mlx-community-conversion",
            Self::TentgentCurated => "tentgent-curated",
        }
    }
}

impl std::fmt::Display for ModelSupportCatalogEvidence {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ModelCatalogHintStatus {
    Supported,
    Unsupported,
}
