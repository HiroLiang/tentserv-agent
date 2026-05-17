use std::path::Path;

use serde::Serialize;
use tentgent_kernel::features::dataset::domain::{
    DatasetInspection, DatasetMetadata, DatasetSummary,
};

#[derive(Debug, Serialize)]
pub struct DatasetsResponse {
    pub datasets: Vec<DatasetItem>,
}

#[derive(Debug, Serialize)]
pub struct DatasetResponse {
    pub dataset: DatasetItem,
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

fn path_string(path: impl AsRef<Path>) -> String {
    path.as_ref().display().to_string()
}
