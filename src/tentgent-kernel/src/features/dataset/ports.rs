//! Dataset feature package ports.

use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
};

use crate::features::auth::domain::AuthSecretMaterial;
use crate::features::runtime::domain::PythonRuntimeLayout;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;

use super::domain::{
    DatasetDiffOutcome, DatasetEvalRequest, DatasetInspection, DatasetManifest,
    DatasetManifestDiff, DatasetMetadata, DatasetPackageMetadata, DatasetRef, DatasetRefSelector,
    DatasetRenderedTemplate, DatasetRuntimeDebug, DatasetStoreLayout, DatasetSummary,
    DatasetSynthPromptRequest, DatasetSynthRequest, DatasetSynthRuntimeOutput,
    DatasetTemplateRequest, DatasetValidationOutcome, LocalDatasetSourceIndex,
};

pub type DatasetPortFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedDatasetSource {
    pub staging_root: PathBuf,
    pub source_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetRuntimeAuth {
    pub secret: AuthSecretMaterial,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DatasetSynthRuntimeRequest {
    pub runtime: PythonRuntimeLayout,
    pub auth: DatasetRuntimeAuth,
    pub request: DatasetSynthRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetSynthPromptRuntimeRequest {
    pub runtime: PythonRuntimeLayout,
    pub request: DatasetSynthPromptRequest,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DatasetEvalRuntimeRequest {
    pub runtime: PythonRuntimeLayout,
    pub auth: DatasetRuntimeAuth,
    pub request: DatasetEvalRequest,
}

/// Ensures dataset-store root directories exist for mutating dataset operations.
pub trait DatasetStoreLayoutInitializer {
    /// Creates the store, source-index, and staging directories for the layout.
    fn ensure_dataset_store_layout(&self, layout: &DatasetStoreLayout) -> KernelResult<()>;
}

/// Stages local or generated dataset content before canonical identity is known.
pub trait DatasetSourceStager {
    /// Creates an isolated staging root and source directory for one dataset operation.
    fn create_staging_source(
        &self,
        layout: &DatasetStoreLayout,
        prefix: &str,
    ) -> KernelResult<StagedDatasetSource>;

    /// Copies a local JSONL file or dataset directory into an existing staging source directory.
    fn copy_local_source(
        &self,
        input_path: &Path,
        staged: &StagedDatasetSource,
    ) -> KernelResult<()>;

    /// Removes a staging root after import, diff, deduplication, or failure cleanup.
    fn discard_staging(&self, staged: &StagedDatasetSource) -> KernelResult<()>;
}

/// Builds a canonical file manifest from staged dataset content.
pub trait DatasetManifestBuilder {
    /// Walks source content and records normalized relative paths, sizes, and hashes.
    fn build_manifest(&self, source_root: &Path) -> KernelResult<DatasetManifest>;
}

/// Generates canonical dataset identity from manifest data.
pub trait DatasetIdentityGenerator {
    /// Hashes the canonical manifest representation into a full dataset_ref.
    fn dataset_ref_for_manifest(&self, manifest: &DatasetManifest) -> KernelResult<DatasetRef>;
}

/// Detects training package shape and split readiness from staged dataset content.
pub trait DatasetPackageDetector {
    /// Detects split files, tuning readiness, and package warnings.
    fn detect_package(
        &self,
        source_root: &Path,
        manifest: &DatasetManifest,
    ) -> KernelResult<DatasetPackageMetadata>;
}

/// Reads and writes dataset catalog metadata.
pub trait DatasetCatalogStore {
    /// Lists stored dataset metadata summaries sorted for stable display.
    fn list_datasets(&self, layout: &DatasetStoreLayout) -> KernelResult<Vec<DatasetSummary>>;

    /// Resolves a full hash or unique hash prefix and returns full inspection paths.
    fn inspect_dataset(
        &self,
        layout: &DatasetStoreLayout,
        selector: &DatasetRefSelector,
    ) -> KernelResult<DatasetInspection>;

    /// Loads metadata for an already resolved dataset_ref.
    fn load_dataset_metadata(
        &self,
        layout: &DatasetStoreLayout,
        dataset_ref: &DatasetRef,
    ) -> KernelResult<DatasetMetadata>;

    /// Writes dataset.toml for an imported or deduplicated dataset.
    fn save_dataset_metadata(
        &self,
        layout: &DatasetStoreLayout,
        metadata: &DatasetMetadata,
    ) -> KernelResult<()>;

    /// Writes manifest.json for an imported dataset.
    fn save_dataset_manifest(
        &self,
        layout: &DatasetStoreLayout,
        dataset_ref: &DatasetRef,
        manifest: &DatasetManifest,
    ) -> KernelResult<()>;
}

/// Reads and writes source indexes that point back to canonical dataset_ref entries.
pub trait DatasetSourceIndexStore {
    /// Writes a local source index for a dataset import.
    fn save_local_source_index(
        &self,
        layout: &DatasetStoreLayout,
        index: &LocalDatasetSourceIndex,
    ) -> KernelResult<PathBuf>;

    /// Removes all source indexes that point to a canonical dataset_ref.
    fn remove_source_indexes(
        &self,
        layout: &DatasetStoreLayout,
        dataset_ref: &DatasetRef,
    ) -> KernelResult<Vec<PathBuf>>;
}

/// Moves, exports, or removes canonical dataset content in the store.
pub trait DatasetContentStore {
    /// Checks whether canonical content for a dataset_ref already exists.
    fn dataset_content_exists(
        &self,
        layout: &DatasetStoreLayout,
        dataset_ref: &DatasetRef,
    ) -> KernelResult<bool>;

    /// Moves staged source content into the canonical dataset source directory.
    fn install_staged_source(
        &self,
        layout: &DatasetStoreLayout,
        staged: &StagedDatasetSource,
        dataset_ref: &DatasetRef,
    ) -> KernelResult<PathBuf>;

    /// Copies canonical source content into a caller-provided working directory.
    fn export_source(
        &self,
        layout: &DatasetStoreLayout,
        dataset_ref: &DatasetRef,
        destination: &Path,
    ) -> KernelResult<PathBuf>;

    /// Deletes canonical dataset content for a resolved dataset_ref.
    fn remove_dataset_content(
        &self,
        layout: &DatasetStoreLayout,
        dataset_ref: &DatasetRef,
    ) -> KernelResult<()>;
}

/// Validates local or managed dataset paths against the canonical dataset schema.
pub trait DatasetValidator {
    /// Validates a local JSONL file or dataset directory.
    fn validate_dataset_path(&self, path: &Path) -> KernelResult<DatasetValidationOutcome>;
}

/// Compares dataset manifests without mutating canonical store content.
pub trait DatasetDiffer {
    /// Compares two manifests and returns a stable file-level diff.
    fn diff_manifests(
        &self,
        left: &DatasetManifest,
        right: &DatasetManifest,
    ) -> KernelResult<DatasetManifestDiff>;

    /// Compares a stored dataset against another stored dataset or staged local source.
    fn diff_dataset(
        &self,
        layout: &DatasetStoreLayout,
        left: &DatasetRefSelector,
        right: DatasetDiffTarget,
    ) -> KernelResult<DatasetDiffOutcome>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatasetDiffTarget {
    Dataset(DatasetRefSelector),
    LocalPath(PathBuf),
}

/// Renders and writes editable Markdown dataset prompt templates.
pub trait DatasetTemplateRenderer {
    /// Renders the dataset generation template from managed Markdown template files.
    fn render_template(
        &self,
        request: &DatasetTemplateRequest,
    ) -> KernelResult<DatasetRenderedTemplate>;

    /// Writes a rendered template to a caller-provided path.
    fn write_template(&self, template: &DatasetRenderedTemplate, path: &Path) -> KernelResult<()>;
}

/// Executes provider-backed dataset synthesis through an external runtime.
pub trait DatasetSynthRuntimeClient {
    /// Renders the provider prompt without auth or network calls.
    fn render_synth_prompt<'a>(
        &'a self,
        request: DatasetSynthPromptRuntimeRequest,
    ) -> DatasetPortFuture<'a, String>;

    /// Runs provider-backed dataset synthesis and returns the helper JSON outcome.
    fn synthesize_dataset<'a>(
        &'a self,
        request: DatasetSynthRuntimeRequest,
    ) -> DatasetPortFuture<'a, DatasetSynthRuntimeOutput>;
}

/// Executes provider-backed dataset evaluation through an external runtime.
pub trait DatasetEvalRuntimeClient {
    /// Runs provider-backed dataset evaluation and returns the helper JSON outcome.
    fn evaluate_dataset<'a>(
        &'a self,
        request: DatasetEvalRuntimeRequest,
    ) -> DatasetPortFuture<'a, serde_json::Value>;

    /// Maps a runtime failure into optional debug artifact paths when available.
    fn runtime_debug(&self, error_detail: &str) -> Option<DatasetRuntimeDebug>;
}

/// Finds train plans or runs that still reference a dataset before removal.
pub trait DatasetReferenceGuard {
    /// Returns stable train refs that would block removal of the dataset.
    fn train_refs_for_dataset(
        &self,
        layout: &RuntimeLayout,
        dataset_ref: &DatasetRef,
    ) -> KernelResult<Vec<String>>;
}
