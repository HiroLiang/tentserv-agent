//! Model support status vocabulary and pure resolver.

use serde::{Deserialize, Serialize};

use super::domain::{
    MlxRuntimeFamily, ModelCapability, ModelCapabilityProof, ModelCapabilityProofStatus,
    ModelFormat, ModelMetadata,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelSupportStatus {
    Verified,
    Failed,
    Supported,
    Unknown,
    Unsupported,
    Stale,
}

impl ModelSupportStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Failed => "failed",
            Self::Supported => "supported",
            Self::Unknown => "unknown",
            Self::Unsupported => "unsupported",
            Self::Stale => "stale",
        }
    }
}

impl std::fmt::Display for ModelSupportStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelSupportEvidenceKind {
    HardIncompatibility,
    LocalProof,
    SupportHint,
    CapabilityMetadata,
    None,
}

impl ModelSupportEvidenceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HardIncompatibility => "hard-incompatibility",
            Self::LocalProof => "local-proof",
            Self::SupportHint => "support-hint",
            Self::CapabilityMetadata => "capability-metadata",
            Self::None => "none",
        }
    }
}

impl std::fmt::Display for ModelSupportEvidenceKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelSupportHintStatus {
    Supported,
    Unsupported,
}

impl ModelSupportHintStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Unsupported => "unsupported",
        }
    }
}

impl std::fmt::Display for ModelSupportHintStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSupportHint {
    pub capability: ModelCapability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_format: Option<ModelFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mlx_runtime_family: Option<MlxRuntimeFamily>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    pub status: ModelSupportHintStatus,
    pub reason: String,
}

impl ModelSupportHint {
    pub fn supported(capability: ModelCapability, reason: impl Into<String>) -> Self {
        Self {
            capability,
            primary_format: None,
            mlx_runtime_family: None,
            backend: None,
            status: ModelSupportHintStatus::Supported,
            reason: reason.into(),
        }
    }

    pub fn unsupported(capability: ModelCapability, reason: impl Into<String>) -> Self {
        Self {
            capability,
            primary_format: None,
            mlx_runtime_family: None,
            backend: None,
            status: ModelSupportHintStatus::Unsupported,
            reason: reason.into(),
        }
    }

    pub fn with_primary_format(mut self, primary_format: ModelFormat) -> Self {
        self.primary_format = Some(primary_format);
        self
    }

    pub fn with_mlx_runtime_family(mut self, mlx_runtime_family: MlxRuntimeFamily) -> Self {
        self.mlx_runtime_family = Some(mlx_runtime_family);
        self
    }

