use std::env;
use std::path::PathBuf;

use crate::features::runtime::domain::{
    PythonRuntimeLayout, PythonRuntimeResolutionInput, PythonRuntimeSource, PYTHON_ENV_DIR_ENV,
    PYTHON_PROJECT_ENV,
};
use crate::features::runtime::ports::PythonRuntimeResolver;
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayout;

use super::path::{
    development_python_project_dir, has_pyproject, normalize_existing_path, normalize_input_path,
    read_env_path,
};

/// Resolves Python runtime assets from explicit input, environment, install layout, or source tree.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdPythonRuntimeResolver;

impl PythonRuntimeResolver for StdPythonRuntimeResolver {
    fn resolve_python_runtime(
        &self,
        layout: &RuntimeLayout,
        input: PythonRuntimeResolutionInput,
    ) -> KernelResult<PythonRuntimeLayout> {
        if let Some(project_dir) = input.project_dir {
            return python_runtime_from_project_dir(
                layout,
                project_dir,
                input.python_env_dir,
                PythonRuntimeSource::EnvironmentOverride,
            );
        }

        if let Some(project_dir) = read_env_path(PYTHON_PROJECT_ENV)? {
            return python_runtime_from_project_dir(
                layout,
                project_dir,
                input.python_env_dir,
                PythonRuntimeSource::EnvironmentOverride,
            );
        }

        if let Some(project_dir) = installed_python_project_candidates()?
            .into_iter()
            .find(|candidate| has_pyproject(candidate))
        {
            return python_runtime_from_project_dir(
                layout,
                project_dir,
                input.python_env_dir,
                PythonRuntimeSource::InstalledPrefix,
            );
        }

        let development_dir = development_python_project_dir();
        if has_pyproject(&development_dir) {
            return python_runtime_from_project_dir(
                layout,
                development_dir,
                input.python_env_dir,
                PythonRuntimeSource::DevelopmentSource,
            );
        }

        Err(KernelError::RuntimeStateUnavailable(format!(
            "Python project metadata was not found at `{}`",
            development_dir.display()
        )))
    }
}

fn python_runtime_from_project_dir(
    layout: &RuntimeLayout,
    project_dir: PathBuf,
    explicit_env_dir: Option<PathBuf>,
    source: PythonRuntimeSource,
) -> KernelResult<PythonRuntimeLayout> {
    let project_dir = normalize_input_path(project_dir)?;
    if !has_pyproject(&project_dir) {
        return Err(KernelError::RuntimeStateUnavailable(format!(
            "Python project metadata was not found at `{}`",
            project_dir.display()
        )));
    }

    let env_dir = match explicit_env_dir {
        Some(path) => normalize_input_path(path)?,
        None => match read_env_path(PYTHON_ENV_DIR_ENV)? {
            Some(path) => normalize_input_path(path)?,
            None if source == PythonRuntimeSource::InstalledPrefix => layout.python_env_dir.clone(),
            None => project_dir.join(".venv"),
        },
    };

    Ok(PythonRuntimeLayout {
        project_dir,
        env_dir,
        source,
    })
}

fn installed_python_project_candidates() -> KernelResult<Vec<PathBuf>> {
    let current_exe = env::current_exe().map_err(|err| {
        KernelError::RuntimeStateUnavailable(format!("failed to resolve current executable: {err}"))
    })?;
    let Some(bin_dir) = current_exe.parent() else {
        return Ok(Vec::new());
    };

    Ok([
        bin_dir.join("../share/tentgent/python/tentgent-model-runtime"),
        bin_dir.join("../share/tentgent/python"),
        bin_dir.join("../share/tentgent/tentgent-model-runtime"),
        bin_dir.join("../libexec/tentgent/python"),
        bin_dir.join("../libexec/tentgent/tentgent-model-runtime"),
    ]
    .into_iter()
    .map(normalize_existing_path)
    .collect())
}
