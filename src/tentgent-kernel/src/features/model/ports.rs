//! Model feature package ports.

use std::path::{Path, PathBuf};

use crate::features::auth::domain::AuthSecretMaterial;
use crate::features::runtime::domain::PythonRuntimeLayout;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;

use super::domain::{
    HfModelMetadata, HfModelPullProgress, HfModelSourceIndex, LocalModelSourceIndex,
    ModelCapability, ModelCapabilityProof, ModelFormat, ModelImportMethod, ModelInspection,
    ModelManifest, ModelMetadata, ModelRef, ModelRefSelector, ModelStoreLayout, ModelSummary,
    ModelVariantMetadata,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedModelSource {
    pub staging_root: PathBuf,
    pub source_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HfModelSnapshotRequest {
    pub runtime: PythonRuntimeLayout,
    pub repo_id: String,
    pub revision: Option<String>,
    pub destination_dir: PathBuf,
    pub secret: Option<AuthSecretMaterial>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HfModelSnapshot {
    pub repo_id: String,
    pub resolved_revision: String,
    pub local_dir: PathBuf,
    pub metadata: Option<HfModelMetadata>,
}

/// Ensures the model-store root directories exist for mutating model operations.
pub trait ModelStoreLayoutInitializer {
    /// Creates the store, source-index, and staging directories for the layout.
    fn ensure_model_store_layout(&self, layout: &ModelStoreLayout) -> KernelResult<()>;
}

/// Stages local or remote model content before it is assigned a canonical model_ref.
pub trait ModelSourceStager {
    /// Creates an isolated staging root and source directory for one import operation.
    fn create_staging_source(
        &self,
        layout: &ModelStoreLayout,
        method: ModelImportMethod,
    ) -> KernelResult<StagedModelSource>;

    /// Copies a local file or directory into an existing staging source directory.
    fn copy_local_source(&self, input_path: &Path, staged: &StagedModelSource) -> KernelResult<()>;

    /// Removes a staging root after successful import, deduplication, or failure cleanup.
    fn discard_staging(&self, staged: &StagedModelSource) -> KernelResult<()>;
}

/// Fetches a Hugging Face model snapshot into a caller-provided staging directory.
pub trait HfModelSnapshotFetcher {
    /// Runs the snapshot helper and reports progress without deciding canonical model identity.
    fn fetch_hf_snapshot(
        &self,
        request: HfModelSnapshotRequest,
        progress: &mut dyn FnMut(HfModelPullProgress),
    ) -> KernelResult<HfModelSnapshot>;
}

/// Builds a canonical file manifest from staged model content.
pub trait ModelManifestBuilder {
    /// Walks staged source content and records normalized relative paths, sizes, and hashes.
    fn build_manifest(&self, source_root: &Path) -> KernelResult<ModelManifest>;
}

/// Generates canonical model identity from manifest data.
pub trait ModelIdentityGenerator {
    /// Hashes the canonical manifest representation into a full model_ref.
    fn model_ref_for_manifest(&self, manifest: &ModelManifest) -> KernelResult<ModelRef>;
}

/// Reads and writes model catalog metadata.
pub trait ModelCatalogStore {
    /// Lists stored model metadata summaries sorted for stable display.
    fn list_models(&self, layout: &ModelStoreLayout) -> KernelResult<Vec<ModelSummary>>;

    /// Resolves a full hash or unique hash prefix and returns full inspection paths.
    fn inspect_model(
        &self,
        layout: &ModelStoreLayout,
        selector: &ModelRefSelector,
    ) -> KernelResult<ModelInspection>;

    /// Loads metadata for an already resolved model_ref.
    fn load_model_metadata(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
    ) -> KernelResult<ModelMetadata>;

    /// Writes model.toml for an imported or deduplicated model.
    fn save_model_metadata(
        &self,
        layout: &ModelStoreLayout,
        metadata: &ModelMetadata,
    ) -> KernelResult<()>;

    /// Writes manifest.json for an imported model.
    fn save_model_manifest(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
        manifest: &ModelManifest,
    ) -> KernelResult<()>;

    /// Writes variant.toml for one stored model variant.
    fn save_variant_metadata(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
        variant: &ModelVariantMetadata,
    ) -> KernelResult<()>;
}

/// Supplies timestamps for model proof records.
pub trait ModelClock {
    /// Returns the current UTC timestamp formatted as RFC3339.
    fn now_rfc3339(&self) -> KernelResult<String>;
}

/// Reads and writes latest model capability proof records.
pub trait ModelCapabilityProofStore {
    /// Lists latest capability proofs for one model.
    fn list_capability_proofs(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
    ) -> KernelResult<Vec<ModelCapabilityProof>>;

    /// Saves or replaces the latest proof for one model capability.
    fn save_capability_proof(
        &self,
        layout: &ModelStoreLayout,
        proof: &ModelCapabilityProof,
    ) -> KernelResult<()>;

    /// Removes all latest capability proofs for one capability.
    fn remove_capability_proof(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
        capability: ModelCapability,
    ) -> KernelResult<()>;
}

/// Reads and writes source indexes that point back to canonical model_ref entries.
pub trait ModelSourceIndexStore {
    /// Writes a local source index for a model import.
    fn save_local_source_index(
        &self,
        layout: &ModelStoreLayout,
        index: &LocalModelSourceIndex,
    ) -> KernelResult<PathBuf>;

    /// Writes a Hugging Face source index for a resolved repository revision.
    fn save_hf_source_index(
        &self,
        layout: &ModelStoreLayout,
        index: &HfModelSourceIndex,
    ) -> KernelResult<PathBuf>;

    /// Removes all source indexes that point to a canonical model_ref.
    fn remove_source_indexes(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
    ) -> KernelResult<Vec<PathBuf>>;
}

/// Moves or removes canonical model content in the store.
pub trait ModelContentStore {
    /// Checks whether canonical content for a model_ref already exists.
    fn model_content_exists(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
    ) -> KernelResult<bool>;

    /// Moves staged source content into the canonical variant source directory.
    fn install_staged_source(
        &self,
        layout: &ModelStoreLayout,
        staged: &StagedModelSource,
        model_ref: &ModelRef,
        format: ModelFormat,
    ) -> KernelResult<PathBuf>;

    /// Deletes canonical model content for a resolved model_ref.
    fn remove_model_content(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
    ) -> KernelResult<()>;
}

/// Finds server specs that still reference a model before removal.
pub trait ModelServerReferenceProbe {
    /// Returns stable server refs that would block removal of the model.
    fn server_refs_for_model(
        &self,
        layout: &RuntimeLayout,
        model_ref: &ModelRef,
    ) -> KernelResult<Vec<String>>;
}
