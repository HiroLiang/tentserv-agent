//! Model use case ports.

use std::path::PathBuf;

use crate::features::auth::usecases::AuthSecretResolutionRequest;
use crate::features::model::domain::{
    HfModelPullProgress, ModelImportOutcome, ModelInspection, ModelRefSelector,
    ModelRemovalOutcome, ModelStoreLayout, ModelSummary,
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
