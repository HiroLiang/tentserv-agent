use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::features::model::domain::{ModelCapabilityProofSource, ModelCapabilityProofStatus};

use super::{CompatibilityError, CompatibilityProofKey, CompatibilityTuple, ObservationScope};

pub const PROOF_SCHEMA_VERSION: u16 = 2;
/// Stores must bound input before parsing; this domain does not perform file IO.
pub const MAX_PROOF_V2_BYTES: usize = 16 * 1024;

/// Deliberately fixed and payload-free. Never persist raw runtime exceptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProofFailureCode {
    ModelLoadFailed,
    RuntimeExecutionFailed,
    ResourceExhausted,
    UnsupportedExecution,
    InvalidRuntimeResponse,
}

impl ProofFailureCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ModelLoadFailed => "model-load-failed",
            Self::RuntimeExecutionFailed => "runtime-execution-failed",
            Self::ResourceExhausted => "resource-exhausted",
            Self::UnsupportedExecution => "unsupported-execution",
            Self::InvalidRuntimeResponse => "invalid-runtime-response",
        }
    }

    pub const fn summary(self) -> &'static str {
        match self {
            Self::ModelLoadFailed => "The selected runtime could not load the model.",
            Self::RuntimeExecutionFailed => "The selected runtime could not complete execution.",
            Self::ResourceExhausted => "The selected runtime exhausted available resources.",
            Self::UnsupportedExecution => "The selected runtime rejected this execution shape.",
            Self::InvalidRuntimeResponse => "The selected runtime returned an invalid response.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum RecordKind {
    LocalProof,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ProofWire")]
pub struct CompatibilityProofV2 {
    schema_version: u16,
    record_kind: RecordKind,
    tuple: CompatibilityTuple,
    status: ModelCapabilityProofStatus,
    source: ModelCapabilityProofSource,
    checked_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure_code: Option<ProofFailureCode>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProofWire {
    schema_version: u16,
    record_kind: RecordKind,
    tuple: CompatibilityTuple,
    status: ModelCapabilityProofStatus,
    source: ModelCapabilityProofSource,
    checked_at: String,
    #[serde(default)]
    failure_code: Option<ProofFailureCode>,
}

impl CompatibilityProofV2 {
    pub fn new(
        tuple: CompatibilityTuple,
        status: ModelCapabilityProofStatus,
        source: ModelCapabilityProofSource,
        checked_at: impl Into<String>,
        failure_code: Option<ProofFailureCode>,
    ) -> Result<Self, CompatibilityError> {
        let checked_at = checked_at.into();
        if checked_at.len() > 64 || OffsetDateTime::parse(&checked_at, &Rfc3339).is_err() {
            return Err(CompatibilityError::InvalidField("checked_at"));
        }
        if (status == ModelCapabilityProofStatus::Failed) != failure_code.is_some() {
            return Err(CompatibilityError::InvalidField("failure_code"));
        }
        let valid_source = matches!(
            (source, tuple.observation().scope()),
            (
                ModelCapabilityProofSource::ServerStart,
                ObservationScope::Load
            ) | (
                ModelCapabilityProofSource::EndpointSmoke
                    | ModelCapabilityProofSource::RuntimeExecution,
                ObservationScope::Execution
            )
        );
        if !valid_source {
            return Err(CompatibilityError::InvalidObservationSource);
        }
        Ok(Self {
            schema_version: PROOF_SCHEMA_VERSION,
            record_kind: RecordKind::LocalProof,
            tuple,
            status,
            source,
            checked_at,
            failure_code,
        })
    }

    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }
    pub const fn record_kind(&self) -> &'static str {
        "local-proof"
    }
    pub fn tuple(&self) -> &CompatibilityTuple {
        &self.tuple
    }
    pub fn key(&self) -> Result<CompatibilityProofKey, CompatibilityError> {
        self.tuple.key()
    }
    pub const fn status(&self) -> ModelCapabilityProofStatus {
        self.status
    }
    pub const fn source(&self) -> ModelCapabilityProofSource {
        self.source
    }
    pub fn checked_at(&self) -> &str {
        &self.checked_at
    }
    pub const fn failure_code(&self) -> Option<ProofFailureCode> {
        self.failure_code
    }
    pub fn failure_summary(&self) -> Option<&'static str> {
        self.failure_code.map(ProofFailureCode::summary)
    }

    pub fn validate_location(&self, key: &CompatibilityProofKey) -> Result<(), CompatibilityError> {
        if self.key()? != *key {
            return Err(CompatibilityError::LocationMismatch);
        }
        Ok(())
    }
}

impl TryFrom<ProofWire> for CompatibilityProofV2 {
    type Error = CompatibilityError;
    fn try_from(wire: ProofWire) -> Result<Self, Self::Error> {
        if wire.schema_version != PROOF_SCHEMA_VERSION {
            return Err(CompatibilityError::UnsupportedSchema(wire.schema_version));
        }
        // Reading the typed field also rejects unknown record kinds at serde boundary.
        let RecordKind::LocalProof = wire.record_kind;
        Self::new(
            wire.tuple,
            wire.status,
            wire.source,
            wire.checked_at,
            wire.failure_code,
        )
    }
}
