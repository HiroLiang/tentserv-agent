//! Adapter feature package ports.

use std::path::{Path, PathBuf};

use crate::features::auth::domain::AuthSecretMaterial;
use crate::features::runtime::domain::PythonRuntimeLayout;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;

use super::domain::{
    AdapterInspection, AdapterManifest, AdapterMetadata, AdapterRef, AdapterRefSelector,
    AdapterSourceKind, AdapterStoreLayout, AdapterSummary, BaseModelAdapterIndex,
    HfAdapterPullProgress, HfAdapterSourceIndex, LocalAdapterSourceIndex,
    TrainRunAdapterSourceIndex,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedAdapterSource {
    pub staging_root: PathBuf,
    pub source_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HfAdapterSnapshotRequest {
    pub runtime: PythonRuntimeLayout,
    pub repo_id: String,
    pub revision: Option<String>,
    pub destination_dir: PathBuf,
    pub secret: Option<AuthSecretMaterial>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HfAdapterSnapshot {
    pub repo_id: String,
    pub resolved_revision: String,
    pub local_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdapterSourceMetadata {
    pub base_model_source_repo: Option<String>,
    pub base_model_source_revision: Option<String>,
    pub model_family: Option<String>,
}

/// Ensures adapter-store root directories exist for mutating adapter operations.
pub trait AdapterStoreLayoutInitializer {
    /// Creates store, base-index, source-index, and staging directories.
    fn ensure_adapter_store_layout(&self, layout: &AdapterStoreLayout) -> KernelResult<()>;
}

/// Stages local, remote, or training-run adapter content before canonical identity is known.
pub trait AdapterSourceStager {
    /// Creates an isolated staging root and source directory for one import operation.
    fn create_staging_source(
        &self,
        layout: &AdapterStoreLayout,
        source_kind: AdapterSourceKind,
    ) -> KernelResult<StagedAdapterSource>;

    /// Copies a local adapter directory or training-run output into a staging source directory.
    fn copy_local_source(
        &self,
        input_path: &Path,
        staged: &StagedAdapterSource,
    ) -> KernelResult<()>;

    /// Removes a staging root after successful import, deduplication, or failure cleanup.
    fn discard_staging(&self, staged: &StagedAdapterSource) -> KernelResult<()>;
}

/// Fetches a Hugging Face adapter snapshot into a caller-provided staging directory.
pub trait HfAdapterSnapshotFetcher {
    /// Runs the snapshot helper and reports progress without deciding canonical adapter identity.
    fn fetch_hf_snapshot(
        &self,
        request: HfAdapterSnapshotRequest,
        progress: &mut dyn FnMut(HfAdapterPullProgress),
    ) -> KernelResult<HfAdapterSnapshot>;
}

/// Builds a canonical file manifest from staged adapter content.
pub trait AdapterManifestBuilder {
    /// Walks staged source content and records normalized relative paths, sizes, and hashes.
    fn build_manifest(&self, source_root: &Path) -> KernelResult<AdapterManifest>;
}

/// Generates canonical adapter identity from manifest data.
pub trait AdapterIdentityGenerator {
    /// Hashes the canonical manifest representation into a full adapter_ref.
    fn adapter_ref_for_manifest(&self, manifest: &AdapterManifest) -> KernelResult<AdapterRef>;
}

/// Reads adapter-source metadata such as base-model hints from staged content.
pub trait AdapterSourceMetadataReader {
    /// Reads optional source-level compatibility hints without validating a local base model.
    fn read_source_metadata(&self, source_root: &Path) -> KernelResult<AdapterSourceMetadata>;
}

/// Reads and writes adapter catalog metadata.
pub trait AdapterCatalogStore {
    /// Lists stored adapter metadata summaries sorted for stable display.
    fn list_adapters(&self, layout: &AdapterStoreLayout) -> KernelResult<Vec<AdapterSummary>>;

    /// Resolves a full hash or unique hash prefix and returns full inspection paths.
    fn inspect_adapter(
        &self,
        layout: &AdapterStoreLayout,
        selector: &AdapterRefSelector,
    ) -> KernelResult<AdapterInspection>;

    /// Loads metadata for an already resolved adapter_ref.
    fn load_adapter_metadata(
        &self,
        layout: &AdapterStoreLayout,
        adapter_ref: &AdapterRef,
    ) -> KernelResult<AdapterMetadata>;

    /// Writes adapter.toml for an imported, deduplicated, or rebound adapter.
    fn save_adapter_metadata(
        &self,
        layout: &AdapterStoreLayout,
        metadata: &AdapterMetadata,
    ) -> KernelResult<()>;

    /// Writes manifest.json for an imported adapter.
    fn save_adapter_manifest(
        &self,
        layout: &AdapterStoreLayout,
        adapter_ref: &AdapterRef,
        manifest: &AdapterManifest,
    ) -> KernelResult<()>;
}

/// Reads and writes source indexes that point back to canonical adapter_ref entries.
pub trait AdapterSourceIndexStore {
    /// Writes a local source index for an adapter import.
    fn save_local_source_index(
        &self,
        layout: &AdapterStoreLayout,
        index: &LocalAdapterSourceIndex,
    ) -> KernelResult<PathBuf>;

    /// Writes a Hugging Face source index for a resolved repository revision.
    fn save_hf_source_index(
        &self,
        layout: &AdapterStoreLayout,
        index: &HfAdapterSourceIndex,
    ) -> KernelResult<PathBuf>;

    /// Writes a training-run source index for an imported training output.
    fn save_train_run_source_index(
        &self,
        layout: &AdapterStoreLayout,
        index: &TrainRunAdapterSourceIndex,
    ) -> KernelResult<PathBuf>;

    /// Removes local, Hugging Face, and training-run source indexes for one adapter_ref.
    fn remove_source_indexes(
        &self,
        layout: &AdapterStoreLayout,
        adapter_ref: &AdapterRef,
    ) -> KernelResult<Vec<PathBuf>>;
}

/// Reads and writes base-model indexes for adapters bound to local managed models.
pub trait AdapterBaseIndexStore {
    /// Writes a base-model index for a proven adapter-to-model binding.
    fn save_base_model_index(
        &self,
        layout: &AdapterStoreLayout,
        index: &BaseModelAdapterIndex,
    ) -> KernelResult<PathBuf>;

    /// Removes one base-model index entry during rebind or removal.
    fn remove_base_model_index(
        &self,
        layout: &AdapterStoreLayout,
        index: &BaseModelAdapterIndex,
    ) -> KernelResult<Option<PathBuf>>;

    /// Removes all base-model index entries that point to a canonical adapter_ref.
    fn remove_base_model_indexes(
        &self,
        layout: &AdapterStoreLayout,
        adapter_ref: &AdapterRef,
    ) -> KernelResult<Vec<PathBuf>>;
}

/// Moves or removes canonical adapter content in the store.
pub trait AdapterContentStore {
    /// Checks whether canonical content for an adapter_ref already exists.
    fn adapter_content_exists(
        &self,
        layout: &AdapterStoreLayout,
        adapter_ref: &AdapterRef,
    ) -> KernelResult<bool>;

    /// Moves staged source content into the canonical adapter source directory.
    fn install_staged_source(
        &self,
        layout: &AdapterStoreLayout,
        staged: &StagedAdapterSource,
        adapter_ref: &AdapterRef,
    ) -> KernelResult<PathBuf>;

    /// Deletes canonical adapter content for a resolved adapter_ref.
    fn remove_adapter_content(
        &self,
        layout: &AdapterStoreLayout,
        adapter_ref: &AdapterRef,
    ) -> KernelResult<()>;
}

/// Finds server specs that still reference an adapter before removal.
pub trait AdapterServerReferenceProbe {
    /// Returns stable server refs that would block removal of the adapter.
    fn server_refs_for_adapter(
        &self,
        layout: &RuntimeLayout,
        adapter_ref: &AdapterRef,
    ) -> KernelResult<Vec<String>>;
}
