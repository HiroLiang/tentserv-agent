use serde::{Deserialize, Serialize};

use crate::features::model::domain::{ModelCapability, ModelFormat, ModelRef};

use super::{
    CompatibilityAdapter, CompatibilityBackend, CompatibilityError, CompatibilityPlatform,
    CompatibilityProfile, CompatibilityProofKey, DeviceClass, Observation, Quantization,
    RuntimeFamily, RuntimeIdentity,
};

pub const TUPLE_IDENTITY_VERSION: u16 = 1;

/// All fields must be facts about the selected runtime, never guessed defaults.
/// Explicit no-profile/base/unquantized states are not substitutes for unknown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompatibilityTupleInput {
    pub model_ref: ModelRef,
    pub capability: ModelCapability,
    pub primary_format: ModelFormat,
    pub quantization: Quantization,
    pub backend: CompatibilityBackend,
    pub runtime_family: RuntimeFamily,
    pub runtime: RuntimeIdentity,
    pub profile: CompatibilityProfile,
    pub platform: CompatibilityPlatform,
    pub device_class: DeviceClass,
    pub adapter: CompatibilityAdapter,
    pub observation: Observation,
}

/// Immutable validated tuple. Canonical JSON field order is an identity contract.
/// Altering normalization or ordering requires a new identity version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "TupleWire")]
pub struct CompatibilityTuple {
    identity_version: u16,
    components: CompatibilityTupleInput,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TupleWire {
    identity_version: u16,
    components: CompatibilityTupleInput,
}

impl CompatibilityTuple {
    pub fn new(components: CompatibilityTupleInput) -> Result<Self, CompatibilityError> {
        components.profile.validate()?;
        components.quantization.validate()?;
        components.observation.validate(components.capability)?;
        if components.runtime_family.backend() != components.backend
            || components.runtime_family.package() != components.runtime.package
        {
            return Err(CompatibilityError::InvalidField("runtime_identity"));
        }
        Ok(Self {
            identity_version: TUPLE_IDENTITY_VERSION,
            components,
        })
    }

    pub const fn identity_version(&self) -> u16 {
        self.identity_version
    }
    pub fn components(&self) -> &CompatibilityTupleInput {
        &self.components
    }
    pub fn model_ref(&self) -> &ModelRef {
        &self.components.model_ref
    }
    pub const fn capability(&self) -> ModelCapability {
        self.components.capability
    }
    pub fn observation(&self) -> &Observation {
        &self.components.observation
    }
    pub fn canonical_json(&self) -> Result<String, CompatibilityError> {
        serde_json::to_string(self).map_err(|_| CompatibilityError::CanonicalSerialization)
    }
    pub fn key(&self) -> Result<CompatibilityProofKey, CompatibilityError> {
        CompatibilityProofKey::from_tuple(self)
    }
}

impl TryFrom<TupleWire> for CompatibilityTuple {
    type Error = CompatibilityError;
    fn try_from(wire: TupleWire) -> Result<Self, Self::Error> {
        if wire.identity_version != TUPLE_IDENTITY_VERSION {
            return Err(CompatibilityError::UnsupportedIdentity(
                wire.identity_version,
            ));
        }
        Self::new(wire.components)
    }
}
