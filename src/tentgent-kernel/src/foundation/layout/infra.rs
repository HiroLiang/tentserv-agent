//! Standard runtime layout resolver implementation.

use std::env;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use crate::foundation::error::{KernelError, KernelResult};

use super::domain::{LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput};
use super::ports::RuntimeLayoutResolver;

pub const HOME_ENV: &str = "TENTGENT_HOME";
pub const DATA_ROOT_ENV: &str = "TENTGENT_DATA_ROOT";

const PROJECT_DIR_QUALIFIER: &str = "com";
const PROJECT_DIR_ORGANIZATION: &str = "tentserv";
const PROJECT_DIR_APPLICATION: &str = "tentgent";

const CONFIG_FILE: &str = "config.toml";
const MODELS_DIR: &str = "models";
const ADAPTERS_DIR: &str = "adapters";
const DATASETS_DIR: &str = "datasets";
const SESSIONS_DIR: &str = "sessions";
const SERVERS_DIR: &str = "servers";
const TRAIN_DIR: &str = "train";
const CACHE_DIR: &str = "cache";
const RUNTIME_DIR: &str = "runtime";
const LOGS_DIR: &str = "logs";
const LOCKS_DIR: &str = "locks";
const PYTHON_ENV_DIR: &str = "python-env";
const BOOTSTRAP_DIR: &str = "bootstrap";
const BOOTSTRAP_UV_DIR: &str = "uv";
const BOOTSTRAP_UV_CACHE_DIR: &str = "uv-cache";
const CAPABILITIES_FILE: &str = "capabilities.toml";

#[derive(Debug, Clone, Copy, Default)]
pub struct StdRuntimeLayoutResolver;

impl RuntimeLayoutResolver for StdRuntimeLayoutResolver {
    fn resolve(&self, input: RuntimeLayoutInput) -> KernelResult<RuntimeLayout> {
        let layout = build_runtime_layout(&input)?;

        if input.mode == LayoutResolveMode::Create {
            ensure_layout_dirs(&layout)?;
        }

        Ok(layout)
    }
}

fn build_runtime_layout(input: &RuntimeLayoutInput) -> KernelResult<RuntimeLayout> {
    let home_dir = resolve_home_dir(input)?;
    let data_root_dir = resolve_data_root_dir(input, &home_dir);
    let runtime_dir = home_dir.join(RUNTIME_DIR);
    let bootstrap_dir = runtime_dir.join(BOOTSTRAP_DIR);
    let cache_dir = data_root_dir.join(CACHE_DIR);

    Ok(RuntimeLayout {
        config_path: home_dir.join(CONFIG_FILE),

        models_dir: data_root_dir.join(MODELS_DIR),
        adapters_dir: data_root_dir.join(ADAPTERS_DIR),
        datasets_dir: data_root_dir.join(DATASETS_DIR),
        train_dir: data_root_dir.join(TRAIN_DIR),
        cache_dir: cache_dir.clone(),

        sessions_dir: home_dir.join(SESSIONS_DIR),
        servers_dir: home_dir.join(SERVERS_DIR),
        logs_dir: home_dir.join(LOGS_DIR),
        locks_dir: home_dir.join(LOCKS_DIR),

        python_env_dir: runtime_dir.join(PYTHON_ENV_DIR),
        bootstrap_uv_cache_dir: cache_dir.join(BOOTSTRAP_UV_CACHE_DIR),
        bootstrap_uv_dir: bootstrap_dir.join(BOOTSTRAP_UV_DIR),
        capabilities_path: runtime_dir.join(CAPABILITIES_FILE),
        bootstrap_dir,
        runtime_dir,
        data_root_dir,
        home_dir,
    })
}

fn resolve_home_dir(input: &RuntimeLayoutInput) -> KernelResult<PathBuf> {
    if let Some(path) = &input.home_dir {
        return Ok(normalize_path(expand_home(path.clone())));
    }

    if let Some(path) = read_env_path(HOME_ENV) {
        return Ok(normalize_path(path));
    }

    let project_dirs = ProjectDirs::from(
        PROJECT_DIR_QUALIFIER,
        PROJECT_DIR_ORGANIZATION,
        PROJECT_DIR_APPLICATION,
    )
    .ok_or_else(|| {
        KernelError::RuntimeStateUnavailable("project directories unavailable".into())
    })?;

    Ok(project_dirs.data_local_dir().to_path_buf())
}

fn resolve_data_root_dir(input: &RuntimeLayoutInput, home_dir: &Path) -> PathBuf {
    if let Some(path) = &input.data_root_dir {
        return normalize_path(expand_home(path.clone()));
    }

    if let Some(path) = read_env_path(DATA_ROOT_ENV) {
        return normalize_path(path);
    }

    home_dir.to_path_buf()
}

fn ensure_layout_dirs(layout: &RuntimeLayout) -> KernelResult<()> {
    for dir in standard_dirs(layout) {
        std::fs::create_dir_all(dir)
            .map_err(|err| KernelError::RuntimeStateUnavailable(err.to_string()))?;
    }

    Ok(())
}

fn standard_dirs(layout: &RuntimeLayout) -> [&Path; 15] {
    [
        layout.home_dir.as_path(),
        layout.data_root_dir.as_path(),
        layout.models_dir.as_path(),
        layout.adapters_dir.as_path(),
        layout.datasets_dir.as_path(),
        layout.sessions_dir.as_path(),
        layout.servers_dir.as_path(),
        layout.train_dir.as_path(),
        layout.cache_dir.as_path(),
        layout.runtime_dir.as_path(),
        layout.logs_dir.as_path(),
        layout.locks_dir.as_path(),
        layout.bootstrap_dir.as_path(),
        layout.bootstrap_uv_dir.as_path(),
        layout.bootstrap_uv_cache_dir.as_path(),
    ]
}

fn read_env_path(name: &str) -> Option<PathBuf> {
    let raw = env::var_os(name)?;
    if raw.is_empty() {
        return None;
    }
    Some(expand_home(PathBuf::from(raw)))
}

fn expand_home(path: PathBuf) -> PathBuf {
    let raw = path.to_string_lossy();
    if raw == "~" {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home);
        }
    }

    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }

    path
}

fn normalize_path(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}
