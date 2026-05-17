//! Dataset use case ports.

use std::{future::Future, path::PathBuf, pin::Pin};

use crate::features::auth::usecases::AuthSecretResolutionRequest;
use crate::features::dataset::domain::{
    DatasetDiffOutcome, DatasetEvalSplit, DatasetExportOutcome, DatasetImportOutcome,
    DatasetInspection, DatasetProvider, DatasetRefSelector, DatasetRemovalOutcome,
    DatasetRenderedTemplate, DatasetStoreLayout, DatasetSummary, DatasetSynthPromptRequest,
    DatasetSynthRequest, DatasetSynthRuntimeOutput, DatasetTemplateRequest,
    DatasetValidationOutcome,
};
use crate::features::runtime::domain::{PythonRuntimeLayout, PythonRuntimeResolutionInput};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

/// Boxed async return type used by dataset use cases that execute runtime work.
pub type DatasetUseCaseFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

/// Request for listing managed datasets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetListRequest {
    pub layout: RuntimeLayoutInput,
}

/// Result of listing managed datasets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetListResult {
    pub layout: RuntimeLayout,
    pub store: DatasetStoreLayout,
    pub datasets: Vec<DatasetSummary>,
}

/// Request for inspecting one managed dataset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetInspectRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: DatasetRefSelector,
}

/// Result of inspecting one managed dataset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetInspectResult {
    pub layout: RuntimeLayout,
    pub store: DatasetStoreLayout,
    pub dataset: DatasetInspection,
}

/// Request for importing a local dataset file or directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetLocalImportRequest {
    pub layout: RuntimeLayoutInput,
    pub source_path: PathBuf,
}

/// Result of importing a local dataset source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetLocalImportResult {
    pub layout: RuntimeLayout,
    pub store: DatasetStoreLayout,
    pub outcome: DatasetImportOutcome,
}

/// Validation target selected by CLI, HTTP, or TUI callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatasetValidationTargetSelection {
    LocalPath(PathBuf),
    ManagedDataset(DatasetRefSelector),
}

/// Request for validating local or managed dataset content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetValidateRequest {
    pub layout: RuntimeLayoutInput,
    pub target: DatasetValidationTargetSelection,
}

/// Result of validating local or managed dataset content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetValidateResult {
    pub layout: RuntimeLayout,
    pub store: DatasetStoreLayout,
    pub dataset: Option<DatasetInspection>,
    pub outcome: DatasetValidationOutcome,
}

/// Request for rendering or writing the editable dataset generation template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetTemplateRenderRequest {
    pub template: DatasetTemplateRequest,
    pub output_path: Option<PathBuf>,
}

/// Result of rendering or writing the editable dataset generation template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetTemplateRenderResult {
    pub rendered: DatasetRenderedTemplate,
    pub output_path: Option<PathBuf>,
}

/// Request for rendering the exact provider prompt used by dataset synthesis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetSynthPromptRenderRequest {
    pub layout: RuntimeLayoutInput,
    pub runtime: PythonRuntimeResolutionInput,
    pub prompt: DatasetSynthPromptRequest,
}

/// Result of rendering the exact provider prompt used by dataset synthesis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetSynthPromptRenderResult {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
    pub prompt: String,
}

/// Request for provider-backed dataset synthesis.
#[derive(Debug, Clone, PartialEq)]
pub struct DatasetSynthesizeRequest {
    pub layout: RuntimeLayoutInput,
    pub runtime: PythonRuntimeResolutionInput,
    pub auth: AuthSecretResolutionRequest,
    pub synth: DatasetSynthRequest,
}

/// Result of provider-backed dataset synthesis.
#[derive(Debug, Clone, PartialEq)]
pub struct DatasetSynthesizeResult {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
    pub output: DatasetSynthRuntimeOutput,
}

/// Dataset input selected for provider-backed evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatasetEvaluationInputSelection {
    LocalPath(PathBuf),
    ManagedDataset(DatasetRefSelector),
}

/// Request for provider-backed dataset evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct DatasetEvaluateRequest {
    pub layout: RuntimeLayoutInput,
    pub runtime: PythonRuntimeResolutionInput,
    pub auth: AuthSecretResolutionRequest,
    pub provider: DatasetProvider,
    pub provider_model: String,
    pub input: DatasetEvaluationInputSelection,
    pub output_dir: PathBuf,
    pub split: DatasetEvalSplit,
    pub max_records: u32,
    pub criteria: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: f32,
    pub timeout_seconds: f32,
}

/// Result of provider-backed dataset evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct DatasetEvaluateResult {
    pub layout: RuntimeLayout,
    pub store: DatasetStoreLayout,
    pub runtime: PythonRuntimeLayout,
    pub dataset: Option<DatasetInspection>,
    pub input_path: PathBuf,
    pub report: serde_json::Value,
}

