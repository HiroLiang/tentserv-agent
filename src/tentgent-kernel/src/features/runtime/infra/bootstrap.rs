use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::features::runtime::domain::{
    BootstrapRuntimeInput, PythonRuntimeLayout, PythonRuntimeSource, RuntimeBootstrapOutcome,
    RuntimeBootstrapPlan, RuntimeBootstrapStatus,
};
use crate::features::runtime::ports::{RuntimeBootstrapExecutor, RuntimeBootstrapPlanner};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayout;
use crate::foundation::platform::{OperatingSystem, PlatformFacts};

use super::path::{
    development_bootstrap_script, normalize_existing_path, normalize_input_path, BOOTSTRAP_SCRIPT,
};

/// Plans bootstrap script invocations for supported host platforms.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdRuntimeBootstrapPlanner;

impl RuntimeBootstrapPlanner for StdRuntimeBootstrapPlanner {
    fn plan_bootstrap(
        &self,
        _layout: &RuntimeLayout,
        runtime: &PythonRuntimeLayout,
        platform: &PlatformFacts,
        input: BootstrapRuntimeInput,
    ) -> KernelResult<RuntimeBootstrapPlan> {
        ensure_bootstrap_supported(platform)?;

        let project_dir = match input.project_dir {
            Some(path) => normalize_input_path(path)?,
            None => runtime.project_dir.clone(),
        };
        let python_env_dir = match input.python_env_dir {
            Some(path) => normalize_input_path(path)?,
            None => runtime.env_dir.clone(),
        };
        let script_path =
            normalize_existing_path(resolve_bootstrap_script(&project_dir, runtime.source));

        if !script_path.is_file() {
            return Err(KernelError::RuntimeStateUnavailable(format!(
                "runtime bootstrap script is missing at `{}`",
                script_path.display()
            )));
        }

        Ok(RuntimeBootstrapPlan {
            project_dir,
            python_env_dir,
            script_path,
            uv_path: input.uv_path.map(normalize_input_path).transpose()?,
            profile: input.profile,
            dry_run: input.dry_run,
            print_plan: input.print_plan,
        })
    }
}

/// Executes an explicit runtime bootstrap plan with inherited process IO.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdRuntimeBootstrapExecutor;

impl RuntimeBootstrapExecutor for StdRuntimeBootstrapExecutor {
    fn execute_bootstrap(
        &self,
        plan: &RuntimeBootstrapPlan,
    ) -> KernelResult<RuntimeBootstrapOutcome> {
        let mut process = Command::new(&plan.script_path);
        process
            .arg("--project")
            .arg(&plan.project_dir)
            .arg("--env")
            .arg(&plan.python_env_dir)
            .arg("--profile")
            .arg(plan.profile.as_str())
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        if let Some(uv_path) = &plan.uv_path {
            process.arg("--uv").arg(uv_path);
        }
        if plan.dry_run {
            process.arg("--dry-run");
        }
        if plan.print_plan {
            process.arg("--print-plan");
        }

        let status = process.status().map_err(|err| {
            KernelError::RuntimeStateUnavailable(format!(
                "failed to run runtime bootstrap script `{}`: {err}",
                plan.script_path.display()
            ))
        })?;

        Ok(RuntimeBootstrapOutcome {
            status: if status.success() {
                RuntimeBootstrapStatus::Succeeded
            } else {
                RuntimeBootstrapStatus::Failed
            },
            exit_code: status.code(),
        })
    }
}

fn ensure_bootstrap_supported(platform: &PlatformFacts) -> KernelResult<()> {
    match &platform.os {
        OperatingSystem::Macos | OperatingSystem::Linux => Ok(()),
        OperatingSystem::Windows => Err(KernelError::UnsupportedTarget(
            "runtime bootstrap uses the POSIX shell script; a native Windows executor is not implemented yet"
                .to_string(),
        )),
        OperatingSystem::Other(os) => Err(KernelError::UnsupportedTarget(format!(
            "runtime bootstrap is not supported on {os}"
        ))),
    }
}

fn resolve_bootstrap_script(project_dir: &Path, source: PythonRuntimeSource) -> PathBuf {
    let packaged = project_dir
        .parent()
        .map(|parent| parent.join("scripts").join(BOOTSTRAP_SCRIPT));

    if source == PythonRuntimeSource::InstalledPrefix {
        return packaged.unwrap_or_else(development_bootstrap_script);
    }

    if let Some(path) = packaged {
        if path.is_file() {
            return path;
        }
    }

    development_bootstrap_script()
}
