//! Complete, versioned local compatibility facts. This module performs no IO.
//!
//! A load observation proves resource loading only. It is never interchangeable
//! with an execution observation, even for otherwise identical components.

mod components;
mod error;
mod evidence;
mod filter;
mod key;
mod proof;
mod shape;
mod tuple;

pub use components::*;
pub use error::CompatibilityError;
pub use evidence::{CompatibilityEvidence, EvidenceGeneration, MissingDimension};
pub use filter::CompatibilityFilter;
pub use key::CompatibilityProofKey;
pub use proof::{CompatibilityProofV2, ProofFailureCode, MAX_PROOF_V2_BYTES, PROOF_SCHEMA_VERSION};
pub use shape::*;
pub use tuple::{CompatibilityTuple, CompatibilityTupleInput, TUPLE_IDENTITY_VERSION};

#[cfg(test)]
mod tests;
