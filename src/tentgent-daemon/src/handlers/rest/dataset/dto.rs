use std::path::Path;

use serde::Serialize;
use tentgent_kernel::features::dataset::domain::{
    DatasetDiffFile, DatasetDiffOutcome, DatasetDiffSide, DatasetDiffStatus, DatasetExportOutcome,
    DatasetImportOutcome, DatasetInspection, DatasetMetadata, DatasetRemovalOutcome,
    DatasetRenderedTemplate, DatasetSummary, DatasetValidationIssue, DatasetValidationOutcome,
    DatasetValidationSplit,
};

const DATASET_DIFF_FILE_LIMIT: usize = 500;

#[derive(Debug, Serialize)]
pub struct DatasetsResponse {
    pub datasets: Vec<DatasetItem>,
}

#[derive(Debug, Serialize)]
pub struct DatasetResponse {
    pub dataset: DatasetItem,
}

#[derive(Debug, Serialize)]
pub struct DatasetMutationResponse {
    pub dataset: DatasetItem,
    pub mutation: DatasetMutationItem,
}

#[derive(Debug, Serialize)]
pub struct DatasetMutationItem {
    pub kind: &'static str,
    pub deduplicated: bool,
    pub store_path: String,
    pub source_index_path: String,
}

#[derive(Debug, Serialize)]
pub struct DatasetValidationResponse {
    pub valid: bool,
    pub source: DatasetValidationSourceItem,
    pub target: String,
    pub tuning_ready: bool,
    pub records: usize,
    pub errors_count: usize,
    pub splits: Vec<DatasetValidationSplitItem>,
    pub warnings: Vec<String>,
    pub errors: Vec<DatasetValidationIssueItem>,
}

