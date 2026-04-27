use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::error::DatasetError;

const HOME_ENV: &str = "TENTGENT_HOME";
const DATASETS_ENV: &str = "TENTGENT_DATASETS_DIR";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DatasetFormat {
    #[serde(rename = "jsonl")]
    Jsonl,
    #[serde(rename = "directory")]
    Directory,
}

impl DatasetFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Jsonl => "jsonl",
            Self::Directory => "directory",
        }
    }
}

impl fmt::Display for DatasetFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DatasetSourceKind {
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "generated")]
    Generated,
    #[serde(rename = "huggingface")]
    HuggingFace,
}

impl DatasetSourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Generated => "generated",
            Self::HuggingFace => "huggingface",
        }
    }
}

impl fmt::Display for DatasetSourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetMetadata {
    pub dataset_ref: String,
    pub short_ref: String,
    pub source_kind: DatasetSourceKind,
    pub source_path: Option<String>,
    pub source_repo: Option<String>,
    pub source_revision: Option<String>,
    pub dataset_format: DatasetFormat,
    pub file_count: usize,
    pub total_bytes: u64,
    pub imported_at: String,
    #[serde(default)]
    pub package: DatasetPackageMetadata,
}

impl DatasetMetadata {
    pub fn source_summary(&self) -> String {
        match self.source_kind {
            DatasetSourceKind::Local => self
                .source_path
                .clone()
                .unwrap_or_else(|| "(local source not recorded)".to_string()),
            DatasetSourceKind::Generated => "generated".to_string(),
            DatasetSourceKind::HuggingFace => match (&self.source_repo, &self.source_revision) {
                (Some(repo), Some(revision)) => format!("{repo}@{revision}"),
                (Some(repo), None) => repo.clone(),
                _ => "(huggingface source not recorded)".to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DatasetPackageMetadata {
    pub tuning_ready: bool,
    pub splits: DatasetSplits,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DatasetSplits {
    pub train: Option<String>,
    pub validation: Option<String>,
    pub test: Option<String>,
    pub eval_cases: Option<String>,
    pub source_manifest: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DatasetStorePaths {
    pub datasets_dir: PathBuf,
    pub store_dir: PathBuf,
    pub local_index_dir: PathBuf,
    pub staging_dir: PathBuf,
}

impl DatasetStorePaths {
    pub fn resolve() -> Result<Self, DatasetError> {
        let home_dir = read_env_path(HOME_ENV).unwrap_or(default_home_dir()?);
        let datasets_dir = read_env_path(DATASETS_ENV).unwrap_or_else(|| home_dir.join("datasets"));
        let by_source_dir = datasets_dir.join("by-source");

        Ok(Self {
            store_dir: datasets_dir.join("store"),
            local_index_dir: by_source_dir.join("local"),
            staging_dir: datasets_dir.join("staging"),
            datasets_dir,
        })
    }

    pub fn ensure_layout(&self) -> Result<(), DatasetError> {
        fs::create_dir_all(&self.store_dir)?;
        fs::create_dir_all(&self.local_index_dir)?;
        fs::create_dir_all(&self.staging_dir)?;
        Ok(())
    }

    pub fn dataset_dir(&self, dataset_ref: &str) -> PathBuf {
        self.store_dir.join(dataset_ref)
    }

    pub fn dataset_toml_path(&self, dataset_ref: &str) -> PathBuf {
        self.dataset_dir(dataset_ref).join("dataset.toml")
    }

    pub fn manifest_path(&self, dataset_ref: &str) -> PathBuf {
        self.dataset_dir(dataset_ref).join("manifest.json")
    }

    pub fn source_dir(&self, dataset_ref: &str) -> PathBuf {
        self.dataset_dir(dataset_ref).join("source")
    }
}

pub fn imported_at_now() -> Result<String, DatasetError> {
    Ok(OffsetDateTime::now_utc().format(&Rfc3339)?)
}

pub fn write_dataset_metadata(path: &Path, metadata: &DatasetMetadata) -> Result<(), DatasetError> {
    let body = toml::to_string_pretty(metadata)?;
    fs::write(path, body)?;
    Ok(())
}

pub fn read_dataset_metadata(path: &Path) -> Result<DatasetMetadata, DatasetError> {
    let body = fs::read_to_string(path)?;
    toml::from_str(&body).map_err(|err| DatasetError::MetadataParse {
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

fn default_home_dir() -> Result<PathBuf, DatasetError> {
    let project_dirs = ProjectDirs::from("com", "tentserv", "tentgent")
        .ok_or(DatasetError::ProjectDirsUnavailable)?;
    Ok(project_dirs.data_local_dir().to_path_buf())
}