    pub fn with_backend(mut self, backend: impl Into<String>) -> Self {
        self.backend = Some(backend.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSupportQuery {
    pub capability: ModelCapability,
    pub primary_format: ModelFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mlx_runtime_family: Option<MlxRuntimeFamily>,
    pub backend: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_profile_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_class: Option<String>,
}

impl ModelSupportQuery {
    pub fn from_metadata(metadata: &ModelMetadata, capability: ModelCapability) -> Self {
        Self {
            capability,
            primary_format: metadata.primary_format,
            mlx_runtime_family: metadata.mlx_runtime_family,
            backend: backend_label(metadata.mlx_runtime_family, metadata.primary_format),
            runtime_version: None,
            runtime_profile: None,
            runtime_profile_version: None,
            platform: None,
            device_class: None,
        }
    }

    pub fn with_runtime_profile(
        mut self,
        runtime_profile: impl Into<String>,
        runtime_profile_version: u32,
    ) -> Self {
        self.runtime_profile = Some(runtime_profile.into());
        self.runtime_profile_version = Some(runtime_profile_version);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSupportResolution {
    pub status: ModelSupportStatus,
    pub reason: String,
    pub evidence: ModelSupportEvidenceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
}

impl ModelSupportResolution {
    fn new(
        status: ModelSupportStatus,
        evidence: ModelSupportEvidenceKind,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            status,
            evidence,
            reason: reason.into(),
            stale_reason: None,
            failure_reason: None,
        }
    }

    fn with_stale_reason(mut self, stale_reason: impl Into<String>) -> Self {
        self.stale_reason = Some(stale_reason.into());
        self
    }

    fn with_failure_reason(mut self, failure_reason: impl Into<String>) -> Self {
        self.failure_reason = Some(failure_reason.into());
        self
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ModelSupportStatusResolver;

impl ModelSupportStatusResolver {
    pub fn resolve(
        &self,
        metadata: &ModelMetadata,
        query: &ModelSupportQuery,
        proofs: &[ModelCapabilityProof],
        hints: &[ModelSupportHint],
    ) -> ModelSupportResolution {
        if !metadata.supports_capability(query.capability) {
            return ModelSupportResolution::new(
                ModelSupportStatus::Unsupported,
                ModelSupportEvidenceKind::HardIncompatibility,
                format!(
                    "model {} does not declare {} capability",
                    metadata.model_ref, query.capability
                ),
            );
        }

        if let Some(proof) = latest_matching_proof(metadata, query, proofs) {
            if let Some(stale_reason) = proof_stale_reason(proof, query) {
                return ModelSupportResolution::new(
                    ModelSupportStatus::Stale,
                    ModelSupportEvidenceKind::LocalProof,
                    "local proof exists but no longer matches the requested runtime tuple",
                )
                .with_stale_reason(stale_reason);
            }

            return match proof.status {
                ModelCapabilityProofStatus::Verified => ModelSupportResolution::new(
                    ModelSupportStatus::Verified,
                    ModelSupportEvidenceKind::LocalProof,
                    format!(
                        "latest local {} proof verified {}",
                        proof.source, query.capability
                    ),
                ),
                ModelCapabilityProofStatus::Failed => {
                    let failure_reason = proof
                        .error
                        .clone()
                        .unwrap_or_else(|| "local proof failed".to_string());
                    ModelSupportResolution::new(
                        ModelSupportStatus::Failed,
                        ModelSupportEvidenceKind::LocalProof,
                        format!("latest local {} proof failed", proof.source),
                    )
                    .with_failure_reason(failure_reason)
                }
            };
        }

        if let Some(hint) = best_matching_hint(query, hints, ModelSupportHintStatus::Unsupported) {
            return ModelSupportResolution::new(
                ModelSupportStatus::Unsupported,
                ModelSupportEvidenceKind::SupportHint,
                hint.reason.clone(),
            );
        }

        if let Some(hint) = best_matching_hint(query, hints, ModelSupportHintStatus::Supported) {
            return ModelSupportResolution::new(
                ModelSupportStatus::Supported,
                ModelSupportEvidenceKind::SupportHint,
                hint.reason.clone(),
            );
        }

        ModelSupportResolution::new(
            ModelSupportStatus::Unknown,
            ModelSupportEvidenceKind::CapabilityMetadata,
            format!(
                "model declares {} capability but no support hint or local proof applies",
                query.capability
            ),
        )
    }
}

fn latest_matching_proof<'a>(
    metadata: &ModelMetadata,
    query: &ModelSupportQuery,
    proofs: &'a [ModelCapabilityProof],
) -> Option<&'a ModelCapabilityProof> {
    let matching = proofs
        .iter()
        .rev()
        .filter(|proof| {
            proof.model_ref == metadata.model_ref && proof.capability == query.capability
        })
        .collect::<Vec<_>>();

    matching
        .iter()
        .copied()
        .find(|proof| proof_stale_reason(proof, query).is_none())
        .or_else(|| matching.first().copied())
}

fn proof_stale_reason(proof: &ModelCapabilityProof, query: &ModelSupportQuery) -> Option<String> {
    if proof.primary_format != query.primary_format {
        return Some(format!(
            "primary format changed from {} to {}",
            proof.primary_format, query.primary_format
        ));
    }

    if proof.mlx_runtime_family != query.mlx_runtime_family {
        return Some(format!(
            "MLX runtime family changed from {} to {}",
            optional_runtime_family(proof.mlx_runtime_family),
            optional_runtime_family(query.mlx_runtime_family)
        ));
    }

    if proof.backend != query.backend {
        return Some(format!(
            "backend changed from {} to {}",
            proof.backend, query.backend
        ));
    }

    if proof.runtime_version.is_some()
        && query.runtime_version.is_some()
        && proof.runtime_version != query.runtime_version
    {
        return Some(format!(
            "runtime version changed from {} to {}",
            proof.runtime_version.as_deref().unwrap_or("unknown"),
            query.runtime_version.as_deref().unwrap_or("unknown")
        ));
    }

    if query.runtime_profile.is_some() && proof.runtime_profile != query.runtime_profile {
        return Some(format!(
            "runtime profile changed from {} to {}",
            optional_text(proof.runtime_profile.as_deref()),
            optional_text(query.runtime_profile.as_deref())
        ));
    }

    if query.runtime_profile_version.is_some()
        && proof.runtime_profile_version != query.runtime_profile_version
    {
        return Some(format!(
            "runtime profile version changed from {} to {}",
            optional_u32(proof.runtime_profile_version),
            optional_u32(query.runtime_profile_version)
        ));
    }

    None
}

fn optional_runtime_family(runtime_family: Option<MlxRuntimeFamily>) -> String {
    runtime_family
        .map(|family| family.as_str().to_string())
        .unwrap_or_else(|| "none".to_string())
}

fn optional_text(value: Option<&str>) -> String {
    value.unwrap_or("none").to_string()
}

fn optional_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string())
}

fn best_matching_hint<'a>(
    query: &ModelSupportQuery,
    hints: &'a [ModelSupportHint],
    status: ModelSupportHintStatus,
) -> Option<&'a ModelSupportHint> {
    hints
        .iter()
        .rev()
        .find(|hint| hint.status == status && hint_matches(query, hint))
}

