//! Runtime layout data objects.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutResolveMode {
    ReadOnly,
    Create,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeLayoutInput {
    pub mode: LayoutResolveMode,
    pub home_dir: Option<PathBuf>,
    pub data_root_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeLayout {
    pub home_dir: PathBuf,
    pub data_root_dir: PathBuf,

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
    pub capabilities_path: PathBuf,
}
