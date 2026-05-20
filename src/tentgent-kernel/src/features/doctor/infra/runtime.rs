use std::path::{Path, PathBuf};

use crate::features::doctor::domain::{
    DoctorCheck, DoctorCheckCategory, DoctorCheckStatus, DoctorExecutionMode,
};
use crate::features::doctor::ports::DoctorRuntimeCheckMapper;
use crate::features::runtime::domain::{
    PythonRuntimeLayout, PythonRuntimeSource, RuntimeEntrypoint, RuntimeInitState, RuntimeReadiness,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;

/// Maps runtime facts into doctor checks without bootstrapping.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdDoctorRuntimeCheckMapper;

impl DoctorRuntimeCheckMapper for StdDoctorRuntimeCheckMapper {
    fn runtime_checks(
        &self,
        layout: &RuntimeLayout,
        runtime: Option<&PythonRuntimeLayout>,
        state: Option<&RuntimeInitState>,
        mode: DoctorExecutionMode,
    ) -> KernelResult<Vec<DoctorCheck>> {
        let mut checks = Vec::new();
        let _ = layout;

        match runtime {
            Some(runtime) => {
                checks.extend(runtime_layout_checks(runtime, state, mode));
            }
            None => checks.push(DoctorCheck::fail(
                DoctorCheckCategory::Runtime,
                "python runtime",
                "could not resolve Python runtime assets",
            )),
        }

        match state {
            Some(state) => checks.extend(runtime_state_checks(runtime, state)),
            None => checks.push(DoctorCheck::warn(
                DoctorCheckCategory::Runtime,
                "runtime state",
                "runtime state was not probed",
            )),
        }

        Ok(checks)
    }
}

fn runtime_layout_checks(
    runtime: &PythonRuntimeLayout,
    state: Option<&RuntimeInitState>,
    mode: DoctorExecutionMode,
) -> Vec<DoctorCheck> {
    let missing_status = missing_runtime_status(runtime.source, mode);
    let mut checks = vec![
        DoctorCheck::pass(
            DoctorCheckCategory::Runtime,
            "python source",
            runtime.source.as_str(),
        ),
        check_file(
            "python pyproject",
            &runtime.pyproject_path(),
            DoctorCheckStatus::Fail,
            "Python project metadata is required",
        ),
        check_directory(
            "python package",
            &runtime.python_src_dir(),
            DoctorCheckStatus::Fail,
            "Python package source is required",
        ),
    ];

    if state.is_none() {
        checks.push(check_directory(
            "python env",
            &runtime.env_dir,
            missing_status,
            python_bootstrap_hint(runtime.source),
        ));
        let python_binary = python_binary_path(&runtime.env_dir);
        checks.push(check_file(
            "python binary",
            &python_binary,
            missing_status,
            python_bootstrap_hint(runtime.source),
        ));
    }

    for entrypoint in runtime_entrypoints() {
        checks.push(check_file(
            format!("entrypoint {}", entrypoint.script_name()),
            &entrypoint_path(&runtime.env_dir, entrypoint),
            missing_status,
            python_bootstrap_hint(runtime.source),
        ));
    }

    checks
}

fn runtime_state_checks(
    runtime: Option<&PythonRuntimeLayout>,
    state: &RuntimeInitState,
) -> Vec<DoctorCheck> {
    let source = runtime
        .map(|runtime| runtime.source)
        .unwrap_or(PythonRuntimeSource::InstalledPrefix);
    let missing_status = missing_runtime_status(source, DoctorExecutionMode::Observational);
    let mut checks = Vec::new();

    checks.push(if state.python.env_exists {
        DoctorCheck::pass(
            DoctorCheckCategory::Runtime,
            "python env",
            format!("present: {}", state.python_env_dir.display()),
        )
    } else {
        DoctorCheck::with_status(
            DoctorCheckCategory::Runtime,
            "python env",
            missing_status,
            format!(
                "missing: {}; {}",
                state.python_env_dir.display(),
                python_bootstrap_hint(source)
            ),
        )
    });

    checks.push(if state.python.binary_path.is_file() {
        DoctorCheck::pass(
            DoctorCheckCategory::Runtime,
            "python binary",
            format!("present: {}", state.python.binary_path.display()),
        )
    } else {
        DoctorCheck::with_status(
            DoctorCheckCategory::Runtime,
            "python binary",
            missing_status,
            format!(
                "missing: {}; {}",
                state.python.binary_path.display(),
                python_bootstrap_hint(source)
            ),
        )
    });

    checks.push(match &state.python.version {
        Some(version) => DoctorCheck::pass(
            DoctorCheckCategory::Runtime,
            "python version",
            version.clone(),
        ),
        None => DoctorCheck::with_status(
            DoctorCheckCategory::Runtime,
            "python version",
            missing_status,
            format!(
                "python version is unavailable; {}",
                python_bootstrap_hint(source)
            ),
        ),
    });

    for profile in &state.profiles {
        let status = runtime_readiness_status(profile.readiness, missing_status);
        checks.push(DoctorCheck::with_status(
            DoctorCheckCategory::Runtime,
            format!("runtime profile {}", profile.profile.as_str()),
            status,
            profile
                .message
                .clone()
                .unwrap_or_else(|| profile.readiness.as_str().to_string()),
        ));
    }

    checks
}

fn check_directory(
    name: impl Into<String>,
    path: &Path,
    missing_status: DoctorCheckStatus,
    hint: &str,
) -> DoctorCheck {
    if path.is_dir() {
        DoctorCheck::pass(
            DoctorCheckCategory::Runtime,
            name,
            format!("present: {}", path.display()),
        )
    } else {
        DoctorCheck::with_status(
            DoctorCheckCategory::Runtime,
            name,
            missing_status,
            format!("missing: {}; {hint}", path.display()),
        )
    }
}

fn check_file(
    name: impl Into<String>,
    path: &Path,
    missing_status: DoctorCheckStatus,
    hint: &str,
) -> DoctorCheck {
    if path.is_file() {
        DoctorCheck::pass(
            DoctorCheckCategory::Runtime,
            name,
            format!("present: {}", path.display()),
        )
    } else {
        DoctorCheck::with_status(
            DoctorCheckCategory::Runtime,
            name,
            missing_status,
            format!("missing: {}; {hint}", path.display()),
        )
    }
}

fn missing_runtime_status(
    source: PythonRuntimeSource,
    _mode: DoctorExecutionMode,
) -> DoctorCheckStatus {
    match source {
        PythonRuntimeSource::InstalledPrefix => DoctorCheckStatus::Fail,
        PythonRuntimeSource::DevelopmentSource | PythonRuntimeSource::EnvironmentOverride => {
            DoctorCheckStatus::Warn
        }
    }
}

fn runtime_readiness_status(
    readiness: RuntimeReadiness,
    missing_status: DoctorCheckStatus,
) -> DoctorCheckStatus {
    match readiness {
        RuntimeReadiness::Ready => DoctorCheckStatus::Pass,
        RuntimeReadiness::Missing => missing_status,
        RuntimeReadiness::Stale | RuntimeReadiness::Unsupported | RuntimeReadiness::Unknown => {
            DoctorCheckStatus::Warn
        }
    }
}

fn python_bootstrap_hint(source: PythonRuntimeSource) -> &'static str {
    match source {
        PythonRuntimeSource::InstalledPrefix => {
            "run `tentgent runtime bootstrap`, then run `tentgent doctor` again"
        }
        PythonRuntimeSource::DevelopmentSource | PythonRuntimeSource::EnvironmentOverride => {
            "run `tentgent runtime bootstrap` or an explicit local repair flow"
        }
    }
}

