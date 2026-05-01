use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::error::AdapterError;

const HOME_ENV: &str = "TENTGENT_HOME";
const ADAPTERS_ENV: &str = "TENTGENT_ADAPTERS_DIR";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdapterFormat {
    #[serde(rename = "peft")]
    Peft,
    #[serde(rename = "mlx")]
    Mlx,
    #[serde(rename = "llama-cpp")]
    LlamaCpp,
}

impl AdapterFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Peft => "peft",
            Self::Mlx => "mlx",
            Self::LlamaCpp => "llama-cpp",
        }
    }
}

impl fmt::Display for AdapterFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdapterType {
    #[serde(rename = "lora")]
    Lora,
}

impl AdapterType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lora => "lora",
        }
    }
}

impl fmt::Display for AdapterType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdapterSourceKind {
    #[serde(rename = "huggingface")]
    HuggingFace,
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "train-run")]
    TrainRun,
}

impl AdapterSourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HuggingFace => "huggingface",
            Self::Local => "local",
            Self::TrainRun => "train-run",
        }
    }
}

impl fmt::Display for AdapterSourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterMetadata {
    pub adapter_ref: String,
    pub short_ref: String,
    pub adapter_format: AdapterFormat,
    pub adapter_type: AdapterType,
    pub base_model_ref: Option<String>,
    pub base_model_source_repo: Option<String>,
    pub base_model_source_revision: Option<String>,
    pub model_family: Option<String>,
    pub backend_support: Vec<String>,
    pub source_kind: AdapterSourceKind,
    pub source_repo: Option<String>,
    pub source_revision: Option<String>,
    pub source_path: Option<String>,
    pub training_dataset_ref: Option<String>,
    pub training_run_ref: Option<String>,
    pub training_config_ref: Option<String>,
    pub file_count: usize,
    pub total_bytes: u64,
    pub imported_at: String,
}

#[derive(Debug, Clone)]
pub struct AdapterStorePaths {
    pub adapters_dir: PathBuf,
    pub store_dir: PathBuf,
    pub by_base_dir: PathBuf,
    pub hf_index_dir: PathBuf,
    pub local_index_dir: PathBuf,
    pub train_run_index_dir: PathBuf,
    pub staging_dir: PathBuf,
    pub servers_dir: PathBuf,
}

impl AdapterStorePaths {
    pub fn resolve() -> Result<Self, AdapterError> {
        Self::resolve_with_home(None)
    }

    pub fn resolve_with_home(home_override: Option<&Path>) -> Result<Self, AdapterError> {
        let home_dir = home_override
            .map(Path::to_path_buf)
            .or_else(|| read_env_path(HOME_ENV))
            .unwrap_or(default_home_dir()?);
        let adapters_dir = read_env_path(ADAPTERS_ENV).unwrap_or_else(|| home_dir.join("adapters"));
        let by_source_dir = adapters_dir.join("by-source");

        Ok(Self {
            store_dir: adapters_dir.join("store"),
            by_base_dir: adapters_dir.join("by-base"),
            hf_index_dir: by_source_dir.join("hf"),
            local_index_dir: by_source_dir.join("local"),
            train_run_index_dir: by_source_dir.join("train-run"),
            staging_dir: adapters_dir.join("staging"),
            servers_dir: home_dir.join("servers"),
            adapters_dir,
        })
    }

    pub fn ensure_layout(&self) -> Result<(), AdapterError> {
        fs::create_dir_all(&self.store_dir)?;
        fs::create_dir_all(&self.by_base_dir)?;
        fs::create_dir_all(&self.hf_index_dir)?;
        fs::create_dir_all(&self.local_index_dir)?;
        fs::create_dir_all(&self.train_run_index_dir)?;
        fs::create_dir_all(&self.staging_dir)?;
        Ok(())
    }

    pub fn adapter_dir(&self, adapter_ref: &str) -> PathBuf {
        self.store_dir.join(adapter_ref)
    }

    pub fn adapter_toml_path(&self, adapter_ref: &str) -> PathBuf {
        self.adapter_dir(adapter_ref).join("adapter.toml")
    }

    pub fn manifest_path(&self, adapter_ref: &str) -> PathBuf {
        self.adapter_dir(adapter_ref).join("manifest.json")
    }

    pub fn source_dir(&self, adapter_ref: &str) -> PathBuf {
        self.adapter_dir(adapter_ref).join("source")
    }
}

pub fn imported_at_now() -> Result<String, AdapterError> {
    Ok(OffsetDateTime::now_utc().format(&Rfc3339)?)
}

pub fn write_adapter_metadata(path: &Path, metadata: &AdapterMetadata) -> Result<(), AdapterError> {
    let body = toml::to_string_pretty(metadata)?;
    fs::write(path, body)?;
    Ok(())
}

pub fn read_adapter_metadata(path: &Path) -> Result<AdapterMetadata, AdapterError> {
    let body = fs::read_to_string(path)?;
    toml::from_str(&body).map_err(|err| AdapterError::MetadataParse {
        path: path.to_path_buf(),
        message: err.to_string(),
    })
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
    let project_dirs = ProjectDirs::from("com", "tentserv", "tentgent")
        .ok_or(AdapterError::ProjectDirsUnavailable)?;
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
    fn explicit_home_override_sets_adapters_root() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous = env::var(ADAPTERS_ENV).ok();
        env::remove_var(ADAPTERS_ENV);
        let home = unique_path("adapter-home");
        let paths = AdapterStorePaths::resolve_with_home(Some(&home)).expect("paths");

        restore_env(ADAPTERS_ENV, previous);
        assert_eq!(paths.adapters_dir, home.join("adapters"));
        assert_eq!(paths.store_dir, home.join("adapters/store"));
    }

    #[test]
    fn specific_adapters_dir_env_overrides_explicit_home() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let home = unique_path("adapter-home-env");
        let adapters = unique_path("adapter-env-root");
        let previous = env::var(ADAPTERS_ENV).ok();
        env::set_var(ADAPTERS_ENV, &adapters);

        let paths = AdapterStorePaths::resolve_with_home(Some(&home)).expect("paths");

        restore_env(ADAPTERS_ENV, previous);
        assert_eq!(paths.adapters_dir, adapters);
        assert_eq!(paths.store_dir, adapters.join("store"));
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
