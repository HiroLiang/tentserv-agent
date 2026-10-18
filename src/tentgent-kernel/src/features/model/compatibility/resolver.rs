use std::collections::BTreeSet;

use serde::Serialize;

use crate::features::model::domain::{ModelCapability, ModelCapabilityProofStatus, ModelRef};
use crate::features::model::support_status::{ModelSupportHintStatus, ModelSupportStatus};

use super::stale::stale_reasons;
use super::{
    CompatibilityError, CompatibilityEvidence, CompatibilityProofKey, CompatibilityTuple,
    MissingDimension, ProofFailureCode,
};

/// These facts are supplied by the caller's existing capability/policy checks.
/// The resolver neither probes a runtime nor silently derives hardware policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibilityConstraints {
    pub model_ref: ModelRef,
    pub declared_capabilities: Vec<ModelCapability>,
    pub hard_incompatibilities: BTreeSet<HardIncompatibility>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HardIncompatibility {
    CapabilityNotDeclared,
    BackendUnavailable,
    ProfileIncompatible,
    AdapterIncompatible,
    ExecutionShapeUnsupported,
}

/// A recommendation scoped to a complete tuple, not observed local execution.
/// There is intentionally no conversion from a legacy partial hint/filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibilityHint {
    pub tuple: CompatibilityTuple,
    pub status: ModelSupportHintStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", content = "dimension", rename_all = "kebab-case")]
pub enum CompatibilityStaleReason {
    MissingDimension(MissingDimension),
    ChangedDimension(MissingDimension),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CompatibilityResolutionEvidence {
    HardIncompatibility,
    ExactProof,
    IncompleteOrDifferentProof,
    ExactHint,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CompatibilityReason {
    HardIncompatibility,
    ExactProofVerified,
    ExactProofFailed,
    StaleProof,
    ExactHintSupported,
    ExactHintUnsupported,
    NoEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompatibilityResolution {
    pub status: ModelSupportStatus,
    pub evidence: CompatibilityResolutionEvidence,
    pub reason: CompatibilityReason,
    pub exact_key: Option<CompatibilityProofKey>,
    pub failure_code: Option<ProofFailureCode>,
    pub stale_reasons: BTreeSet<CompatibilityStaleReason>,
    pub hard_incompatibilities: BTreeSet<HardIncompatibility>,
}

impl CompatibilityResolution {
    fn new(
        status: ModelSupportStatus,
        evidence: CompatibilityResolutionEvidence,
        reason: CompatibilityReason,
    ) -> Self {
        Self {
            status,
            evidence,
            reason,
            exact_key: None,
            failure_code: None,
            stale_reasons: BTreeSet::new(),
            hard_incompatibilities: BTreeSet::new(),
        }
    }

    pub const fn summary(&self) -> &'static str {
        match self.reason {
            CompatibilityReason::HardIncompatibility => {
                "The requested tuple is incompatible with current capability or policy facts."
            }
            CompatibilityReason::ExactProofVerified => {
                "The exact requested observation has a current verified local proof."
            }
            CompatibilityReason::ExactProofFailed => {
                "The exact requested observation has a current failed local proof."
            }
            CompatibilityReason::StaleProof => {
                "Related local evidence is incomplete or describes a different tuple."
            }
            CompatibilityReason::ExactHintSupported => {
                "An exact-scope hint recommends this tuple; local execution is not verified."
            }
            CompatibilityReason::ExactHintUnsupported => {
                "An exact-scope hint does not recommend this tuple."
            }
            CompatibilityReason::NoEvidence => {
                "No local proof or exact-scope hint applies to this tuple."
            }
        }
    }
    pub fn failure_summary(&self) -> Option<&'static str> {
        self.failure_code.map(ProofFailureCode::summary)
    }
}

/// Exact, pure resolution kept separate from the existing legacy gate resolver.
/// A partial enumeration filter is not accepted as a query or a support hint.
#[derive(Debug, Default, Clone, Copy)]
pub struct CompatibilityResolver;

impl CompatibilityResolver {
    pub fn resolve(
        &self,
        query: &CompatibilityTuple,
        constraints: &CompatibilityConstraints,
        evidence: &[CompatibilityEvidence],
        hints: &[CompatibilityHint],
    ) -> Result<CompatibilityResolution, CompatibilityError> {
        if &constraints.model_ref != query.model_ref() {
            return Err(CompatibilityError::InvalidField("constraint_model_ref"));
        }
        let mut hard = constraints.hard_incompatibilities.clone();
        if !constraints
            .declared_capabilities
            .contains(&query.capability())
        {
            hard.insert(HardIncompatibility::CapabilityNotDeclared);
        }
        if !hard.is_empty() {
            let mut resolution = CompatibilityResolution::new(
                ModelSupportStatus::Unsupported,
                CompatibilityResolutionEvidence::HardIncompatibility,
                CompatibilityReason::HardIncompatibility,
            );
            resolution.hard_incompatibilities = hard;
            return Ok(resolution);
        }

        let related: Vec<_> = evidence
            .iter()
            .filter(|item| {
                item.model_ref() == query.model_ref() && item.capability() == query.capability()
            })
            .collect();
        let mut exact = related
            .iter()
            .filter_map(|item| item.v2())
            .filter(|proof| proof.tuple() == query);
        if let Some(proof) = exact.next() {
            // The store supplies one current record per key. A caller must not
            // ask the resolver to infer a commit order from timestamps/slice order.
            if exact.next().is_some() {
                return Err(CompatibilityError::AmbiguousExactProof);
            }
            let (status, reason) = match proof.status() {
                ModelCapabilityProofStatus::Verified => (
                    ModelSupportStatus::Verified,
                    CompatibilityReason::ExactProofVerified,
                ),
                ModelCapabilityProofStatus::Failed => (
                    ModelSupportStatus::Failed,
                    CompatibilityReason::ExactProofFailed,
                ),
            };
            let mut resolution = CompatibilityResolution::new(
                status,
                CompatibilityResolutionEvidence::ExactProof,
                reason,
            );
            resolution.exact_key = Some(proof.key()?);
            resolution.failure_code = proof.failure_code();
            return Ok(resolution);
        }

        if !related.is_empty() {
            let mut resolution = CompatibilityResolution::new(
                ModelSupportStatus::Stale,
                CompatibilityResolutionEvidence::IncompleteOrDifferentProof,
                CompatibilityReason::StaleProof,
            );
            for item in related {
                resolution.stale_reasons.extend(stale_reasons(query, item));
            }
            return Ok(resolution);
        }

        // Opposing exact hints are advisory policy, not contradictory observed
        // proofs. The negative recommendation wins regardless of input order.
        for status in [
            ModelSupportHintStatus::Unsupported,
            ModelSupportHintStatus::Supported,
        ] {
            if hints
                .iter()
                .any(|hint| hint.status == status && hint.tuple == *query)
            {
                let (status, reason) = match status {
                    ModelSupportHintStatus::Supported => (
                        ModelSupportStatus::Supported,
                        CompatibilityReason::ExactHintSupported,
                    ),
                    ModelSupportHintStatus::Unsupported => (
                        ModelSupportStatus::Unsupported,
                        CompatibilityReason::ExactHintUnsupported,
                    ),
                };
                return Ok(CompatibilityResolution::new(
                    status,
                    CompatibilityResolutionEvidence::ExactHint,
                    reason,
                ));
            }
        }
        Ok(CompatibilityResolution::new(
            ModelSupportStatus::Unknown,
            CompatibilityResolutionEvidence::None,
            CompatibilityReason::NoEvidence,
        ))
    }
}
