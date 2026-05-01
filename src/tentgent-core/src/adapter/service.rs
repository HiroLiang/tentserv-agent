use std::{
    env, fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;
use walkdir::WalkDir;

use crate::{
    auth::{AuthManager, Provider},
    model::{ModelManager, ModelMetadata},
    runtime_assets::{PythonRuntime, PythonRuntimeSource},
};

use super::{
    error::AdapterError,
    hash, index,
    manifest::{build_manifest, ManifestDocument},
    store::{
        imported_at_now, read_adapter_metadata, write_adapter_metadata, AdapterFormat,
        AdapterMetadata, AdapterSourceKind, AdapterStorePaths, AdapterType,
    },
};

#[derive(Debug, Clone)]
pub struct AdapterImportOutcome {
    pub metadata: AdapterMetadata,
    pub store_path: PathBuf,
    pub source_index_path: PathBuf,
    pub base_index_path: Option<PathBuf>,
    pub deduplicated: bool,
}

#[derive(Debug, Clone)]
pub struct HfPullProgress {
    pub description: String,
    pub position: u64,
    pub total: Option<u64>,
    pub unit: String,
    pub finished: bool,
}

#[derive(Debug, Clone)]
pub struct AdapterSummary {
    pub metadata: AdapterMetadata,
    pub store_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct AdapterInspection {
    pub metadata: AdapterMetadata,
    pub store_path: PathBuf,
    pub manifest_path: PathBuf,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct AdapterBindOutcome {
    pub metadata: AdapterMetadata,
    pub store_path: PathBuf,
    pub base_index_path: PathBuf,
    pub removed_base_index_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct AdapterRemovalOutcome {
    pub metadata: AdapterMetadata,
    pub store_path: PathBuf,
    pub removed_index_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct AdapterManager {
    auth: AuthManager,
    paths: AdapterStorePaths,
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
    TrainRun {
        output_path: PathBuf,
        run_ref: String,
        dataset_ref: String,
        config_ref: String,
    },
}

#[derive(Debug, Deserialize)]
struct HfSnapshotOutput {
    repo_id: String,
    resolved_revision: String,
    local_dir: String,
}

#[derive(Debug, Deserialize)]
struct HfProgressLine {
    event: String,
    kind: String,
    #[serde(default)]
    desc: Option<String>,
    #[serde(default)]
    position: Option<f64>,
    #[serde(default)]
    total: Option<f64>,
    #[serde(default)]
    unit: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StoredServerAdapterRefs {
    #[serde(default)]
    short_ref: String,
    #[serde(default)]
    adapter_ref: Option<String>,
    #[serde(default)]
    default_adapter_ref: Option<String>,
    #[serde(default)]
    allowed_adapters: Vec<String>,
    #[serde(default)]
    adapter_refs: Vec<String>,
}

impl AdapterManager {
    pub fn new() -> Result<Self, AdapterError> {
        let paths = AdapterStorePaths::resolve()?;
        paths.ensure_layout()?;
        Ok(Self {
            auth: AuthManager::new()?,
            paths,
        })
    }

    pub fn new_with_home(home_override: Option<&Path>) -> Result<Self, AdapterError> {
        let paths = AdapterStorePaths::resolve_with_home(home_override)?;
        paths.ensure_layout()?;
        Ok(Self {
            auth: AuthManager::new()?,
            paths,
        })
    }

    pub fn open_readonly_with_home(home_override: Option<&Path>) -> Result<Self, AdapterError> {
        let paths = AdapterStorePaths::resolve_with_home(home_override)?;
        Ok(Self {
            auth: AuthManager::new()?,
            paths,
        })
    }

    pub fn add_path(
        &self,
        input_path: impl AsRef<Path>,
        base_model_ref: Option<&str>,
    ) -> Result<AdapterImportOutcome, AdapterError> {
        let input_path = input_path.as_ref();
        if !input_path.exists() {
            return Err(AdapterError::MissingPath(input_path.to_path_buf()));
        }

        if !input_path.is_dir() {
            return Err(AdapterError::UnsupportedPath(input_path.to_path_buf()));
        }

        let stage_root = self.create_staging_root("add")?;
        let staged_source_dir = stage_root.join("source");
        fs::create_dir_all(&staged_source_dir)?;

        copy_dir_contents(input_path, &staged_source_dir)?;
        self.finalize_import(
            stage_root,
            ImportSource::Local {
                original_path: input_path.to_path_buf(),
            },
            base_model_ref,
        )
    }

    pub fn add_train_run_output(
        &self,
        output_path: impl AsRef<Path>,
        base_model_ref: &str,
        dataset_ref: &str,
        run_ref: &str,
        config_ref: &str,
    ) -> Result<AdapterImportOutcome, AdapterError> {
        let output_path = output_path.as_ref();
        if !output_path.exists() {
            return Err(AdapterError::MissingPath(output_path.to_path_buf()));
        }

        if !output_path.is_dir() {
            return Err(AdapterError::UnsupportedPath(output_path.to_path_buf()));
        }

        let stage_root = self.create_staging_root("train-run")?;
        let staged_source_dir = stage_root.join("source");
        fs::create_dir_all(&staged_source_dir)?;

        copy_dir_contents(output_path, &staged_source_dir)?;
        self.finalize_import(
            stage_root,
            ImportSource::TrainRun {
                output_path: output_path.to_path_buf(),
                run_ref: run_ref.to_string(),
                dataset_ref: dataset_ref.to_string(),
                config_ref: config_ref.to_string(),
            },
            Some(base_model_ref),
        )
    }

    pub fn pull_hf(
        &self,
        repo_id: &str,
        revision: Option<&str>,
        base_model_ref: Option<&str>,
    ) -> Result<AdapterImportOutcome, AdapterError> {
        self.pull_hf_with_progress(repo_id, revision, base_model_ref, |_| {})
    }

    pub fn pull_hf_with_progress(
        &self,
        repo_id: &str,
        revision: Option<&str>,
        base_model_ref: Option<&str>,
        mut progress: impl FnMut(HfPullProgress),
    ) -> Result<AdapterImportOutcome, AdapterError> {
        let stage_root = self.create_staging_root("pull")?;
        let staged_source_dir = stage_root.join("source");
        fs::create_dir_all(&staged_source_dir)?;

        let hf_output =
            self.run_hf_snapshot(repo_id, revision, &staged_source_dir, &mut progress)?;
        let resolved_source_dir = PathBuf::from(&hf_output.local_dir);
        let expected_source_dir = staged_source_dir
            .canonicalize()
            .unwrap_or_else(|_| staged_source_dir.clone());
        if resolved_source_dir != expected_source_dir {
            return Err(AdapterError::HfHelperOutput {
                message: format!(
                    "helper downloaded to `{}` instead of the expected staging directory `{}`",
                    resolved_source_dir.display(),
                    expected_source_dir.display()
                ),
            });
        }

        self.finalize_import(
            stage_root,
            ImportSource::HuggingFace {
                repo_id: hf_output.repo_id,
                resolved_revision: hf_output.resolved_revision,
            },
            base_model_ref,
        )
    }

    pub fn list_adapters(&self) -> Result<Vec<AdapterSummary>, AdapterError> {
        let mut adapters = Vec::new();

        if !self.paths.store_dir.exists() {
            return Ok(adapters);
        }

        for entry in fs::read_dir(&self.paths.store_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let adapter_ref = entry.file_name().to_string_lossy().into_owned();
            let metadata = read_adapter_metadata(&self.paths.adapter_toml_path(&adapter_ref))?;
            adapters.push(AdapterSummary {
                metadata,
                store_path: self.paths.adapter_dir(&adapter_ref),
            });
        }

        adapters.sort_by(|left, right| left.metadata.short_ref.cmp(&right.metadata.short_ref));
        Ok(adapters)
    }

    pub fn inspect(&self, reference: &str) -> Result<AdapterInspection, AdapterError> {
        let metadata = self.resolve_metadata(reference)?;
        let store_path = self.paths.adapter_dir(&metadata.adapter_ref);

        Ok(AdapterInspection {
            manifest_path: self.paths.manifest_path(&metadata.adapter_ref),
            source_path: self.paths.source_dir(&metadata.adapter_ref),
            store_path,
            metadata,
        })
    }

    pub fn bind_to_model(
        &self,
        adapter_reference: &str,
        base_model_reference: &str,
    ) -> Result<AdapterBindOutcome, AdapterError> {
        let mut metadata = self.resolve_metadata(adapter_reference)?;
        let store_path = self.paths.adapter_dir(&metadata.adapter_ref);
        let previous_base_model_ref = metadata.base_model_ref.clone();
        let config = read_adapter_config(&self.paths.source_dir(&metadata.adapter_ref))?;
        let base_model = resolve_base_model(Some(base_model_reference))?
            .expect("base model reference is always provided");

        validate_base_compatibility(config.as_ref(), Some(&base_model))?;
        apply_base_metadata(&mut metadata, config.as_ref(), Some(&base_model));
        write_adapter_metadata(
            &self.paths.adapter_toml_path(&metadata.adapter_ref),
            &metadata,
        )?;

        let removed_base_index_path = match (
            previous_base_model_ref.as_deref(),
            metadata.base_model_ref.as_deref(),
        ) {
            (Some(previous), Some(current)) if previous != current => {
                index::remove_base_index(&self.paths, &metadata.adapter_ref, previous)?
            }
            _ => None,
        };

        let base_index_path = write_base_index_if_needed(&self.paths, &metadata)?
            .expect("bound adapter metadata always includes base_model_ref");

        Ok(AdapterBindOutcome {
            metadata,
            store_path,
            base_index_path,
            removed_base_index_path,
        })
    }

    pub fn remove(&self, reference: &str) -> Result<AdapterRemovalOutcome, AdapterError> {
        let metadata = self.resolve_metadata(reference)?;
        self.ensure_not_referenced_by_server(&metadata.adapter_ref)?;
        let store_path = self.paths.adapter_dir(&metadata.adapter_ref);
        let removed_index_paths = index::remove_indexes_for_adapter(&self.paths, &metadata)?;

        if store_path.exists() {
            fs::remove_dir_all(&store_path)?;
        }

        Ok(AdapterRemovalOutcome {
            metadata,
            store_path,
            removed_index_paths,
        })
    }

    fn finalize_import(
        &self,
        stage_root: PathBuf,
        source: ImportSource,
        base_model_ref: Option<&str>,
    ) -> Result<AdapterImportOutcome, AdapterError> {
        let staged_source_dir = stage_root.join("source");
        let manifest = build_manifest(&staged_source_dir)?;
        let adapter_format = detect_adapter_format(&manifest)?;
        let config = read_adapter_config(&staged_source_dir)?;
        let base_model = resolve_base_model(base_model_ref)?;
        validate_base_compatibility(config.as_ref(), base_model.as_ref())?;
        let canonical_manifest = manifest.canonical_json_bytes()?;
        let adapter_ref = hash::sha256_bytes(&canonical_manifest);
        let short_ref = adapter_ref.chars().take(12).collect::<String>();
        let store_path = self.paths.adapter_dir(&adapter_ref);

        if store_path.exists() {
            let mut metadata = read_adapter_metadata(&self.paths.adapter_toml_path(&adapter_ref))?;
            apply_base_metadata(&mut metadata, config.as_ref(), base_model.as_ref());
            apply_training_metadata(&mut metadata, &source);
            write_adapter_metadata(&self.paths.adapter_toml_path(&adapter_ref), &metadata)?;
            let source_index_path = self.write_source_index(&metadata, &source)?;
            let base_index_path = write_base_index_if_needed(&self.paths, &metadata)?;
            let _ = fs::remove_dir_all(&stage_root);

            return Ok(AdapterImportOutcome {
                metadata,
                store_path,
                source_index_path,
                base_index_path,
                deduplicated: true,
            });
        }

        fs::create_dir_all(&store_path)?;
        fs::rename(&staged_source_dir, self.paths.source_dir(&adapter_ref))?;

        let mut metadata = AdapterMetadata {
            adapter_ref: adapter_ref.clone(),
            short_ref,
            adapter_format,
            adapter_type: AdapterType::Lora,
            base_model_ref: None,
            base_model_source_repo: None,
            base_model_source_revision: None,
            model_family: None,
            backend_support: backend_support(adapter_format),
            source_kind: source.kind(),
            source_repo: source.repo_id(),
            source_revision: source.resolved_revision(),
            source_path: source
                .original_path()
                .map(|path| path.display().to_string()),
            training_dataset_ref: None,
            training_run_ref: None,
            training_config_ref: None,
            file_count: manifest.file_count(),
            total_bytes: manifest.total_bytes(),
            imported_at: imported_at_now()?,
        };
        apply_base_metadata(&mut metadata, config.as_ref(), base_model.as_ref());
        apply_training_metadata(&mut metadata, &source);

        write_adapter_metadata(&self.paths.adapter_toml_path(&adapter_ref), &metadata)?;
        fs::write(
            self.paths.manifest_path(&adapter_ref),
            manifest.pretty_json_bytes()?,
        )?;
        let source_index_path = self.write_source_index(&metadata, &source)?;
        let base_index_path = write_base_index_if_needed(&self.paths, &metadata)?;
        let _ = fs::remove_dir_all(&stage_root);

        Ok(AdapterImportOutcome {
            metadata,
            store_path,
            source_index_path,
            base_index_path,
            deduplicated: false,
        })
    }

    fn create_staging_root(&self, prefix: &str) -> Result<PathBuf, AdapterError> {
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

    fn write_source_index(
        &self,
        metadata: &AdapterMetadata,
        source: &ImportSource,
    ) -> Result<PathBuf, AdapterError> {
        match source {
            ImportSource::Local { original_path } => {
                index::write_local_index(&self.paths, metadata, original_path)
            }
            ImportSource::HuggingFace {
                repo_id,
                resolved_revision,
            } => index::write_hf_index(&self.paths, metadata, repo_id, resolved_revision),
            ImportSource::TrainRun {
                run_ref,
                dataset_ref,
                config_ref,
                ..
            } => index::write_train_run_index(
                &self.paths,
                metadata,
                run_ref,
                dataset_ref,
                config_ref,
            ),
        }
    }

    fn run_hf_snapshot(
        &self,
        repo_id: &str,
        revision: Option<&str>,
        staged_source_dir: &Path,
        progress: &mut impl FnMut(HfPullProgress),
    ) -> Result<HfSnapshotOutput, AdapterError> {
        let python_runtime = PythonRuntime::resolve().map_err(|err| AdapterError::HfHelper {
            message: err.to_string(),
        })?;
        let helper = python_runtime.pyproject_path();
        let result_path = staged_source_dir
            .parent()
            .unwrap_or(staged_source_dir)
            .join("hf_adapter_snapshot_result.json");
        if !helper.exists() {
            return Err(AdapterError::MissingHelper { path: helper });
        }
        let mut command = hf_snapshot_command(&python_runtime)?;
        command
            .arg("--repo-id")
            .arg(repo_id)
            .arg("--local-dir")
            .arg(staged_source_dir)
            .arg("--result-path")
            .arg(&result_path)
            .arg("--progress-json");
        command.env_remove("VIRTUAL_ENV");
        command.env("HF_HUB_DISABLE_PROGRESS_BARS", "1");

        if let Some(revision) = revision {
            command.arg("--revision").arg(revision);
        }

        if let Some((_, secret)) = self.auth.effective_secret(Provider::HuggingFace)? {
            command.env(Provider::HuggingFace.env_var(), secret);
        }

        command.stdout(Stdio::piped());
        let mut child = command.spawn()?;
        if let Some(stdout) = child.stdout.take() {
            for line in BufReader::new(stdout).lines() {
                let line = line?;
                if let Some(event) = parse_hf_progress_line(&line) {
                    progress(event);
                }
            }
        }

        let status = child.wait()?;
        if !status.success() {
            return Err(AdapterError::HfHelper {
                message: format!("helper exited with status {status}"),
            });
        }

        let result_body = fs::read_to_string(&result_path)?;
        serde_json::from_str::<HfSnapshotOutput>(&result_body).map_err(|err| {
            AdapterError::HfHelperOutput {
                message: format!("{}; result file was `{}`", err, result_body.trim()),
            }
        })
    }

    fn resolve_metadata(&self, reference: &str) -> Result<AdapterMetadata, AdapterError> {
        let exact_path = self.paths.adapter_toml_path(reference);
        if exact_path.exists() {
            return read_adapter_metadata(&exact_path);
        }

        let mut matches = Vec::new();
        for entry in fs::read_dir(&self.paths.store_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let adapter_ref = entry.file_name().to_string_lossy().into_owned();
            if adapter_ref.starts_with(reference) {
                matches.push(read_adapter_metadata(
                    &self.paths.adapter_toml_path(&adapter_ref),
                )?);
            }
        }

        match matches.len() {
            0 => Err(AdapterError::NotFound(reference.to_string())),
            1 => Ok(matches.remove(0)),
            _ => Err(AdapterError::AmbiguousRef(reference.to_string())),
        }
    }

    fn ensure_not_referenced_by_server(&self, adapter_ref: &str) -> Result<(), AdapterError> {
        let server_refs = find_server_refs_for_adapter(adapter_ref)?;
        if server_refs.is_empty() {
            return Ok(());
        }

        Err(AdapterError::InUse {
            adapter_ref: adapter_ref.to_string(),
            server_refs: server_refs.join(", "),
        })
    }
}

fn hf_snapshot_command(python_runtime: &PythonRuntime) -> Result<Command, AdapterError> {
    let script = python_runtime.script_bin("tentgent-hf-snapshot");
    if script.exists() {
        let mut command = Command::new(script);
        command.current_dir(python_runtime.project_dir());
        return Ok(command);
    }

    if python_runtime.source() == PythonRuntimeSource::InstalledPrefix {
        return Err(AdapterError::HfHelper {
            message: format!(
                "Hugging Face snapshot helper is missing at `{}`; run the installer Python bootstrap or `tentgent doctor` to repair the managed runtime",
                script.display()
            ),
        });
    }

    if let Some(parent) = python_runtime.env_dir().parent() {
        fs::create_dir_all(parent)?;
    }

    let mut command = Command::new("uv");
    command
        .current_dir(python_runtime.project_dir())
        .arg("--no-config")
        .arg("run")
        .arg("--project")
        .arg(python_runtime.project_dir())
        .arg("tentgent-hf-snapshot");
    python_runtime.configure_uv_command(&mut command);
    Ok(command)
}

fn parse_hf_progress_line(line: &str) -> Option<HfPullProgress> {
    let parsed = serde_json::from_str::<HfProgressLine>(line).ok()?;
    if parsed.event != "progress" {
        return None;
    }

    let position = parsed
        .position
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_default()
        .round() as u64;
    let total = parsed
        .total
        .filter(|value| value.is_finite() && *value > 0.0)
        .map(|value| value.round() as u64);

    Some(HfPullProgress {
        description: parsed.desc.unwrap_or_default(),
        position,
        total,
        unit: parsed.unit.unwrap_or_else(|| "it".to_string()),
        finished: parsed.kind == "close",
    })
}

impl ImportSource {
    fn kind(&self) -> AdapterSourceKind {
        match self {
            Self::Local { .. } => AdapterSourceKind::Local,
            Self::HuggingFace { .. } => AdapterSourceKind::HuggingFace,
            Self::TrainRun { .. } => AdapterSourceKind::TrainRun,
        }
    }

    fn repo_id(&self) -> Option<String> {
        match self {
            Self::Local { .. } => None,
            Self::HuggingFace { repo_id, .. } => Some(repo_id.clone()),
            Self::TrainRun { .. } => None,
        }
    }

    fn resolved_revision(&self) -> Option<String> {
        match self {
            Self::Local { .. } => None,
            Self::HuggingFace {
                resolved_revision, ..
            } => Some(resolved_revision.clone()),
            Self::TrainRun { .. } => None,
        }
    }

    fn original_path(&self) -> Option<&Path> {
        match self {
            Self::Local { original_path } => Some(original_path.as_path()),
            Self::HuggingFace { .. } => None,
            Self::TrainRun { output_path, .. } => Some(output_path.as_path()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct AdapterConfig {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    base_model_name_or_path: Option<String>,
    #[serde(default)]
    revision: Option<String>,
}

impl AdapterConfig {
    fn base_model_source_repo(&self) -> Option<&str> {
        self.base_model_name_or_path
            .as_deref()
            .or(self.model.as_deref())
            .filter(|value| !value.trim().is_empty())
    }

    fn base_model_source_revision(&self) -> Option<&str> {
        self.revision
            .as_deref()
            .filter(|value| !value.trim().is_empty())
    }
}

fn read_adapter_config(source_dir: &Path) -> Result<Option<AdapterConfig>, AdapterError> {
    let path = source_dir.join("adapter_config.json");
    if !path.exists() {
        return Ok(None);
    }

    let body = fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&body)?))
}

fn resolve_base_model(reference: Option<&str>) -> Result<Option<ModelMetadata>, AdapterError> {
    let Some(reference) = reference else {
        return Ok(None);
    };

    let manager = ModelManager::new()?;
    Ok(Some(manager.inspect(reference)?.metadata))
}

fn validate_base_compatibility(
    config: Option<&AdapterConfig>,
    base_model: Option<&ModelMetadata>,
) -> Result<(), AdapterError> {
    let Some(config) = config else {
        return Ok(());
    };
    let Some(base_model) = base_model else {
        return Ok(());
    };

    if let (Some(adapter_base), Some(model_base)) = (
        config.base_model_source_repo(),
        base_model.source_repo.as_deref(),
    ) {
        if !is_local_path_hint(adapter_base) && adapter_base != model_base {
            return Err(AdapterError::BaseModelMismatch {
                adapter_base: adapter_base.to_string(),
                model_base: model_base.to_string(),
            });
        }
    }

    if let (Some(adapter_revision), Some(model_revision)) = (
        config.base_model_source_revision(),
        base_model.source_revision.as_deref(),
    ) {
        if adapter_revision != model_revision {
            return Err(AdapterError::BaseRevisionMismatch {
                adapter_revision: adapter_revision.to_string(),
                model_revision: model_revision.to_string(),
            });
        }
    }

    Ok(())
}

fn apply_base_metadata(
    metadata: &mut AdapterMetadata,
    config: Option<&AdapterConfig>,
    base_model: Option<&ModelMetadata>,
) {
    if let Some(base_model) = base_model {
        metadata.base_model_ref = Some(base_model.model_ref.clone());
        metadata.base_model_source_repo = base_model.source_repo.clone();
        metadata.base_model_source_revision = base_model.source_revision.clone();
    }

    if let Some(config) = config {
        if let Some(repo) = config.base_model_source_repo() {
            if !is_local_path_hint(repo) {
                metadata.base_model_source_repo = Some(repo.to_string());
            }
        }
        if let Some(revision) = config.base_model_source_revision() {
            metadata.base_model_source_revision = Some(revision.to_string());
        }
    }
}

fn apply_training_metadata(metadata: &mut AdapterMetadata, source: &ImportSource) {
    if let ImportSource::TrainRun {
        run_ref,
        dataset_ref,
        config_ref,
        ..
    } = source
    {
        metadata.training_run_ref = Some(run_ref.clone());
        metadata.training_dataset_ref = Some(dataset_ref.clone());
        metadata.training_config_ref = Some(config_ref.clone());
    }
}

fn is_local_path_hint(value: &str) -> bool {
    let trimmed = value.trim();
    Path::new(trimmed).is_absolute() || trimmed.starts_with("./") || trimmed.starts_with("../")
}

fn write_base_index_if_needed(
    paths: &AdapterStorePaths,
    metadata: &AdapterMetadata,
) -> Result<Option<PathBuf>, AdapterError> {
    let Some(base_model_ref) = metadata.base_model_ref.as_deref() else {
        return Ok(None);
    };

    Ok(Some(index::write_base_index(
        paths,
        metadata,
        base_model_ref,
    )?))
}

fn detect_adapter_format(manifest: &ManifestDocument) -> Result<AdapterFormat, AdapterError> {
    if manifest.contains_path("adapter_model.safetensors") {
        return Ok(AdapterFormat::Peft);
    }

    if manifest.contains_path("adapters.safetensors") {
        return Ok(AdapterFormat::Mlx);
    }

    Err(AdapterError::UnsupportedLayout {
        reason: "expected PEFT `adapter_model.safetensors` or MLX `adapters.safetensors`"
            .to_string(),
    })
}

fn backend_support(format: AdapterFormat) -> Vec<String> {
    match format {
        AdapterFormat::Peft => vec!["transformers-peft".to_string()],
        AdapterFormat::Mlx => vec!["mlx".to_string()],
        AdapterFormat::LlamaCpp => vec!["llama-cpp".to_string()],
    }
}

fn copy_dir_contents(input_path: &Path, source_root: &Path) -> Result<(), AdapterError> {
    for entry in WalkDir::new(input_path) {
        let entry = entry.map_err(|err| AdapterError::Walk {
            path: input_path.to_path_buf(),
            message: err.to_string(),
        })?;

        let path = entry.path();
        let relative = path
            .strip_prefix(input_path)
            .map_err(|err| AdapterError::Walk {
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

fn find_server_refs_for_adapter(adapter_ref: &str) -> Result<Vec<String>, AdapterError> {
    let servers_dir = resolve_servers_dir()?;
    if !servers_dir.exists() {
        return Ok(Vec::new());
    }

    let mut server_refs = Vec::new();
    for entry in fs::read_dir(&servers_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let spec_path = entry.path().join("server.toml");
        if !spec_path.exists() {
            continue;
        }

        let body = fs::read_to_string(&spec_path)?;
        let spec = toml::from_str::<StoredServerAdapterRefs>(&body).map_err(|err| {
            AdapterError::MetadataParse {
                path: spec_path.clone(),
                message: err.to_string(),
            }
        })?;

        if spec.references_adapter(adapter_ref) {
            server_refs.push(if spec.short_ref.is_empty() {
                entry.file_name().to_string_lossy().into_owned()
            } else {
                spec.short_ref
            });
        }
    }

    server_refs.sort();
    server_refs.dedup();
    Ok(server_refs)
}

impl StoredServerAdapterRefs {
    fn references_adapter(&self, adapter_ref: &str) -> bool {
        self.adapter_ref
            .as_deref()
            .is_some_and(|reference| adapter_ref_matches(adapter_ref, reference))
            || self
                .default_adapter_ref
                .as_deref()
                .is_some_and(|reference| adapter_ref_matches(adapter_ref, reference))
            || self
                .allowed_adapters
                .iter()
                .any(|reference| adapter_ref_matches(adapter_ref, reference))
            || self
                .adapter_refs
                .iter()
                .any(|reference| adapter_ref_matches(adapter_ref, reference))
    }
}

fn adapter_ref_matches(adapter_ref: &str, reference: &str) -> bool {
    adapter_ref == reference || adapter_ref.starts_with(reference)
}

fn resolve_servers_dir() -> Result<PathBuf, AdapterError> {
    let home_dir = read_env_path("TENTGENT_HOME").unwrap_or(default_home_dir()?);
    Ok(home_dir.join("servers"))
}

fn read_env_path(name: &str) -> Option<PathBuf> {
    let value = env::var(name).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

fn default_home_dir() -> Result<PathBuf, AdapterError> {
    let project_dirs = directories::ProjectDirs::from("com", "tentserv", "tentgent")
        .ok_or(AdapterError::ProjectDirsUnavailable)?;
    Ok(project_dirs.data_local_dir().to_path_buf())
}