fn runtime_entrypoints() -> [RuntimeEntrypoint; 10] {
    [
        RuntimeEntrypoint::AudioTranscriptionBatch,
        RuntimeEntrypoint::ChatOnce,
        RuntimeEntrypoint::DatasetEval,
        RuntimeEntrypoint::DatasetSynth,
        RuntimeEntrypoint::EmbeddingOnce,
        RuntimeEntrypoint::HfSnapshot,
        RuntimeEntrypoint::ImageGenerateOnce,
        RuntimeEntrypoint::Server,
        RuntimeEntrypoint::TrainLoraRun,
        RuntimeEntrypoint::VisionChatOnce,
    ]
}

fn python_binary_path(env_dir: &Path) -> PathBuf {
    python_bin_dir(env_dir).join(python_executable_name())
}

fn entrypoint_path(env_dir: &Path, entrypoint: RuntimeEntrypoint) -> PathBuf {
    python_bin_dir(env_dir).join(python_script_name(entrypoint.script_name()))
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

fn python_script_name(name: &str) -> String {
    if cfg!(windows) && !name.ends_with(".exe") {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

trait RuntimeReadinessLabel {
    fn as_str(self) -> &'static str;
}

impl RuntimeReadinessLabel for RuntimeReadiness {
    fn as_str(self) -> &'static str {
        match self {
            RuntimeReadiness::Ready => "ready",
            RuntimeReadiness::Missing => "missing",
            RuntimeReadiness::Stale => "stale",
            RuntimeReadiness::Unsupported => "unsupported",
            RuntimeReadiness::Unknown => "unknown",
        }
    }
}
