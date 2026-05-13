//! Runtime layout data objects.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutResolveMode {
    ReadOnly,
    Create,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeLayout {
    pub home_dir: PathBuf,
    pub config_path: PathBuf,

    pub models_dir: PathBuf,
    pub adapters_dir: PathBuf,
    pub datasets_dir: PathBuf,
    pub sessions_dir: PathBuf,
    pub servers_dir: PathBuf,
    pub train_dir: PathBuf,

    pub cache_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub locks_dir: PathBuf,

    pub python_env_dir: PathBuf,
    pub bootstrap_dir: PathBuf,
    pub bootstrap_uv_dir: PathBuf,
    pub bootstrap_uv_cache_dir: PathBuf,
    pub capability_manifest_path: PathBuf,
}

impl RuntimeLayout {
    pub fn standard_dirs(&self) -> Vec<&Path> {
        vec![
            self.models_dir.as_path(),
            self.adapters_dir.as_path(),
            self.datasets_dir.as_path(),
            self.sessions_dir.as_path(),
            self.servers_dir.as_path(),
            self.train_dir.as_path(),
            self.cache_dir.as_path(),
            self.runtime_dir.as_path(),
            self.logs_dir.as_path(),
            self.locks_dir.as_path(),
            self.bootstrap_dir.as_path(),
            self.bootstrap_uv_dir.as_path(),
            self.bootstrap_uv_cache_dir.as_path(),
        ]
    }
}
