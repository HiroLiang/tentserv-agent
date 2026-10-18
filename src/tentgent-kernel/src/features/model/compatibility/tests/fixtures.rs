use std::collections::BTreeSet;

use crate::features::model::domain::{
    MlxRuntimeFamily, ModelCapability, ModelCapabilityProof, ModelCapabilityProofSource as Source,
    ModelCapabilityProofStatus as Status, ModelFormat, ModelRef,
};

use super::super::*;

pub(super) fn identifier(value: &str) -> CompatibilityIdentifier {
    CompatibilityIdentifier::parse(value).unwrap()
}
pub(super) fn version(value: &str) -> RuntimeVersion {
    RuntimeVersion::parse(value).unwrap()
}
pub(super) fn facts() -> CompatibilityTupleInput {
    CompatibilityTupleInput {
        model_ref: ModelRef::parse("a".repeat(64)).unwrap(),
        capability: ModelCapability::Chat,
        primary_format: ModelFormat::Mlx,
        quantization: Quantization::Unquantized,
        backend: CompatibilityBackend::Mlx,
        runtime_family: RuntimeFamily::MlxLm,
        runtime: RuntimeIdentity {
            package: RuntimePackage::MlxLm,
            version: version("0.30.1"),
        },
        profile: CompatibilityProfile::NoProfile,
        platform: CompatibilityPlatform {
            os: OperatingSystem::Macos,
            architecture: Architecture::Aarch64,
        },
        device_class: DeviceClass::Metal,
        adapter: CompatibilityAdapter::Base,
        observation: Observation::Load,
    }
}
pub(super) fn tuple() -> CompatibilityTuple {
    CompatibilityTuple::new(facts()).unwrap()
}
pub(super) fn execution() -> Observation {
    Observation::Execution {
        input: InputShape {
            family: ModelCapability::Chat,
            modalities: BTreeSet::from([Modality::Text]),
            provider: ProviderShape::Native,
            attributes: ShapeAttributes::default(),
        },
        output: OutputShape {
            family: ModelCapability::Chat,
            modalities: BTreeSet::from([Modality::Text]),
            streaming: false,
            format: OutputFormat::Text,
        },
    }
}
pub(super) fn proof() -> CompatibilityProofV2 {
    CompatibilityProofV2::new(
        tuple(),
        Status::Verified,
        Source::ServerStart,
        "2026-10-08T00:00:00Z",
        None,
    )
    .unwrap()
}
pub(super) fn changed(change: impl FnOnce(&mut CompatibilityTupleInput)) -> CompatibilityTuple {
    let mut input = facts();
    change(&mut input);
    CompatibilityTuple::new(input).unwrap()
}
pub(super) fn legacy_proof() -> ModelCapabilityProof {
    ModelCapabilityProof {
        model_ref: facts().model_ref,
        capability: ModelCapability::Chat,
        status: Status::Verified,
        source: Source::ManualProbe,
        primary_format: ModelFormat::Mlx,
        mlx_runtime_family: Some(MlxRuntimeFamily::Lm),
        backend: "mlx".to_owned(),
        runtime_version: None,
        runtime_profile: None,
        runtime_profile_version: None,
        server_ref: None,
        checked_at: "2026-10-08T00:00:00Z".to_owned(),
        error: None,
    }
}