fn hint_matches(query: &ModelSupportQuery, hint: &ModelSupportHint) -> bool {
    if hint.capability != query.capability {
        return false;
    }

    if hint
        .primary_format
        .is_some_and(|primary_format| primary_format != query.primary_format)
    {
        return false;
    }

    if hint
        .mlx_runtime_family
        .is_some_and(|family| Some(family) != query.mlx_runtime_family)
    {
        return false;
    }

    if hint
        .backend
        .as_ref()
        .is_some_and(|backend| backend != &query.backend)
    {
        return false;
    }

    true
}

fn backend_label(
    mlx_runtime_family: Option<MlxRuntimeFamily>,
    primary_format: ModelFormat,
) -> String {
    mlx_runtime_family
        .map(|family| family.as_str().to_string())
        .unwrap_or_else(|| primary_format.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::model::domain::{
        default_model_capability_source, ModelCapabilityProofSource, ModelRef, ModelSourceKind,
    };

    #[test]
    fn resolver_returns_unsupported_when_model_lacks_capability() {
        let metadata = metadata_with_capabilities(vec![ModelCapability::Chat]);
        let query = query_for(ModelCapability::Embedding);

        let resolution = resolver().resolve(&metadata, &query, &[], &[]);

        assert_eq!(resolution.status, ModelSupportStatus::Unsupported);
        assert_eq!(
            resolution.evidence,
            ModelSupportEvidenceKind::HardIncompatibility
        );
    }

    #[test]
    fn resolver_returns_verified_for_matching_local_proof() {
        let metadata = metadata_with_capabilities(vec![ModelCapability::Chat]);
        let query = query_for(ModelCapability::Chat);
        let proofs = vec![proof_for(
            &metadata,
            ModelCapability::Chat,
            ModelCapabilityProofStatus::Verified,
            "gguf",
            None,
        )];

        let resolution = resolver().resolve(&metadata, &query, &proofs, &[]);

        assert_eq!(resolution.status, ModelSupportStatus::Verified);
        assert_eq!(resolution.evidence, ModelSupportEvidenceKind::LocalProof);
    }

    #[test]
    fn resolver_returns_failed_for_matching_failed_local_proof() {
        let metadata = metadata_with_capabilities(vec![ModelCapability::Chat]);
        let query = query_for(ModelCapability::Chat);
        let proofs = vec![proof_for(
            &metadata,
            ModelCapability::Chat,
            ModelCapabilityProofStatus::Failed,
            "gguf",
            Some("backend failed to load model".to_string()),
        )];

        let resolution = resolver().resolve(&metadata, &query, &proofs, &[]);

        assert_eq!(resolution.status, ModelSupportStatus::Failed);
        assert_eq!(resolution.evidence, ModelSupportEvidenceKind::LocalProof);
        assert_eq!(
            resolution.failure_reason.as_deref(),
            Some("backend failed to load model")
        );
    }

    #[test]
    fn resolver_returns_stale_for_backend_mismatch() {
        let metadata = metadata_with_capabilities(vec![ModelCapability::Chat]);
        let query = query_for(ModelCapability::Chat);
        let proofs = vec![proof_for(
            &metadata,
            ModelCapability::Chat,
            ModelCapabilityProofStatus::Verified,
            "llama-cpp",
            None,
        )];

        let resolution = resolver().resolve(&metadata, &query, &proofs, &[]);

        assert_eq!(resolution.status, ModelSupportStatus::Stale);
        assert_eq!(resolution.evidence, ModelSupportEvidenceKind::LocalProof);
        assert_eq!(
            resolution.stale_reason.as_deref(),
            Some("backend changed from llama-cpp to gguf")
        );
    }

    #[test]
    fn resolver_returns_supported_for_matching_positive_hint() {
        let metadata = metadata_with_capabilities(vec![ModelCapability::Chat]);
        let query = query_for(ModelCapability::Chat);
        let hints = vec![ModelSupportHint::supported(
            ModelCapability::Chat,
            "curated fixture supports GGUF chat",
        )
        .with_primary_format(ModelFormat::Gguf)
        .with_backend("gguf")];

        let resolution = resolver().resolve(&metadata, &query, &[], &hints);

        assert_eq!(resolution.status, ModelSupportStatus::Supported);
        assert_eq!(resolution.evidence, ModelSupportEvidenceKind::SupportHint);
        assert_eq!(resolution.reason, "curated fixture supports GGUF chat");
    }

    #[test]
    fn resolver_returns_unsupported_for_matching_negative_hint() {
        let metadata = metadata_with_capabilities(vec![ModelCapability::Chat]);
        let query = query_for(ModelCapability::Chat);
        let hints = vec![ModelSupportHint::unsupported(
            ModelCapability::Chat,
            "known incompatible tokenizer",
        )];

        let resolution = resolver().resolve(&metadata, &query, &[], &hints);

        assert_eq!(resolution.status, ModelSupportStatus::Unsupported);
        assert_eq!(resolution.evidence, ModelSupportEvidenceKind::SupportHint);
        assert_eq!(resolution.reason, "known incompatible tokenizer");
    }

    #[test]
    fn resolver_returns_unknown_without_hint_or_proof() {
        let metadata = metadata_with_capabilities(vec![ModelCapability::Chat]);
        let query = query_for(ModelCapability::Chat);

        let resolution = resolver().resolve(&metadata, &query, &[], &[]);

        assert_eq!(resolution.status, ModelSupportStatus::Unknown);
        assert_eq!(
            resolution.evidence,
            ModelSupportEvidenceKind::CapabilityMetadata
        );
    }

    #[test]
    fn failed_local_proof_beats_supported_hint() {
        let metadata = metadata_with_capabilities(vec![ModelCapability::Chat]);
        let query = query_for(ModelCapability::Chat);
        let proofs = vec![proof_for(
            &metadata,
            ModelCapability::Chat,
            ModelCapabilityProofStatus::Failed,
            "gguf",
            Some("runtime failed".to_string()),
        )];
        let hints = vec![ModelSupportHint::supported(
            ModelCapability::Chat,
            "curated fixture supports this tuple",
        )];

        let resolution = resolver().resolve(&metadata, &query, &proofs, &hints);

        assert_eq!(resolution.status, ModelSupportStatus::Failed);
        assert_eq!(resolution.evidence, ModelSupportEvidenceKind::LocalProof);
        assert_eq!(resolution.failure_reason.as_deref(), Some("runtime failed"));
    }

    #[test]
    fn verified_local_proof_beats_unsupported_hint() {
        let metadata = metadata_with_capabilities(vec![ModelCapability::Chat]);
        let query = query_for(ModelCapability::Chat);
        let proofs = vec![proof_for(
            &metadata,
            ModelCapability::Chat,
            ModelCapabilityProofStatus::Verified,
            "gguf",
            None,
        )];
        let hints = vec![ModelSupportHint::unsupported(
            ModelCapability::Chat,
            "old support record no longer applies",
        )];

        let resolution = resolver().resolve(&metadata, &query, &proofs, &hints);

        assert_eq!(resolution.status, ModelSupportStatus::Verified);
        assert_eq!(resolution.evidence, ModelSupportEvidenceKind::LocalProof);
    }

    #[test]
    fn latest_matching_proof_wins() {
        let metadata = metadata_with_capabilities(vec![ModelCapability::Chat]);
        let query = query_for(ModelCapability::Chat);
        let proofs = vec![
            proof_for(
                &metadata,
                ModelCapability::Chat,
                ModelCapabilityProofStatus::Failed,
                "gguf",
                Some("old failure".to_string()),
            ),
            proof_for(
                &metadata,
                ModelCapability::Chat,
                ModelCapabilityProofStatus::Verified,
                "gguf",
                None,
            ),
        ];

        let resolution = resolver().resolve(&metadata, &query, &proofs, &[]);

        assert_eq!(resolution.status, ModelSupportStatus::Verified);
    }

    #[test]
    fn matching_tuple_proof_wins_over_stale_same_capability_proof() {
        let metadata = metadata_with_capabilities(vec![ModelCapability::Chat]);
        let query = query_for(ModelCapability::Chat);
        let proofs = vec![
            proof_for(
                &metadata,
                ModelCapability::Chat,
                ModelCapabilityProofStatus::Verified,
                "gguf",
                None,
            ),
            proof_for(
                &metadata,
                ModelCapability::Chat,
                ModelCapabilityProofStatus::Failed,
                "llama-cpp",
                Some("stale tuple failure".to_string()),
            ),
        ];

        let resolution = resolver().resolve(&metadata, &query, &proofs, &[]);

        assert_eq!(resolution.status, ModelSupportStatus::Verified);
        assert_eq!(resolution.failure_reason, None);
        assert_eq!(resolution.stale_reason, None);
    }

    #[test]
    fn resolver_returns_stale_when_runtime_profile_version_changes() {
        let metadata = metadata_with_capabilities(vec![ModelCapability::Chat]);
        let query =
            query_for(ModelCapability::Chat).with_runtime_profile("local-chat-llama-cpp", 2);
        let mut proof = proof_for(
            &metadata,
            ModelCapability::Chat,
            ModelCapabilityProofStatus::Verified,
            "gguf",
            None,
        );
        proof.runtime_profile = Some("local-chat-llama-cpp".to_string());
        proof.runtime_profile_version = Some(1);

        let resolution = resolver().resolve(&metadata, &query, &[proof], &[]);

        assert_eq!(resolution.status, ModelSupportStatus::Stale);
        assert_eq!(
            resolution.stale_reason.as_deref(),
            Some("runtime profile version changed from 1 to 2")
        );
    }

    fn resolver() -> ModelSupportStatusResolver {
        ModelSupportStatusResolver
    }

    fn query_for(capability: ModelCapability) -> ModelSupportQuery {
        ModelSupportQuery {
            capability,
            primary_format: ModelFormat::Gguf,
            mlx_runtime_family: None,
            backend: "gguf".to_string(),
            runtime_version: None,
            runtime_profile: None,
            runtime_profile_version: None,
            platform: Some("macos".to_string()),
            device_class: Some("apple-silicon".to_string()),
        }
    }

    fn metadata_with_capabilities(capabilities: Vec<ModelCapability>) -> ModelMetadata {
        let model_ref = ModelRef::parse("a".repeat(64)).expect("model ref");
        ModelMetadata {
            short_ref: model_ref.short_ref().to_string(),
            model_ref,
            source_kind: ModelSourceKind::Local,
            source_repo: None,
            source_revision: None,
            source_path: Some("/tmp/model.gguf".to_string()),
            primary_format: ModelFormat::Gguf,
            detected_formats: vec![ModelFormat::Gguf],
            mlx_runtime_family: None,
            model_capabilities: capabilities,
            model_capability_source: default_model_capability_source(),
            file_count: 1,
            total_bytes: 42,
            imported_at: "2026-06-12T00:00:00Z".to_string(),
        }
    }

    fn proof_for(
        metadata: &ModelMetadata,
        capability: ModelCapability,
        status: ModelCapabilityProofStatus,
        backend: impl Into<String>,
        error: Option<String>,
    ) -> ModelCapabilityProof {
        ModelCapabilityProof {
            model_ref: metadata.model_ref.clone(),
            capability,
            status,
            source: ModelCapabilityProofSource::ServerStart,
            primary_format: metadata.primary_format,
            mlx_runtime_family: metadata.mlx_runtime_family,
            backend: backend.into(),
            runtime_version: None,
            runtime_profile: None,
            runtime_profile_version: None,
            server_ref: Some("server-ref".to_string()),
            checked_at: "2026-06-12T00:00:00Z".to_string(),
            error,
        }
    }
}
