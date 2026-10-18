use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::features::model::domain::{ModelCapability, ModelRef};

use super::{CompatibilityError, CompatibilityTuple};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "KeyWire")]
pub struct CompatibilityProofKey {
    model_ref: ModelRef,
    capability: ModelCapability,
    tuple_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyWire {
    model_ref: ModelRef,
    capability: ModelCapability,
    tuple_sha256: String,
}

impl CompatibilityProofKey {
    /// A persisted key must already be canonical; filenames are never aliases.
    pub fn parse(
        model_ref: ModelRef,
        capability: ModelCapability,
        tuple_sha256: impl Into<String>,
    ) -> Result<Self, CompatibilityError> {
        let tuple_sha256 = tuple_sha256.into();
        if tuple_sha256.len() != 64
            || !tuple_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(CompatibilityError::InvalidField("tuple_sha256"));
        }
        Ok(Self {
            model_ref,
            capability,
            tuple_sha256,
        })
    }

    pub fn from_tuple(tuple: &CompatibilityTuple) -> Result<Self, CompatibilityError> {
        let digest = hex::encode(Sha256::digest(tuple.canonical_json()?.as_bytes()));
        Self::parse(tuple.model_ref().clone(), tuple.capability(), digest)
    }
    pub fn model_ref(&self) -> &ModelRef {
        &self.model_ref
    }
    pub const fn capability(&self) -> ModelCapability {
        self.capability
    }
    pub fn tuple_sha256(&self) -> &str {
        &self.tuple_sha256
    }
    pub fn filename(&self) -> String {
        format!("{}.toml", self.tuple_sha256)
    }
}

impl TryFrom<KeyWire> for CompatibilityProofKey {
    type Error = CompatibilityError;
    fn try_from(wire: KeyWire) -> Result<Self, Self::Error> {
        Self::parse(wire.model_ref, wire.capability, wire.tuple_sha256)
    }
}
