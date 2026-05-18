use std::path::Path;
use std::process::Command;

use crate::features::runtime::domain::{
    BootstrapProfile, PythonRuntimeLayout, PythonRuntimeState, RuntimeInitState,
    RuntimeProfileState, RuntimeReadiness,
};
use crate::features::runtime::ports::RuntimeStateProbe;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;

use super::{
    probe_python_modules, python_binary_for_env, runtime_profile_modules, PythonModuleProbe,
};

/// Probes managed runtime state without mutating the environment.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdRuntimeStateProbe;

impl RuntimeStateProbe for StdRuntimeStateProbe {
    fn probe_runtime_state(
        &self,
        layout: &RuntimeLayout,
        runtime: Option<&PythonRuntimeLayout>,
    ) -> KernelResult<RuntimeInitState> {
        let python_env_dir = runtime
            .map(|runtime| runtime.env_dir.clone())
            .unwrap_or_else(|| layout.python_env_dir.clone());
        let python_binary = python_binary_for_env(&python_env_dir);
        let env_exists = python_env_dir.is_dir();
        let binary_exists = python_binary.is_file();

        Ok(RuntimeInitState {
            home_dir: layout.home_dir.clone(),
            python_env_dir: python_env_dir.clone(),
            bootstrap_dir: layout.bootstrap_dir.clone(),
            uv_cache_dir: layout.bootstrap_uv_cache_dir.clone(),
            python: PythonRuntimeState {
                env_exists,
                binary_path: python_binary.clone(),
                version: binary_exists
                    .then(|| python_version(&python_binary))
                    .flatten(),
            },
            profiles: runtime_profiles(&python_binary, env_exists, binary_exists),
        })
    }
}

fn python_version(python_binary: &Path) -> Option<String> {
    let output = Command::new(python_binary).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let stderr = String::from_utf8(output.stderr).ok()?;
    let version = if stdout.trim().is_empty() {
        stderr.trim()
    } else {
        stdout.trim()
    };

    (!version.is_empty()).then(|| version.to_string())
}

fn runtime_profiles(
    python_binary: &Path,
    env_exists: bool,
    binary_exists: bool,
) -> Vec<RuntimeProfileState> {
    [
        BootstrapProfile::Base,
        BootstrapProfile::LocalModel,
        BootstrapProfile::Training,
        BootstrapProfile::Full,
    ]
    .into_iter()
    .map(|profile| runtime_profile(python_binary, profile, env_exists, binary_exists))
    .collect()
}

fn runtime_profile(
    python_binary: &Path,
    profile: BootstrapProfile,
    env_exists: bool,
    binary_exists: bool,
) -> RuntimeProfileState {
    if !env_exists {
        return RuntimeProfileState {
            profile,
            readiness: RuntimeReadiness::Missing,
            message: Some("managed Python environment is missing".to_string()),
        };
    }

    if !binary_exists {
        return RuntimeProfileState {
            profile,
            readiness: RuntimeReadiness::Missing,
            message: Some("managed Python interpreter is missing".to_string()),
        };
    }

    let modules = runtime_profile_modules(profile);
    let probe = probe_python_modules(python_binary, &modules);
    match probe {
        PythonModuleProbe::Ready => RuntimeProfileState {
            profile,
            readiness: RuntimeReadiness::Ready,
            message: Some(if modules.is_empty() {
                "managed Python interpreter is available".to_string()
            } else {
                format!("{} profile dependencies are importable", profile.as_str())
            }),
        },
        PythonModuleProbe::Missing { modules } => RuntimeProfileState {
            profile,
            readiness: RuntimeReadiness::Missing,
            message: Some(format!("missing Python modules: {}", modules.join(", "))),
        },
        PythonModuleProbe::Failed { detail } => RuntimeProfileState {
            profile,
            readiness: RuntimeReadiness::Unknown,
            message: Some(format!("failed to probe Python modules: {detail}")),
        },
    }
}
