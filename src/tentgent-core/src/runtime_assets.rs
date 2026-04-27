use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

use directories::ProjectDirs;

pub const HOME_ENV: &str = "TENTGENT_HOME";
pub const PYTHON_DIR_ENV: &str = "TENTGENT_PYTHON_DIR";
pub const PYTHON_ENV_DIR_ENV: &str = "TENTGENT_PYTHON_ENV_DIR";

const PROJECT_DIR_QUALIFIER: &str = "com";
const PROJECT_DIR_ORGANIZATION: &str = "tentserv";
const PROJECT_DIR_APPLICATION: &str = "tentgent";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PythonRuntimeSource {
    EnvironmentOverride,
    InstalledPrefix,
    DevelopmentSource,
}

impl PythonRuntimeSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EnvironmentOverride => "environment",
            Self::InstalledPrefix => "installed-prefix",
            Self::DevelopmentSource => "development-source",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PythonRuntime {
    project_dir: PathBuf,
    env_dir: PathBuf,
    source: PythonRuntimeSource,
}

impl PythonRuntime {
    pub fn resolve() -> Result<Self, RuntimeAssetError> {
        if let Some(project_dir) = read_env_path(PYTHON_DIR_ENV) {
            return Self::from_project_dir(project_dir, PythonRuntimeSource::EnvironmentOverride);
        }

        if let Some(project_dir) = installed_python_project_candidates()?
            .into_iter()
            .find(|candidate| candidate.join("pyproject.toml").exists())
        {
            return Self::from_project_dir(project_dir, PythonRuntimeSource::InstalledPrefix);
        }

        let development_dir = development_python_project_dir();
        if development_dir.join("pyproject.toml").exists() {
            return Self::from_project_dir(development_dir, PythonRuntimeSource::DevelopmentSource);
        }

        Err(RuntimeAssetError::MissingPythonProject {
            path: development_dir,
        })
    }

    pub fn project_dir(&self) -> &Path {
        &self.project_dir
    }

    pub fn env_dir(&self) -> &Path {
        &self.env_dir
    }

    pub fn source(&self) -> PythonRuntimeSource {
        self.source
    }

    pub fn pyproject_path(&self) -> PathBuf {
        self.project_dir.join("pyproject.toml")
    }

    pub fn python_src_dir(&self) -> PathBuf {
        self.project_dir.join("src")
    }

    pub fn python_bin(&self) -> PathBuf {
        python_bin_dir(&self.env_dir).join(python_executable_name())
    }

    pub fn script_bin(&self, name: &str) -> PathBuf {
        python_bin_dir(&self.env_dir).join(name)
    }

    pub fn configure_uv_command(&self, command: &mut Command) {
        command.env("UV_PROJECT_ENVIRONMENT", &self.env_dir);
    }

    fn from_project_dir(
        project_dir: PathBuf,
        source: PythonRuntimeSource,
    ) -> Result<Self, RuntimeAssetError> {
        let project_dir = normalize_path(project_dir);
        if !project_dir.join("pyproject.toml").exists() {
            return Err(RuntimeAssetError::MissingPythonProject { path: project_dir });
        }

        let env_dir = match read_env_path(PYTHON_ENV_DIR_ENV) {
            Some(path) => normalize_path(path),
            None if source == PythonRuntimeSource::InstalledPrefix => {
                resolve_runtime_home()?.join("runtime/python-env")
            }
            None => project_dir.join(".venv"),
        };

        Ok(Self {
            project_dir,
            env_dir,
            source,
        })
    }
}

pub fn resolve_runtime_home() -> Result<PathBuf, RuntimeAssetError> {
    if let Some(path) = read_env_path(HOME_ENV) {
        return Ok(normalize_path(path));
    }

    let project_dirs = ProjectDirs::from(
        PROJECT_DIR_QUALIFIER,
        PROJECT_DIR_ORGANIZATION,
        PROJECT_DIR_APPLICATION,
    )
    .ok_or(RuntimeAssetError::ProjectDirsUnavailable)?;
    Ok(project_dirs.data_local_dir().to_path_buf())
}

pub fn resolve_bootstrap_cache_dir() -> Result<PathBuf, RuntimeAssetError> {
    Ok(resolve_runtime_home()?.join("runtime/bootstrap"))
}

fn installed_python_project_candidates() -> Result<Vec<PathBuf>, RuntimeAssetError> {
    let current_exe = env::current_exe().map_err(RuntimeAssetError::CurrentExe)?;
    let Some(bin_dir) = current_exe.parent() else {
        return Ok(Vec::new());
    };

    let candidates = [
        bin_dir.join("../share/tentgent/python"),
        bin_dir.join("../libexec/tentgent/python"),
    ];

    Ok(candidates.into_iter().map(normalize_path).collect())
}

fn development_python_project_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("python/tentgent-daemon")
}

fn python_bin_dir(env_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        env_dir.join("Scripts")
    } else {
        env_dir.join("bin")
    }
}

fn python_executable_name() -> &'static str {
    if cfg!(windows) {
        "python.exe"
    } else {
        "python"
    }
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

#[derive(Debug, thiserror::Error)]
pub enum RuntimeAssetError {
    #[error("failed to resolve current executable path: {0}")]
    CurrentExe(std::io::Error),
    #[error("Python project metadata was not found at `{path}`")]
    MissingPythonProject { path: PathBuf },
    #[error("failed to resolve Tentgent platform directories")]
    ProjectDirsUnavailable,
}
