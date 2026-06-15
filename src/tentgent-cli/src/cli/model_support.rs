use tentgent_kernel::features::model::domain::{
    MlxRuntimeFamily, ModelCapability, ModelCapabilityProof, ModelMetadata,
    MODEL_CAPABILITY_CANONICAL_ORDER,
};
use tentgent_kernel::features::model::support_catalog::built_in_support_hints_for_model;
use tentgent_kernel::features::model::support_status::{
    ModelSupportEvidenceKind, ModelSupportQuery, ModelSupportResolution, ModelSupportStatus,
    ModelSupportStatusResolver,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSupportSummary {
    pub capability: ModelCapability,
    pub declared: bool,
    pub status: ModelSupportStatus,
    pub evidence: ModelSupportEvidenceKind,
    pub backend: String,
    pub mlx_runtime_family: Option<MlxRuntimeFamily>,
    pub runtime_version: Option<String>,
    pub reason: String,
    pub stale_reason: Option<String>,
    pub failure_reason: Option<String>,
}

impl ModelSupportSummary {
    pub fn short_label(&self) -> String {
        format!("{} {}", self.status.as_str(), self.capability.as_str())
    }

    pub fn short_reason(&self) -> &str {
        self.failure_reason
            .as_deref()
            .or(self.stale_reason.as_deref())
            .filter(|reason| !reason.is_empty())
            .unwrap_or(&self.reason)
    }
}

pub fn model_support_summaries(
    metadata: &ModelMetadata,
    proofs: &[ModelCapabilityProof],
) -> Vec<ModelSupportSummary> {
    let hints = built_in_support_hints_for_model(metadata);
    inspect_capabilities(metadata, proofs)
        .into_iter()
        .map(|capability| {
            let query = ModelSupportQuery::from_metadata(metadata, capability);
            let resolution = ModelSupportStatusResolver.resolve(metadata, &query, proofs, &hints);
            support_summary_from_resolution(metadata, capability, query, resolution)
        })
        .collect()
}

pub fn primary_model_support_summary<'a>(
    summaries: &'a [ModelSupportSummary],
) -> Option<&'a ModelSupportSummary> {
    summaries.iter().min_by_key(|summary| {
        (
            support_attention_rank(summary.status),
            capability_order_rank(summary.capability),
        )
    })
}

pub fn model_support_list_label(
    metadata: &ModelMetadata,
    proofs: &[ModelCapabilityProof],
) -> String {
    let summaries = model_support_summaries(metadata, proofs);
    let Some(primary) = primary_model_support_summary(&summaries) else {
        return "none".to_string();
    };
    let extra = summaries.len().saturating_sub(1);
    if extra == 0 {
        primary.short_label()
    } else {
        format!("{} (+{extra})", primary.short_label())
    }
}

pub fn model_support_detail_lines(summary: &ModelSupportSummary) -> Vec<String> {
    let mut lines = vec![
        summary.capability.as_str().to_string(),
        format!("declared: {}", if summary.declared { "yes" } else { "no" }),
        format!("status: {}", summary.status.as_str()),
        format!("evidence: {}", summary.evidence.as_str()),
        format!("backend: {}", summary.backend),
    ];

    if let Some(family) = summary.mlx_runtime_family {
        lines.push(format!("mlx_runtime_family: {}", family.as_str()));
    }
    if let Some(version) = summary.runtime_version.as_deref() {
        lines.push(format!("runtime_version: {version}"));
    }
    if let Some(stale_reason) = summary.stale_reason.as_deref() {
        lines.push(format!("stale: {stale_reason}"));
    }
    if let Some(failure_reason) = summary.failure_reason.as_deref() {
        lines.push(format!("failure: {failure_reason}"));
    }
    if !summary.reason.is_empty() {
        lines.push(format!("reason: {}", summary.reason));
    }

    lines
}

pub fn support_status_is_healthy(status: ModelSupportStatus) -> bool {
    matches!(
        status,
        ModelSupportStatus::Verified | ModelSupportStatus::Supported
    )
}

fn support_summary_from_resolution(
    metadata: &ModelMetadata,
    capability: ModelCapability,
    query: ModelSupportQuery,
    resolution: ModelSupportResolution,
) -> ModelSupportSummary {
    ModelSupportSummary {
        capability,
        declared: metadata.supports_capability(capability),
        status: resolution.status,
        evidence: resolution.evidence,
        backend: query.backend,
        mlx_runtime_family: query.mlx_runtime_family,
        runtime_version: query.runtime_version,
        reason: resolution.reason,
        stale_reason: resolution.stale_reason,
        failure_reason: resolution.failure_reason,
    }
}

fn inspect_capabilities(
    metadata: &ModelMetadata,
    proofs: &[ModelCapabilityProof],
) -> Vec<ModelCapability> {
    MODEL_CAPABILITY_CANONICAL_ORDER
        .into_iter()
        .filter(|capability| {
            metadata.supports_capability(*capability)
                || proofs.iter().any(|proof| proof.capability == *capability)
        })
        .collect()
}

fn support_attention_rank(status: ModelSupportStatus) -> u8 {
    match status {
        ModelSupportStatus::Failed => 0,
        ModelSupportStatus::Stale => 1,
        ModelSupportStatus::Unsupported => 2,
        ModelSupportStatus::Unknown => 3,
        ModelSupportStatus::Supported => 4,
        ModelSupportStatus::Verified => 5,
    }
}

