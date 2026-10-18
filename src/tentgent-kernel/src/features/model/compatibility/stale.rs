use std::collections::BTreeSet;

use crate::features::model::domain::{MlxRuntimeFamily, ModelCapabilityProof};

use super::{
    CompatibilityBackend, CompatibilityEvidence, CompatibilityIdentifier, CompatibilityProfile,
    CompatibilityStaleReason as Reason, CompatibilityTuple, MissingDimension as Dimension,
    Observation, RuntimeFamily, RuntimeVersion,
};

pub(super) fn stale_reasons(
    query: &CompatibilityTuple,
    evidence: &CompatibilityEvidence,
) -> BTreeSet<Reason> {
    let mut reasons: BTreeSet<_> = evidence
        .missing_dimensions()
        .iter()
        .copied()
        .map(Reason::MissingDimension)
        .collect();
    if let Some(proof) = evidence.v2() {
        reasons.extend(changed_tuple_dimensions(query, proof.tuple()));
    }
    if let Some(proof) = evidence.legacy() {
        reasons.extend(changed_legacy_dimensions(query, proof));
    }
    reasons
}

fn changed_tuple_dimensions(
    query: &CompatibilityTuple,
    proof: &CompatibilityTuple,
) -> BTreeSet<Reason> {
    let query = query.components();
    let proof = proof.components();
    let mut reasons = BTreeSet::new();
    macro_rules! changed {
        ($dimension:ident, $left:expr, $right:expr) => {
            if $left != $right {
                reasons.insert(Reason::ChangedDimension(Dimension::$dimension));
            }
        };
    }
    changed!(ModelRef, query.model_ref, proof.model_ref);
    changed!(Capability, query.capability, proof.capability);
    changed!(PrimaryFormat, query.primary_format, proof.primary_format);
    changed!(Quantization, query.quantization, proof.quantization);
    changed!(Backend, query.backend, proof.backend);
    changed!(RuntimeFamily, query.runtime_family, proof.runtime_family);
    changed!(RuntimePackage, query.runtime.package, proof.runtime.package);
    changed!(RuntimeVersion, query.runtime.version, proof.runtime.version);
    match (&query.profile, &proof.profile) {
        (
            CompatibilityProfile::Selected {
                id: q_id,
                version: q_version,
            },
            CompatibilityProfile::Selected {
                id: p_id,
                version: p_version,
            },
        ) => {
            changed!(RuntimeProfile, q_id, p_id);
            changed!(RuntimeProfileVersion, q_version, p_version);
        }
        (left, right) => {
            changed!(RuntimeProfile, left, right);
        }
    }
    changed!(OperatingSystem, query.platform.os, proof.platform.os);
    changed!(
        Architecture,
        query.platform.architecture,
        proof.platform.architecture
    );
    changed!(DeviceClass, query.device_class, proof.device_class);
    changed!(Adapter, query.adapter, proof.adapter);
    changed!(
        Observation,
        query.observation.scope(),
        proof.observation.scope()
    );
    if let (
        Observation::Execution {
            input: query_input,
            output: query_output,
        },
        Observation::Execution {
            input: proof_input,
            output: proof_output,
        },
    ) = (&query.observation, &proof.observation)
    {
        changed!(InputShape, query_input, proof_input);
        changed!(OutputShape, query_output, proof_output);
    }
    reasons
}

fn changed_legacy_dimensions(
    query: &CompatibilityTuple,
    proof: &ModelCapabilityProof,
) -> BTreeSet<Reason> {
    let query = query.components();
    let mut reasons = BTreeSet::new();
    if proof.primary_format != query.primary_format {
        reasons.insert(Reason::ChangedDimension(Dimension::PrimaryFormat));
    }
    if let Ok(backend) = proof.backend.parse::<CompatibilityBackend>() {
        if backend != query.backend {
            reasons.insert(Reason::ChangedDimension(Dimension::Backend));
        }
    }
    if let Some(family) = proof.mlx_runtime_family {
        let family = match family {
            MlxRuntimeFamily::Lm => RuntimeFamily::MlxLm,
            MlxRuntimeFamily::Vlm => RuntimeFamily::MlxVlm,
            MlxRuntimeFamily::Audio => RuntimeFamily::MlxAudio,
            MlxRuntimeFamily::Diffusion => RuntimeFamily::MlxDiffusion,
        };
        if family != query.runtime_family {
            reasons.insert(Reason::ChangedDimension(Dimension::RuntimeFamily));
        }
    }
    if let Some(version) = proof
        .runtime_version
        .as_deref()
        .and_then(|value| RuntimeVersion::parse(value).ok())
    {
        if version != query.runtime.version {
            reasons.insert(Reason::ChangedDimension(Dimension::RuntimeVersion));
        }
    }
    if let Some(id) = proof
        .runtime_profile
        .as_deref()
        .and_then(|value| CompatibilityIdentifier::parse(value).ok())
    {
        if !matches!(&query.profile, CompatibilityProfile::Selected { id: query_id, .. } if id == *query_id)
        {
            reasons.insert(Reason::ChangedDimension(Dimension::RuntimeProfile));
        }
    }
    if let Some(version) = proof.runtime_profile_version.filter(|version| *version > 0) {
        if !matches!(&query.profile, CompatibilityProfile::Selected { version: query_version, .. } if version == *query_version)
        {
            reasons.insert(Reason::ChangedDimension(Dimension::RuntimeProfileVersion));
        }
    }
    reasons
}