/// Request for exporting one managed dataset source into a working directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetExportRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: DatasetRefSelector,
    pub destination_path: PathBuf,
}

/// Result of exporting one managed dataset source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetExportResult {
    pub layout: RuntimeLayout,
    pub store: DatasetStoreLayout,
    pub outcome: DatasetExportOutcome,
}

/// Right-hand side selected for a dataset diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatasetDiffRightSelection {
    ManagedDataset(DatasetRefSelector),
    LocalPath(PathBuf),
}

/// Request for diffing one managed dataset against another dataset or local path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetDiffRequest {
    pub layout: RuntimeLayoutInput,
    pub left: DatasetRefSelector,
    pub right: DatasetDiffRightSelection,
}

/// Result of diffing dataset content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetDiffResult {
    pub layout: RuntimeLayout,
    pub store: DatasetStoreLayout,
    pub outcome: DatasetDiffOutcome,
}

/// Request for removing one managed dataset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetRemoveRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: DatasetRefSelector,
}

/// Result of removing one managed dataset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetRemoveResult {
    pub layout: RuntimeLayout,
    pub store: DatasetStoreLayout,
    pub outcome: DatasetRemovalOutcome,
}

/// Use-case boundary for read-only dataset catalog operations.
pub trait DatasetCatalogReadUseCase {
    /// Lists managed datasets without mutating the dataset store.
    fn list_datasets(&self, request: DatasetListRequest) -> KernelResult<DatasetListResult>;

    /// Inspects one dataset by full dataset_ref or unique prefix.
    fn inspect_dataset(&self, request: DatasetInspectRequest)
        -> KernelResult<DatasetInspectResult>;
}

/// Use-case boundary for importing local dataset content.
pub trait DatasetLocalImportUseCase {
    /// Stages, manifests, deduplicates, and records one local dataset import.
    fn import_local_dataset(
        &self,
        request: DatasetLocalImportRequest,
    ) -> KernelResult<DatasetLocalImportResult>;
}

/// Use-case boundary for validating dataset content.
pub trait DatasetValidationUseCase {
    /// Validates a local path or managed dataset source against the canonical schema.
    fn validate_dataset(
        &self,
        request: DatasetValidateRequest,
    ) -> KernelResult<DatasetValidateResult>;
}

/// Use-case boundary for editable dataset template rendering.
pub trait DatasetTemplateUseCase {
    /// Renders a Markdown-backed template and optionally writes it to disk.
    fn render_dataset_template(
        &self,
        request: DatasetTemplateRenderRequest,
    ) -> KernelResult<DatasetTemplateRenderResult>;
}

/// Use-case boundary for provider-backed dataset synthesis.
pub trait DatasetSynthesisUseCase {
    /// Resolves runtime and renders the exact synthesis prompt without auth or network calls.
    fn render_synth_prompt<'a>(
        &'a self,
        request: DatasetSynthPromptRenderRequest,
    ) -> DatasetUseCaseFuture<'a, DatasetSynthPromptRenderResult>;

    /// Resolves runtime/auth and asks the provider runtime helper to write dataset files.
    fn synthesize_dataset<'a>(
        &'a self,
        request: DatasetSynthesizeRequest,
    ) -> DatasetUseCaseFuture<'a, DatasetSynthesizeResult>;
}

/// Use-case boundary for provider-backed dataset evaluation.
pub trait DatasetEvaluationUseCase {
    /// Resolves local or managed input, runtime, and auth before writing an evaluation report.
    fn evaluate_dataset<'a>(
        &'a self,
        request: DatasetEvaluateRequest,
    ) -> DatasetUseCaseFuture<'a, DatasetEvaluateResult>;
}

/// Use-case boundary for exporting managed dataset sources.
pub trait DatasetExportUseCase {
    /// Resolves a managed dataset and copies its source into a caller-provided directory.
    fn export_dataset(&self, request: DatasetExportRequest) -> KernelResult<DatasetExportResult>;
}

/// Use-case boundary for dataset diffs.
pub trait DatasetDiffUseCase {
    /// Compares one managed dataset against another managed dataset or local working copy.
    fn diff_dataset(&self, request: DatasetDiffRequest) -> KernelResult<DatasetDiffResult>;
}

/// Use-case boundary for removing managed datasets.
pub trait DatasetRemoveUseCase {
    /// Resolves a dataset ref, checks training references, and removes content plus indexes.
    fn remove_dataset(&self, request: DatasetRemoveRequest) -> KernelResult<DatasetRemoveResult>;
}