fn capability_order_rank(capability: ModelCapability) -> usize {
    MODEL_CAPABILITY_CANONICAL_ORDER
        .iter()
        .position(|candidate| *candidate == capability)
        .unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tentgent_kernel::features::model::domain::{
        default_model_capability_source, ModelCapabilityProofSource, ModelCapabilityProofStatus,
        ModelFormat, ModelRef, ModelSourceKind,
    };

    #[test]
    fn list_label_uses_most_actionable_status() {
        let metadata =
            metadata_with_capabilities([ModelCapability::Chat, ModelCapability::Embedding]);
        let proofs = vec![ModelCapabilityProof {
            model_ref: metadata.model_ref.clone(),
            capability: ModelCapability::Chat,
            status: tentgent_kernel::features::model::domain::ModelCapabilityProofStatus::Failed,
            source:
                tentgent_kernel::features::model::domain::ModelCapabilityProofSource::ManualProbe,
            primary_format: ModelFormat::Safetensors,
            mlx_runtime_family: None,
            backend: "safetensors".to_string(),
            runtime_version: None,
            server_ref: None,
            checked_at: "2026-06-15T00:00:00Z".to_string(),
            error: Some("runtime failed".to_string()),
        }];

        assert_eq!(
            model_support_list_label(&metadata, &proofs),
            "failed chat (+1)"
        );
    }

    #[test]
    fn detail_lines_include_status_reason_and_backend() {
        let metadata = metadata_with_capabilities([ModelCapability::Chat]);
        let summary = model_support_summaries(&metadata, &[])
            .into_iter()
            .next()
            .expect("support summary");
        let lines = model_support_detail_lines(&summary);

        assert!(lines.iter().any(|line| line == "chat"));
        assert!(lines.iter().any(|line| line == "status: unknown"));
        assert!(lines.iter().any(|line| line == "backend: safetensors"));
        assert!(lines.iter().any(|line| line.starts_with("reason: ")));
    }

    #[test]
    fn list_label_returns_none_when_model_has_no_support_tuples() {
        let metadata = metadata_with_capabilities([]);

        assert_eq!(model_support_list_label(&metadata, &[]), "none");
    }

    #[test]
    fn list_label_omits_extra_suffix_for_single_tuple() {
        let metadata = metadata_with_capabilities([ModelCapability::Chat]);

        assert_eq!(model_support_list_label(&metadata, &[]), "unknown chat");
    }

    #[test]
    fn detail_lines_include_failure_reason_from_failed_proof() {
        let metadata = metadata_with_capabilities([ModelCapability::Chat]);
        let proofs = vec![proof_for(
            &metadata,
            ModelCapability::Chat,
            ModelCapabilityProofStatus::Failed,
            Some("runtime failed".to_string()),
        )];
        let summary = model_support_summaries(&metadata, &proofs)
            .into_iter()
            .next()
            .expect("support summary");
        let lines = model_support_detail_lines(&summary);

        assert!(lines.iter().any(|line| line == "status: failed"));
        assert!(lines.iter().any(|line| line == "evidence: local-proof"));
        assert!(lines.iter().any(|line| line == "failure: runtime failed"));
        assert_eq!(summary.short_reason(), "runtime failed");
    }

    #[test]
    fn healthy_statuses_are_verified_and_supported_only() {
        assert!(support_status_is_healthy(ModelSupportStatus::Verified));
        assert!(support_status_is_healthy(ModelSupportStatus::Supported));
        assert!(!support_status_is_healthy(ModelSupportStatus::Failed));
        assert!(!support_status_is_healthy(ModelSupportStatus::Stale));
        assert!(!support_status_is_healthy(ModelSupportStatus::Unsupported));
        assert!(!support_status_is_healthy(ModelSupportStatus::Unknown));
    }

    fn proof_for(
        metadata: &ModelMetadata,
        capability: ModelCapability,
        status: ModelCapabilityProofStatus,
        error: Option<String>,
    ) -> ModelCapabilityProof {
        ModelCapabilityProof {
            model_ref: metadata.model_ref.clone(),
            capability,
            status,
            source: ModelCapabilityProofSource::ManualProbe,
            primary_format: metadata.primary_format,
            mlx_runtime_family: metadata.mlx_runtime_family,
            backend: metadata.primary_format.as_str().to_string(),
            runtime_version: None,
            server_ref: None,
            checked_at: "2026-06-15T00:00:00Z".to_string(),
            error,
        }
    }

    fn metadata_with_capabilities(
        capabilities: impl IntoIterator<Item = ModelCapability>,
    ) -> ModelMetadata {
        ModelMetadata {
            model_ref: ModelRef::parse(
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("model ref"),
            short_ref: "0123456789ab".to_string(),
            source_kind: ModelSourceKind::HuggingFace,
            source_repo: Some("example/model".to_string()),
            source_revision: Some("main".to_string()),
            source_path: None,
            primary_format: ModelFormat::Safetensors,
            detected_formats: vec![ModelFormat::Safetensors],
            mlx_runtime_family: None,
            model_capabilities: capabilities.into_iter().collect(),
            model_capability_source: default_model_capability_source(),
            file_count: 1,
            total_bytes: 1,
            imported_at: "2026-06-15T00:00:00Z".to_string(),
        }
    }
}
