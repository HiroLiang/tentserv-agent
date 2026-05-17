use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::features::dataset::domain::{
    DatasetDiffFile, DatasetDiffOutcome, DatasetDiffSide, DatasetDiffStatus, DatasetDiffSummary,
    DatasetManifest, DatasetManifestDiff, DatasetPackageMetadata, DatasetStoreLayout,
};
use crate::features::dataset::ports::{DatasetDiffTarget, DatasetDiffer};
use crate::foundation::error::KernelResult;

use super::catalog::FileDatasetCatalogStore;
use super::error::{dataset_store_error, path_error};
use super::manifest::build_manifest_from_root;
use super::package::detect_package_from_root;
use crate::features::dataset::ports::DatasetCatalogStore;

/// Standard dataset manifest differ.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdDatasetDiffer;

impl DatasetDiffer for StdDatasetDiffer {
    fn diff_manifests(
        &self,
        left: &DatasetManifest,
        right: &DatasetManifest,
    ) -> KernelResult<DatasetManifestDiff> {
        Ok(diff_manifests(left, right))
    }

    fn diff_dataset(
        &self,
        layout: &DatasetStoreLayout,
        left: &crate::features::dataset::domain::DatasetRefSelector,
        right: DatasetDiffTarget,
    ) -> KernelResult<DatasetDiffOutcome> {
        let catalog = FileDatasetCatalogStore;
        let left = catalog.inspect_dataset(layout, left)?;
        let left_manifest = read_manifest(&left.manifest_path)?;

        let (right_side, right_manifest) = match right {
            DatasetDiffTarget::Dataset(selector) => {
                let inspection = catalog.inspect_dataset(layout, &selector)?;
                let manifest = read_manifest(&inspection.manifest_path)?;
                (diff_side_for_metadata(&inspection.metadata), manifest)
            }
            DatasetDiffTarget::LocalPath(path) => {
                let local = LocalSourceRoot::prepare(&path)?;
                let manifest = build_manifest_from_root(&local.root)?;
                let package = detect_package_from_root(&local.root, &manifest)?;
                (
                    DatasetDiffSide {
                        label: path.display().to_string(),
                        short_ref: None,
                        tuning_ready: package.tuning_ready,
                        splits: split_summary_for_package(&package),
                        path: Some(path),
                    },
                    manifest,
                )
            }
        };

        Ok(DatasetDiffOutcome {
            left: diff_side_for_metadata(&left.metadata),
            right: right_side,
            diff: diff_manifests(&left_manifest, &right_manifest),
        })
    }
}

fn diff_manifests(left: &DatasetManifest, right: &DatasetManifest) -> DatasetManifestDiff {
    let left_by_path = left
        .files
        .iter()
        .map(|entry| (entry.relative_path.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let right_by_path = right
        .files
        .iter()
        .map(|entry| (entry.relative_path.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let mut paths = left_by_path.keys().copied().collect::<Vec<_>>();

    for path in right_by_path.keys() {
        if !left_by_path.contains_key(path) {
            paths.push(path);
        }
    }

    paths.sort_unstable();

    let mut summary = DatasetDiffSummary {
        left_total_bytes: left.total_bytes(),
        right_total_bytes: right.total_bytes(),
        ..DatasetDiffSummary::default()
    };
    let mut files = Vec::new();

    for path in paths {
        let left_entry = left_by_path.get(path).copied();
        let right_entry = right_by_path.get(path).copied();
        let status = match (left_entry, right_entry) {
            (None, Some(_)) => DatasetDiffStatus::Added,
            (Some(_), None) => DatasetDiffStatus::Removed,
            (Some(left_entry), Some(right_entry))
                if left_entry.sha256 == right_entry.sha256
                    && left_entry.size_bytes == right_entry.size_bytes =>
            {
                DatasetDiffStatus::Unchanged
            }
            (Some(_), Some(_)) => DatasetDiffStatus::Modified,
            (None, None) => continue,
        };

        match status {
            DatasetDiffStatus::Added => summary.added += 1,
            DatasetDiffStatus::Removed => summary.removed += 1,
            DatasetDiffStatus::Modified => summary.modified += 1,
            DatasetDiffStatus::Unchanged => summary.unchanged += 1,
        }

        files.push(DatasetDiffFile {
            status,
            relative_path: path.to_string(),
            left_size_bytes: left_entry.map(|entry| entry.size_bytes),
            right_size_bytes: right_entry.map(|entry| entry.size_bytes),
        });
    }

    DatasetManifestDiff { summary, files }
}

fn read_manifest(path: &Path) -> KernelResult<DatasetManifest> {
    let body =
        fs::read_to_string(path).map_err(|err| path_error("read dataset manifest", path, err))?;
    serde_json::from_str(&body).map_err(|err| {
        dataset_store_error(format!(
            "parse dataset manifest `{}` failed: {err}",
            path.display()
        ))
    })
}

struct LocalSourceRoot {
    root: std::path::PathBuf,
    cleanup: bool,
}

impl LocalSourceRoot {
    fn prepare(path: &Path) -> KernelResult<Self> {
        if !path.exists() {
            return Err(dataset_store_error(format!(
                "dataset diff path does not exist: `{}`",
                path.display()
            )));
        }
        if path.is_dir() {
            return Ok(Self {
                root: path.to_path_buf(),
                cleanup: false,
            });
        }
        if path.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
        {
            let temp = std::env::temp_dir().join(format!(
                "tentgent-dataset-diff-{}-{}",
                std::process::id(),
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("jsonl")
            ));
            if temp.exists() {
                fs::remove_dir_all(&temp)
                    .map_err(|err| path_error("remove stale dataset diff staging", &temp, err))?;
            }
            fs::create_dir_all(&temp)
                .map_err(|err| path_error("create dataset diff staging", &temp, err))?;
            let file_name = path.file_name().ok_or_else(|| {
                dataset_store_error(format!(
                    "dataset diff file has no file name: `{}`",
                    path.display()
                ))
            })?;
            fs::copy(path, temp.join(file_name))
                .map_err(|err| path_error("copy dataset diff file", path, err))?;
            return Ok(Self {
                root: temp,
                cleanup: true,
            });
        }
        Err(dataset_store_error(format!(
            "dataset diff path is not a JSONL file or directory: `{}`",
            path.display()
        )))
    }
}

impl Drop for LocalSourceRoot {
    fn drop(&mut self) {
        if self.cleanup {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

fn diff_side_for_metadata(
    metadata: &crate::features::dataset::domain::DatasetMetadata,
) -> DatasetDiffSide {
    DatasetDiffSide {
        label: metadata.short_ref.clone(),
        short_ref: Some(metadata.short_ref.clone()),
        tuning_ready: metadata.package.tuning_ready,
        splits: split_summary_for_package(&metadata.package),
        path: None,
    }
}

fn split_summary_for_package(package: &DatasetPackageMetadata) -> String {
    let names = package.splits.split_names();
    if names.is_empty() {
        "-".to_string()
    } else {
        names.join(",")
    }
}
