//! Adapter use case ports.

use std::path::PathBuf;

use crate::features::auth::usecases::AuthSecretResolutionRequest;
use crate::features::model::domain::ModelRefSelector;
use crate::features::runtime::domain::{PythonRuntimeLayout, PythonRuntimeResolutionInput};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

use super::super::domain::{
    AdapterBindOutcome, AdapterCompatibilityTarget, AdapterImportOutcome, AdapterInspection,
    AdapterRefSelector, AdapterRemovalOutcome, AdapterStoreLayout, AdapterSummary,
    HfAdapterPullProgress,
};

/// Request for listing stored adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterListRequest {
    pub layout: RuntimeLayoutInput,
}

/// Result of listing stored adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterListResult {
    pub layout: RuntimeLayout,
    pub store: AdapterStoreLayout,
    pub adapters: Vec<AdapterSummary>,
}

/// Request for inspecting one stored adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterInspectRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: AdapterRefSelector,
}

/// Result of inspecting one stored adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterInspectResult {
    pub layout: RuntimeLayout,
    pub store: AdapterStoreLayout,
    pub adapter: AdapterInspection,
}

/// Request for importing a local adapter directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterLocalImportRequest {
    pub layout: RuntimeLayoutInput,
    pub source_path: PathBuf,
    pub base_model_selector: Option<ModelRefSelector>,
}

/// Result of importing a local adapter directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterLocalImportResult {
    pub layout: RuntimeLayout,
    pub store: AdapterStoreLayout,
    pub outcome: AdapterImportOutcome,
}

/// Request for pulling a Hugging Face adapter snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterHfPullRequest {
    pub layout: RuntimeLayoutInput,
    pub runtime: PythonRuntimeResolutionInput,
    pub repo_id: String,
    pub revision: Option<String>,
    pub base_model_selector: Option<ModelRefSelector>,
    pub auth: AuthSecretResolutionRequest,
}

/// Result of pulling a Hugging Face adapter snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterHfPullResult {
    pub layout: RuntimeLayout,
    pub store: AdapterStoreLayout,
    pub runtime: PythonRuntimeLayout,
    pub outcome: AdapterImportOutcome,
}

/// Request for importing a successful training-run adapter output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterTrainRunImportRequest {
    pub layout: RuntimeLayoutInput,
    pub output_path: PathBuf,
    pub base_model_selector: ModelRefSelector,
    pub training_dataset_ref: String,
    pub training_run_ref: String,
    pub training_config_ref: String,
}

/// Result of importing a successful training-run adapter output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterTrainRunImportResult {
    pub layout: RuntimeLayout,
    pub store: AdapterStoreLayout,
    pub outcome: AdapterImportOutcome,
}

/// Request for binding an existing adapter to one managed local base model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterBindRequest {
    pub layout: RuntimeLayoutInput,
    pub adapter_selector: AdapterRefSelector,
    pub base_model_selector: ModelRefSelector,
}

/// Result of binding an existing adapter to one managed local base model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterBindResult {
    pub layout: RuntimeLayout,
    pub store: AdapterStoreLayout,
    pub outcome: AdapterBindOutcome,
}

/// Request for validating an adapter against an already selected server target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterCompatibilityCheckRequest {
    pub layout: RuntimeLayoutInput,
    pub adapter_selector: AdapterRefSelector,
    pub target: AdapterCompatibilityTarget,
}

/// Result of validating an adapter against an already selected server target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterCompatibilityCheckResult {
    pub layout: RuntimeLayout,
    pub store: AdapterStoreLayout,
    pub adapter: AdapterInspection,
}

/// Request for removing one stored adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterRemoveRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: AdapterRefSelector,
}

/// Result of removing one stored adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterRemoveResult {
    pub layout: RuntimeLayout,
    pub store: AdapterStoreLayout,
    pub outcome: AdapterRemovalOutcome,
}

/// Use-case boundary for read-only adapter catalog operations.
pub trait AdapterCatalogReadUseCase {
    /// Lists stored adapters without mutating the adapter store.
    fn list_adapters(&self, request: AdapterListRequest) -> KernelResult<AdapterListResult>;

    /// Inspects one adapter by full adapter_ref or unique prefix.
    fn inspect_adapter(&self, request: AdapterInspectRequest)
        -> KernelResult<AdapterInspectResult>;
}

/// Use-case boundary for importing local adapter content.
pub trait AdapterLocalImportUseCase {
    /// Stages, manifests, deduplicates, optionally binds, and records one local adapter import.
    fn import_local_adapter(
        &self,
        request: AdapterLocalImportRequest,
    ) -> KernelResult<AdapterLocalImportResult>;
}

/// Use-case boundary for pulling Hugging Face adapter content.
pub trait AdapterHfPullUseCase {
    /// Resolves auth/runtime, fetches a snapshot, then imports it into the adapter store.
    fn pull_hf_adapter(
        &self,
        request: AdapterHfPullRequest,
        progress: &mut dyn FnMut(HfAdapterPullProgress),
    ) -> KernelResult<AdapterHfPullResult>;
}

/// Use-case boundary for importing training-run adapter output.
pub trait AdapterTrainRunImportUseCase {
    /// Stages a successful train-run output and records training provenance indexes.
    fn import_train_run_adapter(
        &self,
        request: AdapterTrainRunImportRequest,
    ) -> KernelResult<AdapterTrainRunImportResult>;
}

/// Use-case boundary for binding adapters to local managed base models.
pub trait AdapterBindUseCase {
    /// Resolves adapter and model metadata, validates source hints, and writes base indexes.
    fn bind_adapter(&self, request: AdapterBindRequest) -> KernelResult<AdapterBindResult>;
}

/// Use-case boundary for server-time adapter compatibility checks.
pub trait AdapterCompatibilityCheckUseCase {
    /// Resolves an adapter and validates it for a selected base model/backend target.
    fn check_adapter_compatibility(
        &self,
        request: AdapterCompatibilityCheckRequest,
    ) -> KernelResult<AdapterCompatibilityCheckResult>;
}

/// Use-case boundary for removing stored adapters.
pub trait AdapterRemoveUseCase {
    /// Resolves an adapter ref, checks server references, and removes content plus indexes.
    fn remove_adapter(&self, request: AdapterRemoveRequest) -> KernelResult<AdapterRemoveResult>;
}
