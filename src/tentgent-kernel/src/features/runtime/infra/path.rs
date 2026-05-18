use std::env;
use std::path::{Path, PathBuf};

use crate::foundation::error::{KernelError, KernelResult};

pub(super) const BOOTSTRAP_SCRIPT: &str = "bootstrap-python-env.sh";
pub(super) const PYPROJECT_FILE: &str = "pyproject.toml";

pub(super) fn development_python_project_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("python/tentgent-daemon")
}

pub(super) fn development_bootstrap_script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("scripts")
        .join(BOOTSTRAP_SCRIPT)
}

pub(super) fn has_pyproject(path: &Path) -> bool {
    path.join(PYPROJECT_FILE).is_file()
}

pub(super) fn read_env_path(name: &str) -> KernelResult<Option<PathBuf>> {
    let Some(raw) = env::var_os(name) else {
        return Ok(None);
    };
    if raw.is_empty() {
        return Ok(None);
    }
    normalize_input_path(PathBuf::from(raw)).map(Some)
}

pub(super) fn normalize_input_path(path: PathBuf) -> KernelResult<PathBuf> {
    let path = expand_home(path);
    let absolute = if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .map_err(|err| {
                KernelError::RuntimeStateUnavailable(format!(
                    "failed to resolve current directory: {err}"
                ))
            })?
            .join(path)
    };

    Ok(normalize_existing_path(absolute))
}

pub(super) fn normalize_existing_path(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

pub(super) fn python_binary_path(env_dir: &Path) -> PathBuf {
    python_bin_dir(env_dir).join(python_executable_name())
}

pub(super) fn python_bin_dir(env_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        env_dir.join("Scripts")
    } else {
        env_dir.join("bin")
    }
}

pub(super) fn python_script_name(name: &str) -> String {
    if cfg!(windows) && !name.ends_with(".exe") {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
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

fn python_executable_name() -> &'static str {
    if cfg!(windows) {
        "python.exe"
    } else {
        "python"
    }
}
