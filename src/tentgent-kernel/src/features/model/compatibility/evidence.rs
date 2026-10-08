use std::collections::BTreeSet;

use serde::Serialize;

use crate::features::model::domain::{
    ModelCapability, ModelCapabilityProof, ModelCapabilityProofSource, ModelCapabilityProofStatus,
    ModelRef,
};

use super::{
    CompatibilityBackend, CompatibilityError, CompatibilityIdentifier, CompatibilityProofKey,
    CompatibilityProofV2, RuntimeVersion,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceGeneration {
    V2,
    TupleAwareV1,
    LegacyLatest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MissingDimension {
    ModelRef,
    Capability,
    PrimaryFormat,
    Quantization,
    Backend,
    RuntimeFamily,
    RuntimePackage,
    RuntimeVersion,
    RuntimeProfile,
    RuntimeProfileVersion,
    OperatingSystem,
    Architecture,
    DeviceClass,
    Adapter,
    Observation,
    InputShape,
    OutputShape,
}

impl MissingDimension {
    pub const ALL: [Self; 17] = [
        Self::ModelRef,
        Self::Capability,
        Self::PrimaryFormat,
        Self::Quantization,
        Self::Backend,
        Self::RuntimeFamily,
        Self::RuntimePackage,
        Self::RuntimeVersion,
        Self::RuntimeProfile,
        Self::RuntimeProfileVersion,
        Self::OperatingSystem,
        Self::Architecture,
        Self::DeviceClass,
        Self::Adapter,
        Self::Observation,
        Self::InputShape,
        Self::OutputShape,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ModelRef => "model-ref",
            Self::Capability => "capability",
            Self::PrimaryFormat => "primary-format",
            Self::Quantization => "quantization",
            Self::Backend => "backend",
            Self::RuntimeFamily => "runtime-family",
            Self::RuntimePackage => "runtime-package",
            Self::RuntimeVersion => "runtime-version",
            Self::RuntimeProfile => "runtime-profile",
            Self::RuntimeProfileVersion => "runtime-profile-version",
            Self::OperatingSystem => "operating-system",
            Self::Architecture => "architecture",
            Self::DeviceClass => "device-class",
            Self::Adapter => "adapter",
            Self::Observation => "observation",
            Self::InputShape => "input-shape",
            Self::OutputShape => "output-shape",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "record", rename_all = "kebab-case")]
enum EvidenceRecord {
    V2(Box<CompatibilityProofV2>),
    Legacy(ModelCapabilityProof),
}

/// A read projection, not a proof authority or a legacy-to-v2 migration.
/// Missing legacy fields stay missing; no profile/base/shape/runtime is guessed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompatibilityEvidence {
    generation: EvidenceGeneration,
    record: EvidenceRecord,
    missing_dimensions: BTreeSet<MissingDimension>,
}

impl CompatibilityEvidence {
    pub fn from_v2(proof: CompatibilityProofV2) -> Self {
        Self {
            generation: EvidenceGeneration::V2,
            record: EvidenceRecord::V2(Box::new(proof)),
            missing_dimensions: BTreeSet::new(),
        }
    }

    pub fn from_legacy(
        mut proof: ModelCapabilityProof,
        generation: EvidenceGeneration,
    ) -> Result<Self, CompatibilityError> {
        if generation == EvidenceGeneration::V2 {
            return Err(CompatibilityError::InvalidField("evidence_generation"));
        }
        use MissingDimension as M;
        let mut missing_dimensions = BTreeSet::from([
            M::Quantization,
            M::RuntimePackage,
            M::OperatingSystem,
            M::Architecture,
            M::DeviceClass,
            M::Adapter,
            M::Observation,
            M::InputShape,
            M::OutputShape,
        ]);
        if proof.backend.parse::<CompatibilityBackend>().is_err() {
            missing_dimensions.insert(M::Backend);
        }
        if proof.mlx_runtime_family.is_none() {
            missing_dimensions.insert(M::RuntimeFamily);
        }
        if proof
            .runtime_version
            .as_ref()
            .is_none_or(|version| RuntimeVersion::parse(version).is_err())
        {
            missing_dimensions.insert(M::RuntimeVersion);
        }
        if proof
            .runtime_profile
            .as_ref()
            .is_none_or(|id| CompatibilityIdentifier::parse(id).is_err())
        {
            missing_dimensions.insert(M::RuntimeProfile);
        }
        if proof
            .runtime_profile_version
            .is_none_or(|version| version == 0)
        {
            missing_dimensions.insert(M::RuntimeProfileVersion);
        }
        // Old on-disk failures may predate the safe-error maintenance boundary.
        // Preserve their failed status, but never return arbitrary exception text.
        proof.error = (proof.status == ModelCapabilityProofStatus::Failed)
            .then(|| "Legacy local proof failed; detailed error is not exposed.".to_owned());
        Ok(Self {
            generation,
            record: EvidenceRecord::Legacy(proof),
            missing_dimensions,
        })
    }

    pub const fn generation(&self) -> EvidenceGeneration {
        self.generation
    }
    pub fn v2(&self) -> Option<&CompatibilityProofV2> {
        match &self.record {
            EvidenceRecord::V2(proof) => Some(proof),
            EvidenceRecord::Legacy(_) => None,
        }
    }
    pub fn legacy(&self) -> Option<&ModelCapabilityProof> {
        match &self.record {
            EvidenceRecord::V2(_) => None,
            EvidenceRecord::Legacy(proof) => Some(proof),
        }
    }
    pub fn missing_dimensions(&self) -> &BTreeSet<MissingDimension> {
        &self.missing_dimensions
    }
    pub fn model_ref(&self) -> &ModelRef {
        match &self.record {
            EvidenceRecord::V2(proof) => proof.tuple().model_ref(),
            EvidenceRecord::Legacy(proof) => &proof.model_ref,
        }
    }
    pub fn capability(&self) -> ModelCapability {
        match &self.record {
            EvidenceRecord::V2(proof) => proof.tuple().capability(),
            EvidenceRecord::Legacy(proof) => proof.capability,
        }
    }
    pub fn status(&self) -> ModelCapabilityProofStatus {
        match &self.record {
            EvidenceRecord::V2(proof) => proof.status(),
            EvidenceRecord::Legacy(proof) => proof.status,
        }
    }
    pub fn source(&self) -> ModelCapabilityProofSource {
        match &self.record {
            EvidenceRecord::V2(proof) => proof.source(),
            EvidenceRecord::Legacy(proof) => proof.source,
        }
    }
    pub fn checked_at(&self) -> &str {
        match &self.record {
            EvidenceRecord::V2(proof) => proof.checked_at(),
            EvidenceRecord::Legacy(proof) => &proof.checked_at,
        }
    }
    pub fn key(&self) -> Result<Option<CompatibilityProofKey>, CompatibilityError> {
        self.v2().map(CompatibilityProofV2::key).transpose()
    }
    pub fn failure_summary(&self) -> Option<&str> {
        match &self.record {
            EvidenceRecord::V2(proof) => proof.failure_summary(),
            EvidenceRecord::Legacy(proof) => proof.error.as_deref(),
        }
    }
}