#[derive(Debug, Serialize)]
pub struct DatasetValidationSourceItem {
    pub kind: &'static str,
    pub path: String,
    pub dataset_ref: Option<String>,
    pub short_ref: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DatasetValidationSplitItem {
    pub name: String,
    pub path: String,
    pub records: usize,
    pub errors: usize,
}

#[derive(Debug, Serialize)]
pub struct DatasetValidationIssueItem {
    pub path: String,
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct DatasetTemplateResponse {
    pub template_version: String,
    pub task: String,
    pub language: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct DatasetExportResponse {
    pub dataset: DatasetItem,
    pub export: DatasetExportItem,
}

#[derive(Debug, Serialize)]
pub struct DatasetExportItem {
    pub output_path: String,
    pub managed_source_path: String,
    pub file_count: usize,
    pub total_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct DatasetDiffResponse {
    pub left: DatasetDiffSideItem,
    pub right: DatasetDiffSideItem,
    pub diff: DatasetDiffItem,
}

#[derive(Debug, Serialize)]
pub struct DatasetDiffSideItem {
    pub label: String,
    pub short_ref: Option<String>,
    pub path: Option<String>,
    pub tuning_ready: bool,
    pub splits: String,
}

#[derive(Debug, Serialize)]
pub struct DatasetDiffItem {
    pub summary: DatasetDiffSummaryItem,
    pub files: Vec<DatasetDiffFileItem>,
    pub file_limit: usize,
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
pub struct DatasetDiffSummaryItem {
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
    pub unchanged: usize,
    pub left_total_bytes: u64,
    pub right_total_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct DatasetDiffFileItem {
    pub status: &'static str,
    pub relative_path: String,
    pub left_size_bytes: Option<u64>,
    pub right_size_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct DatasetItem {
    pub dataset_ref: String,
    pub short_ref: String,
    pub store_path: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub imported_at: String,
    pub format: String,
    pub source_kind: String,
    pub source_path: Option<String>,
    pub source_repo: Option<String>,
    pub source_revision: Option<String>,
    pub tuning_ready: bool,
    pub splits: DatasetSplitsItem,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub managed_source_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DatasetSplitsItem {
    pub train: Option<String>,
    pub validation: Option<String>,
    pub test: Option<String>,
    pub eval_cases: Option<String>,
    pub source_manifest: Option<String>,
}

pub fn dataset_summary_item(summary: DatasetSummary) -> DatasetItem {
    dataset_item_from_parts(summary.metadata, &summary.store_path, None, None)
}

pub fn dataset_inspection_item(inspection: DatasetInspection) -> DatasetItem {
    dataset_item_from_parts(
        inspection.metadata,
        &inspection.store_path,
        Some(&inspection.manifest_path),
        Some(&inspection.source_path),
    )
}

pub fn dataset_removal_item(outcome: DatasetRemovalOutcome) -> DatasetItem {
    dataset_item_from_parts(outcome.metadata, &outcome.store_path, None, None)
}

pub fn dataset_mutation_response(
    outcome: DatasetImportOutcome,
    kind: &'static str,
) -> DatasetMutationResponse {
    let mutation = DatasetMutationItem {
        kind,
        deduplicated: outcome.deduplicated,
        store_path: path_string(&outcome.store_path),
        source_index_path: path_string(&outcome.source_index_path),
    };
    DatasetMutationResponse {
        dataset: dataset_item_from_parts(outcome.metadata, &outcome.store_path, None, None),
        mutation,
    }
}

pub fn dataset_validation_response(
    outcome: DatasetValidationOutcome,
    dataset: Option<DatasetInspection>,
) -> DatasetValidationResponse {
    let source = match dataset {
        Some(dataset) => DatasetValidationSourceItem {
            kind: "dataset",
            path: path_string(&outcome.path),
            dataset_ref: Some(dataset.metadata.dataset_ref.into_string()),
            short_ref: Some(dataset.metadata.short_ref),
        },
        None => DatasetValidationSourceItem {
            kind: "path",
            path: path_string(&outcome.path),
            dataset_ref: None,
            short_ref: None,
        },
    };
    DatasetValidationResponse {
        valid: outcome.is_valid(),
        source,
        target: outcome.target_kind.as_str().to_string(),
        tuning_ready: outcome.tuning_ready,
        records: outcome.record_count(),
        errors_count: outcome.errors.len(),
        splits: outcome
            .splits
            .into_iter()
            .map(dataset_validation_split_item)
            .collect(),
        warnings: outcome.warnings,
        errors: outcome
            .errors
            .into_iter()
            .map(dataset_validation_issue_item)
            .collect(),
    }
}

pub fn dataset_template_response(
    request: tentgent_kernel::features::dataset::domain::DatasetTemplateRequest,
    rendered: DatasetRenderedTemplate,
) -> DatasetTemplateResponse {
    DatasetTemplateResponse {
        template_version: rendered.template_version,
        task: request.task,
        language: request.language,
        content: rendered.body,
    }
}

pub fn dataset_export_response(
    outcome: DatasetExportOutcome,
    store_path: &Path,
) -> DatasetExportResponse {
    let file_count = outcome.metadata.file_count;
    let total_bytes = outcome.metadata.total_bytes;
    let managed_source_path = path_string(&outcome.managed_source_path);
    let output_path = path_string(&outcome.destination_path);
    let dataset = dataset_item_from_parts(
        outcome.metadata,
        store_path,
        None,
        Some(&outcome.managed_source_path),
    );
    DatasetExportResponse {
        dataset,
        export: DatasetExportItem {
            output_path,
            managed_source_path,
            file_count,
            total_bytes,
        },
    }
}

pub fn dataset_diff_response(outcome: DatasetDiffOutcome) -> DatasetDiffResponse {
    let file_count = outcome.diff.files.len();
    DatasetDiffResponse {
        left: dataset_diff_side_item(outcome.left),
        right: dataset_diff_side_item(outcome.right),
        diff: DatasetDiffItem {
            summary: DatasetDiffSummaryItem {
                added: outcome.diff.summary.added,
                removed: outcome.diff.summary.removed,
                modified: outcome.diff.summary.modified,
                unchanged: outcome.diff.summary.unchanged,
                left_total_bytes: outcome.diff.summary.left_total_bytes,
                right_total_bytes: outcome.diff.summary.right_total_bytes,
            },
            files: outcome
                .diff
                .files
                .into_iter()
                .take(DATASET_DIFF_FILE_LIMIT)
                .map(dataset_diff_file_item)
                .collect(),
            file_limit: DATASET_DIFF_FILE_LIMIT,
            truncated: file_count > DATASET_DIFF_FILE_LIMIT,
        },
    }
}

fn dataset_item_from_parts(
    metadata: DatasetMetadata,
    store_path: &Path,
    manifest_path: Option<&Path>,
    managed_source_path: Option<&Path>,
) -> DatasetItem {
    let package = metadata.package;
    DatasetItem {
        dataset_ref: metadata.dataset_ref.into_string(),
        short_ref: metadata.short_ref,
        store_path: path_string(store_path),
        file_count: metadata.file_count,
        total_bytes: metadata.total_bytes,
        imported_at: metadata.imported_at,
        format: metadata.dataset_format.to_string(),
        source_kind: metadata.source_kind.to_string(),
        source_path: metadata.source_path,
        source_repo: metadata.source_repo,
        source_revision: metadata.source_revision,
        tuning_ready: package.tuning_ready,
        splits: DatasetSplitsItem {
            train: package.splits.train,
            validation: package.splits.validation,
            test: package.splits.test,
            eval_cases: package.splits.eval_cases,
            source_manifest: package.splits.source_manifest,
        },
        warnings: package.warnings,
        manifest_path: manifest_path.map(path_string),
        managed_source_path: managed_source_path.map(path_string),
    }
}

fn dataset_validation_split_item(split: DatasetValidationSplit) -> DatasetValidationSplitItem {
    DatasetValidationSplitItem {
        name: split.name,
        path: path_string(split.path),
        records: split.records,
        errors: split.errors,
    }
}

fn dataset_validation_issue_item(issue: DatasetValidationIssue) -> DatasetValidationIssueItem {
    DatasetValidationIssueItem {
        path: path_string(issue.path),
        line: issue.line,
        message: issue.message,
    }
}

fn dataset_diff_side_item(side: DatasetDiffSide) -> DatasetDiffSideItem {
    DatasetDiffSideItem {
        label: side.label,
        short_ref: side.short_ref,
        path: side.path.map(path_string),
        tuning_ready: side.tuning_ready,
        splits: side.splits,
    }
}

fn dataset_diff_file_item(file: DatasetDiffFile) -> DatasetDiffFileItem {
    DatasetDiffFileItem {
        status: dataset_diff_status(file.status),
        relative_path: file.relative_path,
        left_size_bytes: file.left_size_bytes,
        right_size_bytes: file.right_size_bytes,
    }
}

fn dataset_diff_status(status: DatasetDiffStatus) -> &'static str {
    status.as_str()
}

fn path_string(path: impl AsRef<Path>) -> String {
    path.as_ref().display().to_string()
}
