use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use walkdir::WalkDir;

use super::{
    diff::{diff_manifests, DatasetManifestDiff},
    error::DatasetError,
    hash, index,
    manifest::{build_manifest, read_manifest},
    package::detect_dataset_package,
    store::{
        imported_at_now, read_dataset_metadata, write_dataset_metadata, DatasetFormat,
        DatasetMetadata, DatasetSourceKind, DatasetStorePaths,
    },
};

#[derive(Debug, Clone)]
pub struct DatasetImportOutcome {
    pub metadata: DatasetMetadata,
    pub store_path: PathBuf,
    pub source_index_path: PathBuf,
    pub deduplicated: bool,
}

#[derive(Debug, Clone)]
pub struct DatasetSummary {
    pub metadata: DatasetMetadata,
    pub store_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct DatasetInspection {
    pub metadata: DatasetMetadata,
    pub store_path: PathBuf,
    pub manifest_path: PathBuf,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct DatasetExportOutcome {
    pub metadata: DatasetMetadata,
    pub managed_source_path: PathBuf,
    pub destination_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct DatasetDiffOutcome {
    pub left: DatasetDiffSide,
    pub right: DatasetDiffSide,
    pub diff: DatasetManifestDiff,
}

#[derive(Debug, Clone)]
pub struct DatasetDiffSide {
    pub label: String,
    pub short_ref: Option<String>,
    pub tuning_ready: bool,
    pub splits: String,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct DatasetRemovalOutcome {
    pub metadata: DatasetMetadata,
    pub store_path: PathBuf,
    pub removed_index_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct DatasetManager {
    paths: DatasetStorePaths,
}

#[derive(Debug, Clone)]
struct ImportSource {
    original_path: PathBuf,
    dataset_format: DatasetFormat,
}

impl DatasetManager {
    pub fn new() -> Result<Self, DatasetError> {
        let paths = DatasetStorePaths::resolve()?;
        paths.ensure_layout()?;
        Ok(Self { paths })
    }

    pub fn add_path(
        &self,
        input_path: impl AsRef<Path>,
    ) -> Result<DatasetImportOutcome, DatasetError> {
        let input_path = input_path.as_ref();
        if !input_path.exists() {
            return Err(DatasetError::MissingPath(input_path.to_path_buf()));
        }

        let dataset_format = detect_dataset_format(input_path)?;
        let stage_root = self.create_staging_root("add")?;
        let staged_source_dir = stage_root.join("source");
        fs::create_dir_all(&staged_source_dir)?;

        copy_source_into(input_path, &staged_source_dir, dataset_format)?;

        self.finalize_import(
            stage_root,
            ImportSource {
                original_path: input_path.to_path_buf(),
                dataset_format,
            },
        )
    }

    pub fn list_datasets(&self) -> Result<Vec<DatasetSummary>, DatasetError> {
        let mut datasets = Vec::new();

        if !self.paths.store_dir.exists() {
            return Ok(datasets);
        }

        for entry in fs::read_dir(&self.paths.store_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let dataset_ref = entry.file_name().to_string_lossy().into_owned();
            let metadata = read_dataset_metadata(&self.paths.dataset_toml_path(&dataset_ref))?;
            datasets.push(DatasetSummary {
                metadata,
                store_path: self.paths.dataset_dir(&dataset_ref),
            });
        }

        datasets.sort_by(|left, right| left.metadata.short_ref.cmp(&right.metadata.short_ref));
        Ok(datasets)
    }

    pub fn inspect(&self, reference: &str) -> Result<DatasetInspection, DatasetError> {
        let metadata = self.resolve_metadata(reference)?;
        let store_path = self.paths.dataset_dir(&metadata.dataset_ref);

        Ok(DatasetInspection {
            manifest_path: self.paths.manifest_path(&metadata.dataset_ref),
            source_path: self.paths.source_dir(&metadata.dataset_ref),
            store_path,
            metadata,
        })
    }

    pub fn export_to(
        &self,
        reference: &str,
        destination: impl AsRef<Path>,
    ) -> Result<DatasetExportOutcome, DatasetError> {
        let metadata = self.resolve_metadata(reference)?;
        let managed_source_path = self.paths.source_dir(&metadata.dataset_ref);
        let destination_path = destination.as_ref().to_path_buf();

        ensure_export_destination(&destination_path)?;
        fs::create_dir_all(&destination_path)?;
        copy_dir_contents(&managed_source_path, &destination_path)?;

        Ok(DatasetExportOutcome {
            metadata,
            managed_source_path,
            destination_path,
        })
    }

    pub fn diff_refs(
        &self,
        left_reference: &str,
        right_reference: &str,
    ) -> Result<DatasetDiffOutcome, DatasetError> {
        let left = self.resolve_metadata(left_reference)?;
        let right = self.resolve_metadata(right_reference)?;
        let left_manifest = read_manifest(&self.paths.manifest_path(&left.dataset_ref))?;
        let right_manifest = read_manifest(&self.paths.manifest_path(&right.dataset_ref))?;

        Ok(DatasetDiffOutcome {
            left: diff_side_for_metadata(left),
            right: diff_side_for_metadata(right),
            diff: diff_manifests(&left_manifest, &right_manifest),
        })
    }

    pub fn diff_ref_to_path(
        &self,
        left_reference: &str,
        right_path: impl AsRef<Path>,
    ) -> Result<DatasetDiffOutcome, DatasetError> {
        let left = self.resolve_metadata(left_reference)?;
        let left_manifest = read_manifest(&self.paths.manifest_path(&left.dataset_ref))?;
        let right_path = right_path.as_ref();
        if !right_path.exists() {
            return Err(DatasetError::MissingPath(right_path.to_path_buf()));
        }

        let right_format = detect_dataset_format(right_path)?;
        let stage_root = self.create_staging_root("diff")?;
        let staged_source_dir = stage_root.join("source");
        fs::create_dir_all(&staged_source_dir)?;
        copy_source_into(right_path, &staged_source_dir, right_format)?;

        let right_manifest = build_manifest(&staged_source_dir)?;
        let right_package = detect_dataset_package(&staged_source_dir, right_format)?;
        let _ = fs::remove_dir_all(&stage_root);

        Ok(DatasetDiffOutcome {
            left: diff_side_for_metadata(left),
            right: DatasetDiffSide {
                label: right_path.display().to_string(),
                short_ref: None,
                tuning_ready: right_package.tuning_ready,
                splits: split_summary_for_package(&right_package),
                path: Some(right_path.to_path_buf()),
            },
            diff: diff_manifests(&left_manifest, &right_manifest),
        })
    }

    pub fn remove(&self, reference: &str) -> Result<DatasetRemovalOutcome, DatasetError> {
        let metadata = self.resolve_metadata(reference)?;
        let store_path = self.paths.dataset_dir(&metadata.dataset_ref);
        let removed_index_paths =
            index::remove_indexes_for_dataset_ref(&self.paths, &metadata.dataset_ref)?;

        if store_path.exists() {
            fs::remove_dir_all(&store_path)?;
        }

        Ok(DatasetRemovalOutcome {
            metadata,
            store_path,
            removed_index_paths,
        })
    }

    fn finalize_import(
        &self,
        stage_root: PathBuf,
        source: ImportSource,
    ) -> Result<DatasetImportOutcome, DatasetError> {
        let staged_source_dir = stage_root.join("source");
        let manifest = build_manifest(&staged_source_dir)?;
        let package = detect_dataset_package(&staged_source_dir, source.dataset_format)?;
        let canonical_manifest = manifest.canonical_json_bytes()?;
        let dataset_ref = hash::sha256_bytes(&canonical_manifest);
        let short_ref = dataset_ref.chars().take(12).collect::<String>();
        let store_path = self.paths.dataset_dir(&dataset_ref);

        if store_path.exists() {
            let metadata = read_dataset_metadata(&self.paths.dataset_toml_path(&dataset_ref))?;
            let source_index_path =
                index::write_local_index(&self.paths, &metadata, &source.original_path)?;
            let _ = fs::remove_dir_all(&stage_root);

            return Ok(DatasetImportOutcome {
                metadata,
                store_path,
                source_index_path,
                deduplicated: true,
            });
        }

        fs::create_dir_all(&store_path)?;
        fs::rename(&staged_source_dir, self.paths.source_dir(&dataset_ref))?;

        let metadata = DatasetMetadata {
            dataset_ref: dataset_ref.clone(),
            short_ref,
            source_kind: DatasetSourceKind::Local,
            source_path: Some(source.original_path.display().to_string()),
            source_repo: None,
            source_revision: None,
            dataset_format: source.dataset_format,
            file_count: manifest.file_count(),
            total_bytes: manifest.total_bytes(),
            imported_at: imported_at_now()?,
            package,
        };

        write_dataset_metadata(&self.paths.dataset_toml_path(&dataset_ref), &metadata)?;
        fs::write(
            self.paths.manifest_path(&dataset_ref),
            manifest.pretty_json_bytes()?,
        )?;
        let source_index_path =
            index::write_local_index(&self.paths, &metadata, &source.original_path)?;
        let _ = fs::remove_dir_all(&stage_root);

        Ok(DatasetImportOutcome {
            metadata,
            store_path,
            source_index_path,
            deduplicated: false,
        })
    }

    fn create_staging_root(&self, prefix: &str) -> Result<PathBuf, DatasetError> {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        let stage_root = self
            .paths
            .staging_dir
            .join(format!("{prefix}-{millis}-{}", std::process::id()));
        fs::create_dir_all(&stage_root)?;
        Ok(stage_root)
    }

    fn resolve_metadata(&self, reference: &str) -> Result<DatasetMetadata, DatasetError> {
        let exact_path = self.paths.dataset_toml_path(reference);
        if exact_path.exists() {
            return read_dataset_metadata(&exact_path);
        }

        let mut matches = Vec::new();
        for entry in fs::read_dir(&self.paths.store_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let dataset_ref = entry.file_name().to_string_lossy().into_owned();
            if dataset_ref.starts_with(reference) {
                matches.push(read_dataset_metadata(
                    &self.paths.dataset_toml_path(&dataset_ref),
                )?);
            }
        }

        match matches.len() {
            0 => Err(DatasetError::NotFound(reference.to_string())),
            1 => Ok(matches.remove(0)),
            _ => Err(DatasetError::AmbiguousRef(reference.to_string())),
        }
    }
}

fn detect_dataset_format(path: &Path) -> Result<DatasetFormat, DatasetError> {
    if path.is_dir() {
        return Ok(DatasetFormat::Directory);
    }

    if path.is_file()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
    {
        return Ok(DatasetFormat::Jsonl);
    }

    Err(DatasetError::UnsupportedLayout {
        reason: "expected a .jsonl file or a directory containing dataset files".to_string(),
    })
}

fn diff_side_for_metadata(metadata: DatasetMetadata) -> DatasetDiffSide {
    DatasetDiffSide {
        label: metadata.short_ref.clone(),
        short_ref: Some(metadata.short_ref),
        tuning_ready: metadata.package.tuning_ready,
        splits: split_summary_for_package(&metadata.package),
        path: None,
    }
}

fn split_summary_for_package(package: &super::store::DatasetPackageMetadata) -> String {
    let mut names = Vec::new();
    if package.splits.train.is_some() {
        names.push("train");
    }
    if package.splits.validation.is_some() {
        names.push("valid");
    }
    if package.splits.test.is_some() {
        names.push("test");
    }
    if package.splits.eval_cases.is_some() {
        names.push("eval");
    }

    if names.is_empty() {
        "-".to_string()
    } else {
        names.join(",")
    }
}

fn copy_source_into(
    input_path: &Path,
    source_root: &Path,
    dataset_format: DatasetFormat,
) -> Result<(), DatasetError> {
    match dataset_format {
        DatasetFormat::Jsonl => {
            let file_name =
                input_path
                    .file_name()
                    .ok_or_else(|| DatasetError::UnsupportedLayout {
                        reason: format!("dataset file `{}` has no file name", input_path.display()),
                    })?;
            fs::copy(input_path, source_root.join(file_name))?;
            Ok(())
        }
        DatasetFormat::Directory => copy_dir_contents(input_path, source_root),
    }
}

fn copy_dir_contents(input_path: &Path, source_root: &Path) -> Result<(), DatasetError> {
    for entry in WalkDir::new(input_path) {
        let entry = entry.map_err(|err| DatasetError::Walk {
            path: input_path.to_path_buf(),
            message: err.to_string(),
        })?;

        let path = entry.path();
        let relative = path
            .strip_prefix(input_path)
            .map_err(|err| DatasetError::Walk {
                path: path.to_path_buf(),
                message: err.to_string(),
            })?;

        if relative.as_os_str().is_empty() {
            continue;
        }

        let destination = source_root.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(path, &destination)?;
        }
    }

    Ok(())
}

fn ensure_export_destination(destination: &Path) -> Result<(), DatasetError> {
    if !destination.exists() {
        return Ok(());
    }

    if !destination.is_dir() {
        return Err(DatasetError::ExportDestinationNotDirectory(
            destination.to_path_buf(),
        ));
    }

    if std::fs::read_dir(destination)?.next().is_some() {
        return Err(DatasetError::ExportDestinationNotEmpty(
            destination.to_path_buf(),
        ));
    }

    Ok(())
}
