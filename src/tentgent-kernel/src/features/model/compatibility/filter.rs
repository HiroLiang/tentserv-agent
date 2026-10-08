use crate::features::model::domain::{ModelCapability, ModelFormat, ModelRef};

use super::{
    CompatibilityAdapter, CompatibilityBackend, CompatibilityError, CompatibilityPlatform,
    CompatibilityProfile, CompatibilityProofKey, CompatibilityProofV2, CompatibilityTuple,
    DeviceClass, InputShape, ObservationScope, OutputShape, Quantization, RuntimeFamily,
    RuntimePackage, RuntimeVersion,
};

/// In-memory enumeration only. A partial match must never authorize execution.
/// Complete-query resolution compares the entire tuple, including observation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompatibilityFilter {
    pub exact_key: Option<CompatibilityProofKey>,
    pub model_ref: Option<ModelRef>,
    pub capability: Option<ModelCapability>,
    pub primary_format: Option<ModelFormat>,
    pub quantization: Option<Quantization>,
    pub backend: Option<CompatibilityBackend>,
    pub runtime_family: Option<RuntimeFamily>,
    pub runtime_package: Option<RuntimePackage>,
    pub runtime_version: Option<RuntimeVersion>,
    pub profile: Option<CompatibilityProfile>,
    pub platform: Option<CompatibilityPlatform>,
    pub device_class: Option<DeviceClass>,
    pub adapter: Option<CompatibilityAdapter>,
    pub observation_scope: Option<ObservationScope>,
    pub input_shape: Option<InputShape>,
    pub output_shape: Option<OutputShape>,
}

impl CompatibilityFilter {
    pub fn matches(&self, proof: &CompatibilityProofV2) -> Result<bool, CompatibilityError> {
        self.matches_tuple(proof.tuple())
    }

    pub fn matches_tuple(&self, tuple: &CompatibilityTuple) -> Result<bool, CompatibilityError> {
        if let Some(key) = &self.exact_key {
            if tuple.key()? != *key {
                return Ok(false);
            }
        }
        let facts = tuple.components();
        macro_rules! matching {
            ($filter:expr, $fact:expr) => {
                $filter.as_ref().is_none_or(|value| value == $fact)
            };
        }
        let fields_match = matching!(self.model_ref, &facts.model_ref)
            && matching!(self.capability, &facts.capability)
            && matching!(self.primary_format, &facts.primary_format)
            && matching!(self.quantization, &facts.quantization)
            && matching!(self.backend, &facts.backend)
            && matching!(self.runtime_family, &facts.runtime_family)
            && matching!(self.runtime_package, &facts.runtime.package)
            && matching!(self.runtime_version, &facts.runtime.version)
            && matching!(self.profile, &facts.profile)
            && matching!(self.platform, &facts.platform)
            && matching!(self.device_class, &facts.device_class)
            && matching!(self.adapter, &facts.adapter)
            && matching!(self.observation_scope, &facts.observation.scope());
        let shapes_match = match &facts.observation {
            super::Observation::Load => self.input_shape.is_none() && self.output_shape.is_none(),
            super::Observation::Execution { input, output } => {
                matching!(self.input_shape, input) && matching!(self.output_shape, output)
            }
        };
        Ok(fields_match && shapes_match)
    }
}
