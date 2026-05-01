use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::{error::ModelError, format::ModelFormat};

const HOME_ENV: &str = "TENTGENT_HOME";
const MODELS_ENV: &str = "TENTGENT_MODELS_DIR";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceKind {
    #[serde(rename = "huggingface")]
    HuggingFace,
    #[serde(rename = "local")]
    Local,
}

impl SourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HuggingFace => "huggingface",
            Self::Local => "local",
        }
    }
}

impl fmt::Display for SourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportMethod {
    #[serde(rename = "add")]
    Add,
    #[serde(rename = "pull")]
    Pull,
}

impl ImportMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Pull => "pull",
        }
    }
}

impl fmt::Display for ImportMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub model_ref: String,
    pub short_ref: String,
    pub source_kind: SourceKind,
    pub source_repo: Option<String>,
    pub source_revision: Option<String>,
    pub source_path: Option<String>,
    pub primary_format: ModelFormat,
    pub detected_formats: Vec<ModelFormat>,
    pub file_count: usize,
    pub total_bytes: u64,
    pub imported_at: String,
}

impl ModelMetadata {
    pub fn source_summary(&self) -> String {
        match self.source_kind {
            SourceKind::HuggingFace => match (&self.source_repo, &self.source_revision) {
                (Some(repo), Some(revision)) => format!("{repo}@{revision}"),
                (Some(repo), None) => repo.clone(),
                _ => "unknown".to_string(),
            },
            SourceKind::Local => self
                .source_path
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantMetadata {
    pub format: ModelFormat,
    pub status: String,
    pub import_method: ImportMethod,
    pub relative_source_path: String,
}

#[derive(Debug, Clone)]
pub struct ModelStorePaths {
    pub store_dir: PathBuf,
    pub hf_index_dir: PathBuf,
    pub local_index_dir: PathBuf,
    pub staging_dir: PathBuf,
    pub servers_dir: PathBuf,
}

impl ModelStorePaths {
    pub fn resolve() -> Result<Self, ModelError> {
        Self::resolve_with_home(None)
    }

    pub fn resolve_with_home(home_override: Option<&Path>) -> Result<Self, ModelError> {
        let home_dir = home_override
            .map(Path::to_path_buf)
            .or_else(|| read_env_path(HOME_ENV))
            .unwrap_or(default_home_dir()?);
        let models_dir = read_env_path(MODELS_ENV).unwrap_or_else(|| home_dir.join("models"));
        let by_source_dir = models_dir.join("by-source");

        Ok(Self {
            store_dir: models_dir.join("store"),
            hf_index_dir: by_source_dir.join("hf"),
            local_index_dir: by_source_dir.join("local"),
            staging_dir: models_dir.join("staging"),
            servers_dir: home_dir.join("servers"),
        })
    }

    pub fn ensure_layout(&self) -> Result<(), ModelError> {
        fs::create_dir_all(&self.store_dir)?;
        fs::create_dir_all(&self.hf_index_dir)?;
        fs::create_dir_all(&self.local_index_dir)?;
        fs::create_dir_all(&self.staging_dir)?;
        Ok(())
    }

    pub fn model_dir(&self, model_ref: &str) -> PathBuf {
        self.store_dir.join(model_ref)
    }

    pub fn model_toml_path(&self, model_ref: &str) -> PathBuf {
        self.model_dir(model_ref).join("model.toml")
    }

    pub fn manifest_path(&self, model_ref: &str) -> PathBuf {
        self.model_dir(model_ref).join("manifest.json")
    }

    pub fn variant_dir(&self, model_ref: &str, format: ModelFormat) -> PathBuf {
        self.model_dir(model_ref)
            .join("variants")
            .join(format.as_str())
    }

    pub fn variant_toml_path(&self, model_ref: &str, format: ModelFormat) -> PathBuf {
        self.variant_dir(model_ref, format).join("variant.toml")
    }

    pub fn variant_source_dir(&self, model_ref: &str, format: ModelFormat) -> PathBuf {
        self.variant_dir(model_ref, format).join("source")
    }
}

pub fn imported_at_now() -> Result<String, ModelError> {
    Ok(OffsetDateTime::now_utc().format(&Rfc3339)?)
}

pub fn write_model_metadata(path: &Path, metadata: &ModelMetadata) -> Result<(), ModelError> {
    let body = toml::to_string_pretty(metadata)?;
    fs::write(path, body)?;
    Ok(())
}

pub fn read_model_metadata(path: &Path) -> Result<ModelMetadata, ModelError> {
    let body = fs::read_to_string(path)?;
    toml::from_str(&body).map_err(|err| ModelError::MetadataParse {
        path: path.to_path_buf(),
        message: err.to_string(),
    })
}

pub fn write_variant_metadata(path: &Path, metadata: &VariantMetadata) -> Result<(), ModelError> {
    let body = toml::to_string_pretty(metadata)?;
    fs::write(path, body)?;
    Ok(())
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

fn default_home_dir() -> Result<PathBuf, ModelError> {
    let project_dirs = ProjectDirs::from("com", "tentserv", "tentgent")
        .ok_or(ModelError::ProjectDirsUnavailable)?;
    Ok(project_dirs.data_local_dir().to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::{
        env,
        sync::Mutex,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn explicit_home_override_sets_models_root() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous = env::var(MODELS_ENV).ok();
        env::remove_var(MODELS_ENV);
        let home = unique_path("model-home");
        let paths = ModelStorePaths::resolve_with_home(Some(&home)).expect("paths");

        restore_env(MODELS_ENV, previous);
        assert_eq!(paths.store_dir, home.join("models/store"));
        assert_eq!(paths.staging_dir, home.join("models/staging"));
    }

    #[test]
    fn specific_models_dir_env_overrides_explicit_home() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let home = unique_path("model-home-env");
        let models = unique_path("model-env-root");
        let previous = env::var(MODELS_ENV).ok();
        env::set_var(MODELS_ENV, &models);

        let paths = ModelStorePaths::resolve_with_home(Some(&home)).expect("paths");

        restore_env(MODELS_ENV, previous);
        assert_eq!(paths.store_dir, models.join("store"));
        assert_eq!(paths.staging_dir, models.join("staging"));
    }

    fn unique_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        env::temp_dir().join(format!("tentgent-{label}-{nanos}"))
    }

    fn restore_env(name: &str, previous: Option<String>) {
        if let Some(value) = previous {
            env::set_var(name, value);
        } else {
            env::remove_var(name);
        }
    }
}
