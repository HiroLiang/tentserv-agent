use std::path::PathBuf;

use miette::{miette, Result};
use tentgent_core::runtime_assets::{PythonRuntime, PythonRuntimeSource};

pub fn resolve_python_runtime() -> Result<PythonRuntime> {
    PythonRuntime::resolve()
        .map_err(|err| miette!("failed to resolve Python runtime assets: {err}"))
}

pub fn require_python_interpreter(runtime: &PythonRuntime, label: &str) -> Result<PathBuf> {
    let python = runtime.python_bin();
    if python.exists() {
        return Ok(python);
    }

    Err(miette!(
        "{label} is missing at `{}`; {}",
        python.display(),
        missing_runtime_hint(runtime)
    ))
}

pub fn require_python_script(
    runtime: &PythonRuntime,
    script: &str,
    label: &str,
) -> Result<PathBuf> {
    let entrypoint = runtime.script_bin(script);
    if entrypoint.exists() {
        return Ok(entrypoint);
    }

    Err(miette!(
        "{label} is missing at `{}`; {}",
        entrypoint.display(),
        missing_runtime_hint(runtime)
    ))
}

fn missing_runtime_hint(runtime: &PythonRuntime) -> &'static str {
    match runtime.source() {
        PythonRuntimeSource::InstalledPrefix => {
            "run `tentgent runtime bootstrap`, then run `tentgent doctor` to verify the managed runtime"
        }
        PythonRuntimeSource::DevelopmentSource | PythonRuntimeSource::EnvironmentOverride => {
            "run `tentgent doctor --fix` during development or `tentgent status` to inspect runtime asset paths"
        }
    }
}
