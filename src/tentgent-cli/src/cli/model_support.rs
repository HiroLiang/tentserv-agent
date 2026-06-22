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
    pub runtime_profile: Option<String>,
    pub runtime_profile_version: Option<u32>,
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

    pub fn runtime_profile_label(&self) -> Option<String> {
        match (
            self.runtime_profile.as_deref(),
            self.runtime_profile_version,
        ) {
            (Some(profile), Some(version)) => Some(format!("{profile}@v{version}")),
            (Some(profile), None) => Some(profile.to_string()),
            (None, _) => None,
        }
    }
}

pub fn model_support_summaries(
    metadata: &ModelMetadata,
    proofs: &[ModelCapabilityProof],
) -> Vec<ModelSupportSummary> {
    model_support_summaries_with_runtime_profile(metadata, proofs, None)
}

pub fn model_support_summaries_with_runtime_profile(
    metadata: &ModelMetadata,
    proofs: &[ModelCapabilityProof],
    runtime_profile: Option<(&str, u32)>,
) -> Vec<ModelSupportSummary> {
    let hints = built_in_support_hints_for_model(metadata);
    inspect_capabilities(metadata, proofs)
        .into_iter()
        .map(|capability| {
            let mut query = ModelSupportQuery::from_metadata(metadata, capability);
            if let Some((profile, version)) = runtime_profile {
                query = query.with_runtime_profile(profile, version);
            }
            let resolution = ModelSupportStatusResolver.resolve(metadata, &query, proofs, &hints);
            support_summary_from_resolution(metadata, capability, query, resolution, proofs)
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
        format!("execution_backend: {}", summary.backend),
    ];

    if let Some(profile) = summary.runtime_profile_label() {
        lines.push(format!("runtime_profile: {profile}"));
    }
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

pub fn model_support_diagnostic_lines(
    summary: &ModelSupportSummary,
    model_ref: Option<&str>,
) -> Vec<String> {
    let mut lines = model_support_detail_lines(summary);
    if let Some(guidance) = model_support_recovery_guidance(summary, model_ref) {
        lines.push(format!("recovery: {guidance}"));
    }
    if let Some(action) = model_support_next_action(summary, model_ref) {
        lines.push(format!("next_action: {action}"));
    }
    lines
}

pub fn model_support_recovery_guidance(
    summary: &ModelSupportSummary,
    model_ref: Option<&str>,
) -> Option<String> {
    let capability = summary.capability.as_str();
    match summary.status {
        ModelSupportStatus::Failed => Some(match model_ref {
            Some(model_ref) => format!(
                "fix the runtime/backend issue, clear the failed proof, then retry the route or run tentgent model capability verify {model_ref} {capability}"
            ),
            None => "fix the runtime/backend issue, clear the failed proof, then retry the route or rerun verification".to_string(),
        }),
        ModelSupportStatus::Stale => Some(match model_ref {
            Some(model_ref) => format!(
                "refresh evidence for the current runtime tuple, or clear stale proof with tentgent model capability proof clear {model_ref} {capability} before retrying"
            ),
            None => "refresh evidence for the current runtime tuple, or clear stale proof before retrying".to_string(),
        }),
        ModelSupportStatus::Unknown => Some(match model_ref {
            Some(model_ref) => format!(
                "run tentgent model capability verify {model_ref} {capability} before relying on this tuple, or use --allow-unverified for an explicit local server retry"
            ),
            None => "run verification before relying on this tuple, or use --allow-unverified for an explicit local server retry".to_string(),
        }),
        ModelSupportStatus::Unsupported if !summary.declared => Some(
            "add capability metadata only if the model is intended to support this capability"
                .to_string(),
        ),
        ModelSupportStatus::Unsupported => {
            Some("choose a different model, capability, or backend tuple".to_string())
        }
        ModelSupportStatus::Verified | ModelSupportStatus::Supported => None,
    }
}

pub fn model_support_next_action(
    summary: &ModelSupportSummary,
    model_ref: Option<&str>,
) -> Option<String> {
    let model_ref = model_ref?;
    let capability = summary.capability.as_str();
    match summary.status {
        ModelSupportStatus::Failed | ModelSupportStatus::Stale => Some(format!(
            "tentgent model capability proof clear {model_ref} {capability}"
        )),
        ModelSupportStatus::Unknown => Some(format!(
            "tentgent model capability verify {model_ref} {capability}"
        )),
        ModelSupportStatus::Unsupported if !summary.declared => Some(format!(
            "tentgent model capability add {model_ref} {capability}"
        )),
        ModelSupportStatus::Unsupported => Some(format!("tentgent model inspect {model_ref}")),
        ModelSupportStatus::Verified | ModelSupportStatus::Supported => None,
    }
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
    proofs: &[ModelCapabilityProof],
) -> ModelSupportSummary {
    let proof_profile = latest_proof_runtime_profile(metadata, &query, proofs);
    let runtime_profile = query.runtime_profile.clone().or_else(|| {
        proof_profile
            .as_ref()
            .and_then(|(profile, _)| profile.clone())
    });
    let runtime_profile_version = query
        .runtime_profile_version
        .or_else(|| proof_profile.and_then(|(_, version)| version));
    ModelSupportSummary {
        capability,
        declared: metadata.supports_capability(capability),
        status: resolution.status,
        evidence: resolution.evidence,
        backend: query.backend,
        mlx_runtime_family: query.mlx_runtime_family,
        runtime_version: query.runtime_version,
        runtime_profile,
        runtime_profile_version,
        reason: resolution.reason,
        stale_reason: resolution.stale_reason,
        failure_reason: resolution.failure_reason,
    }
}

fn latest_proof_runtime_profile(
    metadata: &ModelMetadata,
    query: &ModelSupportQuery,
    proofs: &[ModelCapabilityProof],
) -> Option<(Option<String>, Option<u32>)> {
    proofs
        .iter()
        .rev()
        .find(|proof| {
            proof.model_ref == metadata.model_ref
                && proof.capability == query.capability
                && proof.primary_format == query.primary_format
                && proof.mlx_runtime_family == query.mlx_runtime_family
                && proof.backend == query.backend
        })
        .map(|proof| (proof.runtime_profile.clone(), proof.runtime_profile_version))
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
            runtime_profile: None,
            runtime_profile_version: None,
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
        assert!(!lines
            .iter()
            .any(|line| line.starts_with("runtime_profile:")));
        assert!(lines
            .iter()
            .any(|line| line == "execution_backend: safetensors"));
        assert!(lines.iter().any(|line| line.starts_with("reason: ")));
    }

    #[test]
    fn profile_aware_summary_includes_selected_runtime_profile() {
        let metadata = metadata_with_capabilities([ModelCapability::Chat]);
        let summary = model_support_summaries_with_runtime_profile(
            &metadata,
            &[],
            Some(("local-chat-mlx-v1", 1)),
        )
        .into_iter()
        .next()
        .expect("support summary");
        let lines = model_support_detail_lines(&summary);

        assert!(lines
            .iter()
            .any(|line| line == "runtime_profile: local-chat-mlx-v1@v1"));
    }

    #[test]
    fn detail_lines_include_profile_recorded_by_latest_proof() {
        let metadata = metadata_with_capabilities([ModelCapability::Chat]);
        let mut proof = proof_for(
            &metadata,
            ModelCapability::Chat,
            ModelCapabilityProofStatus::Verified,
            None,
        );
        proof.runtime_profile = Some("local-chat-transformers-peft-v1".to_string());
        proof.runtime_profile_version = Some(2);
        let summary = model_support_summaries(&metadata, &[proof])
            .into_iter()
            .next()
            .expect("support summary");
        let lines = model_support_detail_lines(&summary);

        assert!(lines
            .iter()
            .any(|line| line == "runtime_profile: local-chat-transformers-peft-v1@v2"));
    }

    #[test]
    fn diagnostic_lines_include_copyable_next_action_for_unknown_support() {
        let metadata = metadata_with_capabilities([ModelCapability::Chat]);
        let summary = model_support_summaries(&metadata, &[])
            .into_iter()
            .next()
            .expect("support summary");
        let lines = model_support_diagnostic_lines(&summary, Some(&metadata.short_ref));

        assert!(lines.iter().any(|line| {
            line == "next_action: tentgent model capability verify 0123456789ab chat"
        }));
    }

    #[test]
    fn unsupported_missing_capability_points_to_capability_metadata() {
        let summary = summary_for_status(ModelSupportStatus::Unsupported, false);

        assert_eq!(
            model_support_recovery_guidance(&summary, Some("0123456789ab")).as_deref(),
            Some(
                "add capability metadata only if the model is intended to support this capability"
            )
        );
        assert_eq!(
            model_support_next_action(&summary, Some("0123456789ab")).as_deref(),
            Some("tentgent model capability add 0123456789ab chat")
        );
    }

    #[test]
    fn unsupported_declared_capability_points_to_another_tuple() {
        let summary = summary_for_status(ModelSupportStatus::Unsupported, true);

        assert_eq!(
            model_support_recovery_guidance(&summary, Some("0123456789ab")).as_deref(),
            Some("choose a different model, capability, or backend tuple")
        );
        assert_eq!(
            model_support_next_action(&summary, Some("0123456789ab")).as_deref(),
            Some("tentgent model inspect 0123456789ab")
        );
    }

    #[test]
    fn next_action_clears_failed_proofs() {
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

        assert_eq!(
            model_support_next_action(&summary, Some("0123456789ab")).as_deref(),
            Some("tentgent model capability proof clear 0123456789ab chat")
        );
    }

    #[test]
    fn diagnostic_lines_include_failed_recovery_guidance() {
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
        let lines = model_support_diagnostic_lines(&summary, Some("0123456789ab"));

        assert!(lines.iter().any(|line| line == "failure: runtime failed"));
        assert!(lines.iter().any(|line| {
            line == "recovery: fix the runtime/backend issue, clear the failed proof, then retry the route or run tentgent model capability verify 0123456789ab chat"
        }));
        assert!(lines.iter().any(|line| {
            line == "next_action: tentgent model capability proof clear 0123456789ab chat"
        }));
    }

    #[test]
    fn diagnostic_lines_include_stale_recovery_guidance() {
        let metadata = metadata_with_capabilities([ModelCapability::Chat]);
        let mut proof = proof_for(
            &metadata,
            ModelCapability::Chat,
            ModelCapabilityProofStatus::Failed,
            Some("old backend failed".to_string()),
        );
        proof.backend = "old-backend".to_string();
        let summary = model_support_summaries(&metadata, &[proof])
            .into_iter()
            .next()
            .expect("support summary");
        let lines = model_support_diagnostic_lines(&summary, Some("0123456789ab"));

        assert_eq!(summary.status, ModelSupportStatus::Stale);
        assert!(lines
            .iter()
            .any(|line| line == "stale: backend changed from old-backend to safetensors"));
        assert!(lines.iter().any(|line| {
            line == "recovery: refresh evidence for the current runtime tuple, or clear stale proof with tentgent model capability proof clear 0123456789ab chat before retrying"
        }));
        assert!(lines.iter().any(|line| {
            line == "next_action: tentgent model capability proof clear 0123456789ab chat"
        }));
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
            runtime_profile: None,
            runtime_profile_version: None,
            server_ref: None,
            checked_at: "2026-06-15T00:00:00Z".to_string(),
            error,
        }
    }

    fn summary_for_status(status: ModelSupportStatus, declared: bool) -> ModelSupportSummary {
        ModelSupportSummary {
            capability: ModelCapability::Chat,
            declared,
            status,
            evidence: ModelSupportEvidenceKind::SupportHint,
            backend: "mlx-lm".to_string(),
            mlx_runtime_family: None,
            runtime_version: None,
            runtime_profile: None,
            runtime_profile_version: None,
            reason: "known unsupported runtime tuple".to_string(),
            stale_reason: None,
            failure_reason: None,
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
