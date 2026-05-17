use std::path::Path;

use crate::features::dataset::domain::{
    DatasetManifest, DatasetPackageMetadata, DatasetSplits, EVAL_CASES_SPLIT_FILENAME,
    LEGACY_VALID_SPLIT_FILENAME, SOURCE_MANIFEST_FILENAME, TEST_SPLIT_FILENAME,
    TRAIN_SPLIT_FILENAME, VALID_SPLIT_FILENAME,
};
use crate::features::dataset::ports::DatasetPackageDetector;
use crate::foundation::error::KernelResult;

use super::error::path_error;

/// Detects tuning package shape from dataset source content.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdDatasetPackageDetector;

impl DatasetPackageDetector for StdDatasetPackageDetector {
    fn detect_package(
        &self,
        source_root: &Path,
        manifest: &DatasetManifest,
    ) -> KernelResult<DatasetPackageMetadata> {
        detect_package_from_root(source_root, manifest)
    }
}

pub(super) fn detect_package_from_root(
    source_root: &Path,
    manifest: &DatasetManifest,
) -> KernelResult<DatasetPackageMetadata> {
    if is_single_jsonl_manifest(manifest) {
        return detect_single_jsonl_package(manifest);
    }

    detect_directory_package(source_root)
}

fn detect_single_jsonl_package(manifest: &DatasetManifest) -> KernelResult<DatasetPackageMetadata> {
    let Some(file) = manifest
        .files
        .iter()
        .find(|entry| entry.relative_path.ends_with(".jsonl"))
    else {
        return Ok(DatasetPackageMetadata {
            tuning_ready: false,
            splits: DatasetSplits::default(),
            warnings: vec!["single-file dataset import did not contain a root JSONL file".into()],
        });
    };

    let mut warnings = Vec::new();
    if file.relative_path != TRAIN_SPLIT_FILENAME {
        warnings.push(format!(
            "single JSONL import `{}` is treated as the train split; use `train.jsonl` in a dataset directory for canonical tuning packages",
            file.relative_path
        ));
    }

    Ok(DatasetPackageMetadata {
        tuning_ready: true,
        splits: DatasetSplits {
            train: Some(file.relative_path.clone()),
            ..DatasetSplits::default()
        },
        warnings,
    })
}

fn detect_directory_package(source_root: &Path) -> KernelResult<DatasetPackageMetadata> {
    let mut splits = DatasetSplits {
        train: root_file(source_root, TRAIN_SPLIT_FILENAME),
        validation: root_file(source_root, VALID_SPLIT_FILENAME),
        test: root_file(source_root, TEST_SPLIT_FILENAME),
        eval_cases: root_file(source_root, EVAL_CASES_SPLIT_FILENAME),
        source_manifest: root_file(source_root, SOURCE_MANIFEST_FILENAME),
    };
    let mut warnings = Vec::new();

    let legacy_valid = root_file(source_root, LEGACY_VALID_SPLIT_FILENAME);
    match (splits.validation.as_deref(), legacy_valid.as_deref()) {
        (None, Some(path)) => {
            splits.validation = Some(path.to_string());
            warnings.push(
                "`val.jsonl` detected; treating it as validation, but `valid.jsonl` is the canonical MLX-compatible name".into(),
            );
        }
        (Some(_), Some(_))
            if files_match(
                source_root,
                VALID_SPLIT_FILENAME,
                LEGACY_VALID_SPLIT_FILENAME,
            )? =>
        {
            warnings.push(
                "`val.jsonl` duplicates `valid.jsonl`; `valid.jsonl` is used as the validation split".into(),
            );
        }
        (Some(_), Some(_)) => {
            warnings.push(
                "`val.jsonl` and `valid.jsonl` both exist but differ; `valid.jsonl` is used as the validation split".into(),
            );
        }
        _ => {}
    }

    if splits.train.is_none() {
        warnings.push(
            "no root `train.jsonl` detected; this dataset can be stored but is not ready for tuning".into(),
        );
    }

    Ok(DatasetPackageMetadata {
        tuning_ready: splits.train.is_some(),
        splits,
        warnings,
    })
}

fn is_single_jsonl_manifest(manifest: &DatasetManifest) -> bool {
    manifest.files.len() == 1
        && manifest.files[0].relative_path.ends_with(".jsonl")
        && !manifest.files[0].relative_path.contains('/')
}

fn root_file(source_root: &Path, file_name: &str) -> Option<String> {
    source_root
        .join(file_name)
        .is_file()
        .then(|| file_name.to_string())
}

fn files_match(source_root: &Path, left: &str, right: &str) -> KernelResult<bool> {
    let left = std::fs::read(source_root.join(left))
        .map_err(|err| path_error("read dataset split file", &source_root.join(left), err))?;
    let right = std::fs::read(source_root.join(right))
        .map_err(|err| path_error("read dataset split file", &source_root.join(right), err))?;
    Ok(left == right)
}
