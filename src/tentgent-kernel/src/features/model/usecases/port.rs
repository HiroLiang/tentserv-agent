//! Model use case ports.

use std::path::PathBuf;

use crate::features::auth::usecases::AuthSecretResolutionRequest;
use crate::features::model::domain::{
    HfModelPullProgress, ModelCapability, ModelCapabilityProof, ModelCapabilityProofSource,
    ModelCapabilityProofStatus, ModelImportOutcome, ModelInspection, ModelMetadata,
    ModelRefSelector, ModelRemovalOutcome, ModelStoreLayout, ModelSummary,
};
use crate::features::runtime::domain::{PythonRuntimeLayout, PythonRuntimeResolutionInput};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

/// Request for listing stored models.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelListRequest {
    pub layout: RuntimeLayoutInput,
}

/// Result of listing stored models.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelListResult {
    pub layout: RuntimeLayout,
    pub store: ModelStoreLayout,
    pub models: Vec<ModelSummary>,
}

/// Request for inspecting one stored model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInspectRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: ModelRefSelector,
}

/// Result of inspecting one stored model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInspectResult {
    pub layout: RuntimeLayout,
    pub store: ModelStoreLayout,
    pub model: ModelInspection,
}

/// Request for importing a local model file or directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelLocalImportRequest {
    pub layout: RuntimeLayoutInput,
    pub source_path: PathBuf,
    pub capability: Option<ModelCapability>,
}

/// Result of importing a local model source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelLocalImportResult {
    pub layout: RuntimeLayout,
    pub store: ModelStoreLayout,
    pub outcome: ModelImportOutcome,
}

/// Request for pulling a Hugging Face model snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelHfPullRequest {
    pub layout: RuntimeLayoutInput,
    pub runtime: PythonRuntimeResolutionInput,
    pub repo_id: String,
    pub revision: Option<String>,
    pub capability: Option<ModelCapability>,
    pub auth: AuthSecretResolutionRequest,
}

/// Result of pulling a Hugging Face model snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelHfPullResult {
    pub layout: RuntimeLayout,
    pub store: ModelStoreLayout,
    pub runtime: PythonRuntimeLayout,
    pub outcome: ModelImportOutcome,
}

/// Request for removing one stored model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRemoveRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: ModelRefSelector,
}

/// Result of removing one stored model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRemoveResult {
    pub layout: RuntimeLayout,
    pub store: ModelStoreLayout,
    pub outcome: ModelRemovalOutcome,
}

/// Request for correcting stored model capability metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilityUpdateRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: ModelRefSelector,
    pub mutation: ModelCapabilityMutation,
}

/// Capability metadata mutation to apply to one stored model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelCapabilityMutation {
    Set(Vec<ModelCapability>),
    AddRemove {
        add: Vec<ModelCapability>,
        remove: Vec<ModelCapability>,
    },
}

/// Result of correcting stored model capability metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilityUpdateResult {
    pub layout: RuntimeLayout,
    pub store: ModelStoreLayout,
    pub model: ModelInspection,
    pub previous_capabilities: Vec<ModelCapability>,
    pub added_capabilities: Vec<ModelCapability>,
    pub removed_capabilities: Vec<ModelCapability>,
}

/// Request for listing the latest capability proofs for one stored model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilityProofListRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: ModelRefSelector,
}

/// Result of listing the latest capability proofs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilityProofListResult {
    pub layout: RuntimeLayout,
    pub store: ModelStoreLayout,
    pub model: ModelInspection,
    pub proofs: Vec<ModelCapabilityProof>,
}

/// Request for manually probing one model capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilityVerifyRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: ModelRefSelector,
    pub capability: ModelCapability,
}

/// Request for recording a capability proof from an external runtime event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilityProofRecordRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: ModelRefSelector,
    pub capability: ModelCapability,
    pub status: ModelCapabilityProofStatus,
    pub source: ModelCapabilityProofSource,
    pub server_ref: Option<String>,
    pub runtime_profile: Option<String>,
    pub runtime_profile_version: Option<u32>,
    pub error: Option<String>,
}

/// Result of writing one latest capability proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilityProofRecordResult {
    pub layout: RuntimeLayout,
    pub store: ModelStoreLayout,
    pub model: ModelInspection,
    pub proof: ModelCapabilityProof,
}

