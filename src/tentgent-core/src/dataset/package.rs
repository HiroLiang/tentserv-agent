use std::path::Path;

use super::{
    error::DatasetError,
    hash,
    store::{DatasetFormat, DatasetPackageMetadata, DatasetSplits},
};

const TRAIN_FILE: &str = "train.jsonl";
const VALID_FILE: &str = "valid.jsonl";
const LEGACY_VAL_FILE: &str = "val.jsonl";
const TEST_FILE: &str = "test.jsonl";
const EVAL_CASES_FILE: &str = "eval_cases.jsonl";
const SOURCE_MANIFEST_FILE: &str = "manifest.json";

pub fn detect_dataset_package(
    source_root: &Path,
    dataset_format: DatasetFormat,
) -> Result<DatasetPackageMetadata, DatasetError> {
    match dataset_format {
        DatasetFormat::Jsonl => detect_single_jsonl_package(source_root),
        DatasetFormat::Directory => detect_directory_package(source_root),
    }
}

fn detect_single_jsonl_package(source_root: &Path) -> Result<DatasetPackageMetadata, DatasetError> {
    let Some(file_name) = first_root_jsonl_file(source_root)? else {
        return Ok(DatasetPackageMetadata {
            tuning_ready: false,
            splits: DatasetSplits::default(),
            warnings: vec!["single-file dataset import did not contain a root JSONL file".into()],
        });
    };

    let mut warnings = Vec::new();
    if file_name != TRAIN_FILE {
        warnings.push(format!(
            "single JSONL import `{file_name}` is treated as the train split; use `train.jsonl` in a dataset directory for canonical tuning packages"
        ));
    }

    Ok(DatasetPackageMetadata {
        tuning_ready: true,
        splits: DatasetSplits {
            train: Some(file_name),
            ..DatasetSplits::default()
        },
        warnings,
    })
}

fn detect_directory_package(source_root: &Path) -> Result<DatasetPackageMetadata, DatasetError> {
    let mut splits = DatasetSplits {
        train: root_file(source_root, TRAIN_FILE),
        validation: root_file(source_root, VALID_FILE),
        test: root_file(source_root, TEST_FILE),
        eval_cases: root_file(source_root, EVAL_CASES_FILE),
        source_manifest: root_file(source_root, SOURCE_MANIFEST_FILE),
    };
    let mut warnings = Vec::new();

    let legacy_val = root_file(source_root, LEGACY_VAL_FILE);
    match (splits.validation.as_deref(), legacy_val.as_deref()) {
        (None, Some(val_path)) => {
            splits.validation = Some(val_path.to_string());
            warnings.push(
                "`val.jsonl` detected; treating it as validation, but `valid.jsonl` is the canonical MLX-compatible name".into(),
            );
        }
        (Some(_), Some(_)) if files_match(source_root, VALID_FILE, LEGACY_VAL_FILE)? => {
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

fn first_root_jsonl_file(source_root: &Path) -> Result<Option<String>, DatasetError> {
    for entry in std::fs::read_dir(source_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }

        let path = entry.path();
        let is_jsonl = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"));
        if is_jsonl {
            return Ok(entry.file_name().to_str().map(ToOwned::to_owned));
        }
    }

    Ok(None)
}

fn root_file(source_root: &Path, file_name: &str) -> Option<String> {
    source_root
        .join(file_name)
        .is_file()
        .then(|| file_name.to_string())
}

fn files_match(source_root: &Path, left: &str, right: &str) -> Result<bool, DatasetError> {
    Ok(hash::sha256_file(&source_root.join(left))? == hash::sha256_file(&source_root.join(right))?)
}
