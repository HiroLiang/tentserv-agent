use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;
use walkdir::WalkDir;

use crate::auth::{AuthManager, Provider};

use super::{
    error::ModelError,
    format::{detect_formats, select_primary_format},
    hash, index,
    manifest::build_manifest,
    store::{
        imported_at_now, read_model_metadata, write_model_metadata, write_variant_metadata,
        ImportMethod, ModelMetadata, ModelStorePaths, SourceKind, VariantMetadata,
    },
};

#[derive(Debug, Clone)]
pub struct ImportOutcome {
    pub metadata: ModelMetadata,
    pub store_path: PathBuf,
    pub source_index_path: PathBuf,
    pub deduplicated: bool,
}

#[derive(Debug, Clone)]
pub struct ModelSummary {
    pub metadata: ModelMetadata,
    pub store_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ModelInspection {
    pub metadata: ModelMetadata,
    pub store_path: PathBuf,
    pub manifest_path: PathBuf,
    pub variant_source_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct RemovalOutcome {
    pub metadata: ModelMetadata,
    pub store_path: PathBuf,
    pub removed_index_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ModelManager {
    auth: AuthManager,
    paths: ModelStorePaths,
}

#[derive(Debug, Clone)]
enum ImportSource {
    Local {
        original_path: PathBuf,
    },
    HuggingFace {
        repo_id: String,
        resolved_revision: String,
    },
}

#[derive(Debug, Deserialize)]
struct HfSnapshotOutput {
    repo_id: String,
    resolved_revision: String,
    local_dir: String,
}

impl ModelManager {
    pub fn new() -> Result<Self, ModelError> {
        let paths = ModelStorePaths::resolve()?;
        paths.ensure_layout()?;

        Ok(Self {
            auth: AuthManager::new()?,
            paths,
        })
    }

    pub fn add_path(&self, input_path: impl AsRef<Path>) -> Result<ImportOutcome, ModelError> {
        let input_path = input_path.as_ref();
        if !input_path.exists() {
            return Err(ModelError::MissingPath(input_path.to_path_buf()));
        }

        if !input_path.is_file() && !input_path.is_dir() {
            return Err(ModelError::UnsupportedPath(input_path.to_path_buf()));
        }

        let stage_root = self.create_staging_root("add")?;
        let staged_source_dir = stage_root.join("source");
        fs::create_dir_all(&staged_source_dir)?;

        copy_into_source_root(input_path, &staged_source_dir)?;

        self.finalize_import(
            stage_root,
            ImportSource::Local {
                original_path: input_path.to_path_buf(),
            },
            ImportMethod::Add,
        )
    }

    pub fn pull_hf(
        &self,
        repo_id: &str,
        revision: Option<&str>,
    ) -> Result<ImportOutcome, ModelError> {
        let stage_root = self.create_staging_root("pull")?;
        let staged_source_dir = stage_root.join("source");
        fs::create_dir_all(&staged_source_dir)?;

        let hf_output = self.run_hf_snapshot(repo_id, revision, &staged_source_dir)?;
        let resolved_source_dir = PathBuf::from(&hf_output.local_dir);
        if resolved_source_dir != staged_source_dir {
            return Err(ModelError::HfHelperOutput {
                message: format!(
                    "helper downloaded to `{}` instead of the expected staging directory `{}`",
                    resolved_source_dir.display(),
                    staged_source_dir.display()
                ),
            });
        }

        self.finalize_import(
            stage_root,
            ImportSource::HuggingFace {
                repo_id: hf_output.repo_id,
                resolved_revision: hf_output.resolved_revision,
            },
            ImportMethod::Pull,
        )
    }

    pub fn list_models(&self) -> Result<Vec<ModelSummary>, ModelError> {
        let mut models = Vec::new();

        if !self.paths.store_dir.exists() {
            return Ok(models);
        }

        for entry in fs::read_dir(&self.paths.store_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let model_ref = entry.file_name().to_string_lossy().into_owned();
            let metadata = read_model_metadata(&self.paths.model_toml_path(&model_ref))?;
            models.push(ModelSummary {
                metadata,
                store_path: self.paths.model_dir(&model_ref),
            });
        }

        models.sort_by(|left, right| left.metadata.short_ref.cmp(&right.metadata.short_ref));
        Ok(models)
    }

    pub fn inspect(&self, reference: &str) -> Result<ModelInspection, ModelError> {
        let metadata = self.resolve_metadata(reference)?;
        let store_path = self.paths.model_dir(&metadata.model_ref);

        Ok(ModelInspection {
            manifest_path: self.paths.manifest_path(&metadata.model_ref),
            variant_source_path: self
                .paths
                .variant_source_dir(&metadata.model_ref, metadata.primary_format),
            store_path,
            metadata,
        })
    }

    pub fn remove(&self, reference: &str) -> Result<RemovalOutcome, ModelError> {
        let metadata = self.resolve_metadata(reference)?;
        let store_path = self.paths.model_dir(&metadata.model_ref);

        let removed_index_paths =
            index::remove_indexes_for_model_ref(&self.paths, &metadata.model_ref)?;
        if store_path.exists() {
            fs::remove_dir_all(&store_path)?;
        }

        Ok(RemovalOutcome {
            metadata,
            store_path,
            removed_index_paths,
        })
    }

    fn finalize_import(
        &self,
        stage_root: PathBuf,
        source: ImportSource,
        import_method: ImportMethod,
    ) -> Result<ImportOutcome, ModelError> {
        let staged_source_dir = stage_root.join("source");
        let manifest = build_manifest(&staged_source_dir)?;
        let canonical_manifest = manifest.canonical_json_bytes()?;
        let model_ref = hash::sha256_bytes(&canonical_manifest);
        let short_ref = model_ref.chars().take(12).collect::<String>();
        let source_repo = source.repo_id();
        let detected_formats = detect_formats(&manifest, source_repo.as_deref());
        let primary_format = select_primary_format(&detected_formats, source_repo.as_deref())?;
        let store_path = self.paths.model_dir(&model_ref);

        if store_path.exists() {
            let metadata = read_model_metadata(&self.paths.model_toml_path(&model_ref))?;
            let source_index_path = self.write_source_index(&metadata, &source)?;
            let _ = fs::remove_dir_all(&stage_root);

            return Ok(ImportOutcome {
                metadata,
                store_path,
                source_index_path,
                deduplicated: true,
            });
        }

        fs::create_dir_all(self.paths.variant_dir(&model_ref, primary_format))?;
        fs::rename(
            &staged_source_dir,
            self.paths.variant_source_dir(&model_ref, primary_format),
        )?;

        let metadata = ModelMetadata {
            model_ref: model_ref.clone(),
            short_ref,
            source_kind: source.kind(),
            source_repo: source_repo,
            source_revision: source.resolved_revision(),
            source_path: source
                .original_path()
                .map(|path| path.display().to_string()),
            primary_format,
            detected_formats,
            file_count: manifest.file_count(),
            total_bytes: manifest.total_bytes(),
            imported_at: imported_at_now()?,
        };

        let variant = VariantMetadata {
            format: primary_format,
            status: "imported".to_string(),
            import_method,
            relative_source_path: "source".to_string(),
        };

        write_model_metadata(&self.paths.model_toml_path(&metadata.model_ref), &metadata)?;
        fs::write(
            self.paths.manifest_path(&metadata.model_ref),
            manifest.pretty_json_bytes()?,
        )?;
        write_variant_metadata(
            &self
                .paths
                .variant_toml_path(&metadata.model_ref, metadata.primary_format),
            &variant,
        )?;

        let source_index_path = self.write_source_index(&metadata, &source)?;

        let _ = fs::remove_dir_all(&stage_root);

        Ok(ImportOutcome {
            metadata,
            store_path: self.paths.model_dir(&model_ref),
            source_index_path,
            deduplicated: false,
        })
    }

    fn write_source_index(
        &self,
        metadata: &ModelMetadata,
        source: &ImportSource,
    ) -> Result<PathBuf, ModelError> {
        match source {
            ImportSource::Local { original_path } => {
                index::write_local_index(&self.paths, metadata, original_path)
            }
            ImportSource::HuggingFace {
                repo_id,
                resolved_revision,
            } => index::write_hf_index(&self.paths, metadata, repo_id, resolved_revision),
        }
    }

    fn resolve_metadata(&self, reference: &str) -> Result<ModelMetadata, ModelError> {
        let exact_path = self.paths.model_toml_path(reference);
        if exact_path.exists() {
            return read_model_metadata(&exact_path);
        }

        let mut matches = Vec::new();
        for entry in fs::read_dir(&self.paths.store_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let model_ref = entry.file_name().to_string_lossy().into_owned();
            if model_ref.starts_with(reference) {
                matches.push(read_model_metadata(
                    &self.paths.model_toml_path(&model_ref),
                )?);
            }
        }

        match matches.len() {
            0 => Err(ModelError::NotFound(reference.to_string())),
            1 => Ok(matches.remove(0)),
            _ => Err(ModelError::AmbiguousRef(reference.to_string())),
        }
    }

    fn create_staging_root(&self, prefix: &str) -> Result<PathBuf, ModelError> {
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

    fn run_hf_snapshot(
        &self,
        repo_id: &str,
        revision: Option<&str>,
        staged_source_dir: &Path,
    ) -> Result<HfSnapshotOutput, ModelError> {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let python_project = repo_root.join("python/tentgent-daemon");
        let helper = python_project.join("pyproject.toml");
        let result_path = staged_source_dir
            .parent()
            .unwrap_or(staged_source_dir)
            .join("hf_snapshot_result.json");
        if !helper.exists() {
            return Err(ModelError::MissingHelper { path: helper });
        }

        let mut command = Command::new("uv");
        command
            .current_dir(&python_project)
            .arg("run")
            .arg("tentgent-hf-snapshot")
            .arg("--repo-id")
            .arg(repo_id)
            .arg("--local-dir")
            .arg(staged_source_dir)
            .arg("--result-path")
            .arg(&result_path);
        command.env_remove("VIRTUAL_ENV");

        if let Some(revision) = revision {
            command.arg("--revision").arg(revision);
        }

        if let Some((_, secret)) = self.auth.effective_secret(Provider::HuggingFace)? {
            command.env(Provider::HuggingFace.env_var(), secret);
        }

        let status = command.status()?;
        if !status.success() {
            return Err(ModelError::HfHelper {
                message: format!("helper exited with status {status}"),
            });
        }

        let result_body = fs::read_to_string(&result_path)?;

        serde_json::from_str::<HfSnapshotOutput>(&result_body).map_err(|err| {
            ModelError::HfHelperOutput {
                message: format!("{}; result file was `{}`", err, result_body.trim()),
            }
        })
    }
}

impl ImportSource {
    fn kind(&self) -> SourceKind {
        match self {
            Self::Local { .. } => SourceKind::Local,
            Self::HuggingFace { .. } => SourceKind::HuggingFace,
        }
    }

    fn repo_id(&self) -> Option<String> {
        match self {
            Self::Local { .. } => None,
            Self::HuggingFace { repo_id, .. } => Some(repo_id.clone()),
        }
    }

    fn resolved_revision(&self) -> Option<String> {
        match self {
            Self::Local { .. } => None,
            Self::HuggingFace {
                resolved_revision, ..
            } => Some(resolved_revision.clone()),
        }
    }

    fn original_path(&self) -> Option<&Path> {
        match self {
            Self::Local { original_path } => Some(original_path.as_path()),
            Self::HuggingFace { .. } => None,
        }
    }
}

fn copy_into_source_root(input_path: &Path, source_root: &Path) -> Result<(), ModelError> {
    if input_path.is_file() {
        let file_name = input_path
            .file_name()
            .ok_or_else(|| ModelError::UnsupportedPath(input_path.to_path_buf()))?;
        fs::copy(input_path, source_root.join(file_name))?;
        return Ok(());
    }

    for entry in WalkDir::new(input_path) {
        let entry = entry.map_err(|err| ModelError::Walk {
            path: input_path.to_path_buf(),
            message: err.to_string(),
        })?;

        let path = entry.path();
        let relative = path
            .strip_prefix(input_path)
            .map_err(|err| ModelError::Walk {
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