/// Request for recording proof evidence from a resolved local runtime execution attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRuntimeExecutionEvidenceRecordRequest {
    pub layout: RuntimeLayout,
    pub metadata: ModelMetadata,
    pub capability: ModelCapability,
    pub status: ModelCapabilityProofStatus,
    pub server_ref: Option<String>,
    pub runtime_profile: Option<String>,
    pub runtime_profile_version: Option<u32>,
    pub error: Option<String>,
}

/// Result of recording runtime execution proof evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRuntimeExecutionEvidenceRecordResult {
    pub proof: ModelCapabilityProof,
}

/// Request for clearing stored proof records for one model capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilityProofClearRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: ModelRefSelector,
    pub capability: ModelCapability,
}

/// Result of clearing stored proof records for one model capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilityProofClearResult {
    pub layout: RuntimeLayout,
    pub store: ModelStoreLayout,
    pub model: ModelInspection,
    pub capability: ModelCapability,
    pub removed_proof_count: usize,
}

/// Use-case boundary for read-only model catalog operations.
pub trait ModelCatalogReadUseCase {
    /// Lists stored models without mutating the model store.
    fn list_models(&self, request: ModelListRequest) -> KernelResult<ModelListResult>;

    /// Inspects one model by full model_ref or unique prefix.
    fn inspect_model(&self, request: ModelInspectRequest) -> KernelResult<ModelInspectResult>;
}

/// Use-case boundary for importing local model content.
pub trait ModelLocalImportUseCase {
    /// Stages, manifests, deduplicates, and records one local model import.
    fn import_local_model(
        &self,
        request: ModelLocalImportRequest,
    ) -> KernelResult<ModelLocalImportResult>;
}

/// Use-case boundary for pulling Hugging Face model content.
pub trait ModelHfPullUseCase {
    /// Resolves auth/runtime, fetches a snapshot, then imports it into the model store.
    fn pull_hf_model(
        &self,
        request: ModelHfPullRequest,
        progress: &mut dyn FnMut(HfModelPullProgress),
    ) -> KernelResult<ModelHfPullResult>;
}

/// Use-case boundary for removing stored models.
pub trait ModelRemoveUseCase {
    /// Resolves a model ref, checks server references, and removes content plus source indexes.
    fn remove_model(&self, request: ModelRemoveRequest) -> KernelResult<ModelRemoveResult>;
}

/// Use-case boundary for correcting model capability metadata.
pub trait ModelCapabilityUpdateUseCase {
    /// Resolves a model ref and rewrites only capability metadata.
    fn update_model_capability(
        &self,
        request: ModelCapabilityUpdateRequest,
    ) -> KernelResult<ModelCapabilityUpdateResult>;
}

/// Use-case boundary for model capability proof reads and writes.
pub trait ModelCapabilityProofUseCase {
    /// Lists latest proofs stored for one model.
    fn list_model_capability_proofs(
        &self,
        request: ModelCapabilityProofListRequest,
    ) -> KernelResult<ModelCapabilityProofListResult>;

    /// Runs the local metadata-level probe and writes a proof record.
    fn verify_model_capability(
        &self,
        request: ModelCapabilityVerifyRequest,
    ) -> KernelResult<ModelCapabilityProofRecordResult>;

    /// Writes a proof record for a runtime event such as server start.
    fn record_model_capability_proof(
        &self,
        request: ModelCapabilityProofRecordRequest,
    ) -> KernelResult<ModelCapabilityProofRecordResult>;

    /// Clears all proof records for one model capability.
    fn clear_model_capability_proofs(
        &self,
        request: ModelCapabilityProofClearRequest,
    ) -> KernelResult<ModelCapabilityProofClearResult>;
}

/// Use-case boundary for recording local runtime execution evidence.
pub trait ModelRuntimeExecutionEvidenceRecorder {
    /// Records a verified or failed local runtime execution proof.
    fn record_runtime_execution_evidence(
        &self,
        request: ModelRuntimeExecutionEvidenceRecordRequest,
    ) -> KernelResult<ModelRuntimeExecutionEvidenceRecordResult>;
}
