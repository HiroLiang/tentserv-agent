//! Runtime layout resolution.

use std::env;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use crate::foundation::error::{KernelError, KernelResult};

use super::domain::{LayoutResolveMode, RuntimeLayout};

pub const HOME_ENV: &str = "TENTGENT_HOME";
pub const MODELS_DIR_ENV: &str = "TENTGENT_MODELS_DIR";
pub const ADAPTERS_DIR_ENV: &str = "TENTGENT_ADAPTERS_DIR";
pub const DATASETS_DIR_ENV: &str = "TENTGENT_DATASETS_DIR";
pub const SESSIONS_DIR_ENV: &str = "TENTGENT_SESSIONS_DIR";
pub const SERVERS_DIR_ENV: &str = "TENTGENT_SERVERS_DIR";
pub const TRAIN_DIR_ENV: &str = "TENTGENT_TRAIN_DIR";
pub const CACHE_DIR_ENV: &str = "TENTGENT_CACHE_DIR";
pub const RUNTIME_DIR_ENV: &str = "TENTGENT_RUNTIME_DIR";
pub const LOG_DIR_ENV: &str = "TENTGENT_LOG_DIR";
pub const LOCKS_DIR_ENV: &str = "TENTGENT_LOCKS_DIR";
pub const PYTHON_ENV_DIR_ENV: &str = "TENTGENT_PYTHON_ENV_DIR";
pub const BOOTSTRAP_UV_CACHE_DIR_ENV: &str = "TENTGENT_BOOTSTRAP_UV_CACHE_DIR";

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
const CAPABILITY_MANIFEST: &str = "capabilities.toml";

pub trait RuntimeLayoutResolver {
    fn resolve_runtime_layout(&self, mode: LayoutResolveMode) -> KernelResult<RuntimeLayout>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StdRuntimeLayoutResolver;

impl RuntimeLayoutResolver for StdRuntimeLayoutResolver {
    fn resolve_runtime_layout(&self, mode: LayoutResolveMode) -> KernelResult<RuntimeLayout> {
        let layout = build_runtime_layout()?;

        if mode == LayoutResolveMode::Create {
            ensure_layout_dirs(&layout)?;
        }

        Ok(layout)
    }
}

fn build_runtime_layout() -> KernelResult<RuntimeLayout> {
    let home_dir = runtime_home()?;
    let runtime_dir = env_or_home_join(RUNTIME_DIR_ENV, &home_dir, RUNTIME_DIR);
    let bootstrap_dir = runtime_dir.join(BOOTSTRAP_DIR);

    Ok(RuntimeLayout {
        config_path: home_dir.join(CONFIG_FILE),
        models_dir: env_or_home_join(MODELS_DIR_ENV, &home_dir, MODELS_DIR),
        adapters_dir: env_or_home_join(ADAPTERS_DIR_ENV, &home_dir, ADAPTERS_DIR),
        datasets_dir: env_or_home_join(DATASETS_DIR_ENV, &home_dir, DATASETS_DIR),
        sessions_dir: env_or_home_join(SESSIONS_DIR_ENV, &home_dir, SESSIONS_DIR),
        servers_dir: env_or_home_join(SERVERS_DIR_ENV, &home_dir, SERVERS_DIR),
        train_dir: env_or_home_join(TRAIN_DIR_ENV, &home_dir, TRAIN_DIR),
        cache_dir: env_or_home_join(CACHE_DIR_ENV, &home_dir, CACHE_DIR),
        logs_dir: env_or_home_join(LOG_DIR_ENV, &home_dir, LOGS_DIR),
        locks_dir: env_or_home_join(LOCKS_DIR_ENV, &home_dir, LOCKS_DIR),
        python_env_dir: env_or_base_join(PYTHON_ENV_DIR_ENV, &runtime_dir, PYTHON_ENV_DIR),
        bootstrap_uv_cache_dir: env_or_home_join(
            BOOTSTRAP_UV_CACHE_DIR_ENV,
            &bootstrap_dir,
            BOOTSTRAP_UV_CACHE_DIR,
        ),
        bootstrap_uv_dir: bootstrap_dir.join(BOOTSTRAP_UV_DIR),
        capability_manifest_path: runtime_dir.join(CAPABILITY_MANIFEST),
        bootstrap_dir,
        runtime_dir,
        home_dir,
    })
}

fn runtime_home() -> KernelResult<PathBuf> {
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

fn env_or_home_join(env_name: &str, home: &Path, relative: &str) -> PathBuf {
    env_or_base_join(env_name, home, relative)
}

fn env_or_base_join(env_name: &str, base: &Path, relative: &str) -> PathBuf {
    read_env_path(env_name)
        .map(normalize_path)
        .unwrap_or_else(|| base.join(relative))
}

fn ensure_layout_dirs(layout: &RuntimeLayout) -> KernelResult<()> {
    std::fs::create_dir_all(&layout.home_dir)
        .map_err(|err| KernelError::RuntimeStateUnavailable(err.to_string()))?;

    for dir in layout.standard_dirs() {
        std::fs::create_dir_all(dir)
            .map_err(|err| KernelError::RuntimeStateUnavailable(err.to_string()))?;
    }

    Ok(())
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
