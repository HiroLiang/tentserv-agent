use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use console::style;
use miette::{miette, IntoDiagnostic, Result};
use tentgent_core::runtime_assets::{
    resolve_runtime_home, PythonRuntime, PythonRuntimeSource, RuntimeAssetError,
};

use super::commands::{RuntimeBootstrapCommand, RuntimeCommands};

pub fn handle_runtime_command(action: RuntimeCommands) -> Result<()> {
    match action {
        RuntimeCommands::Bootstrap(command) => handle_bootstrap(command),
    }
}

fn handle_bootstrap(command: RuntimeBootstrapCommand) -> Result<()> {
    let runtime = PythonRuntime::resolve().ok();
    let project = resolve_project_path(command.project.as_deref(), runtime.as_ref())?;
    let env_dir = resolve_env_path(command.env.as_deref(), runtime.as_ref())?;
    let source = runtime
        .as_ref()
        .map(PythonRuntime::source)
        .unwrap_or(PythonRuntimeSource::EnvironmentOverride);
    let script = normalize_existing_path(resolve_bootstrap_script(&project, source));

    if !script.is_file() {
        return Err(miette!(
            "runtime bootstrap script is missing at `{}`; reinstall Tentgent or pass --project/--env to inspect a valid runtime layout",
            script.display()
        ));
    }

    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Tentgent runtime bootstrap").bold()
    );
    println!("project: {}", project.display());
    println!("env: {}", env_dir.display());
    println!("script: {}", script.display());
    println!("profile: {}", command.profile);

    let mut process = Command::new(&script);
    process
        .arg("--project")
        .arg(&project)
        .arg("--env")
        .arg(&env_dir)
        .arg("--profile")
        .arg(command.profile.as_str())
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    if let Some(uv) = command.uv {
        process.arg("--uv").arg(uv);
    }
    if command.dry_run {
        process.arg("--dry-run");
    }
    if command.print_plan {
        process.arg("--print-plan");
    }

    let status = process
        .status()
        .into_diagnostic()
        .map_err(|err| miette!("failed to run runtime bootstrap script: {err}"))?;
    if !status.success() {
        return Err(miette!("runtime bootstrap failed with status {status}"));
    }

    Ok(())
}

fn resolve_project_path(
    override_path: Option<&Path>,
    runtime: Option<&PythonRuntime>,
) -> Result<PathBuf> {
    if let Some(path) = override_path {
        return Ok(normalize_input_path(path)?);
    }
    let runtime = runtime.ok_or_else(|| {
        miette!("failed to resolve Python runtime assets; pass --project to specify the packaged Python project")
    })?;
    Ok(runtime.project_dir().to_path_buf())
}

fn resolve_env_path(
    override_path: Option<&Path>,
    runtime: Option<&PythonRuntime>,
) -> Result<PathBuf> {
    if let Some(path) = override_path {
        return Ok(normalize_input_path(path)?);
    }
    if let Some(runtime) = runtime {
        return Ok(runtime.env_dir().to_path_buf());
    }
    Ok(resolve_runtime_home()
        .map_err(runtime_home_error)?
        .join("runtime/python-env"))
}

fn runtime_home_error(error: RuntimeAssetError) -> miette::Report {
    miette!("failed to resolve Tentgent runtime home: {error}")
}

fn normalize_input_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir()
            .into_diagnostic()
            .map_err(|err| miette!("failed to resolve current directory: {err}"))?
            .join(path))
    }
}

fn normalize_existing_path(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

fn resolve_bootstrap_script(project: &Path, source: PythonRuntimeSource) -> PathBuf {
    let packaged = project
        .parent()
        .map(|parent| parent.join("scripts/bootstrap-python-env.sh"));
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

fn development_bootstrap_script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("scripts/bootstrap-python-env.sh")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_prefix_script_is_project_sibling() {
        let script = resolve_bootstrap_script(
            Path::new("/opt/tentgent/share/tentgent/python"),
            PythonRuntimeSource::InstalledPrefix,
        );

        assert_eq!(
            script,
            PathBuf::from("/opt/tentgent/share/tentgent/scripts/bootstrap-python-env.sh")
        );
    }

    #[test]
    fn development_script_uses_repo_root() {
        let script = resolve_bootstrap_script(
            Path::new("/tmp/does-not-have-sibling-script/python"),
            PythonRuntimeSource::DevelopmentSource,
        );

        assert!(script.ends_with("scripts/bootstrap-python-env.sh"));
    }

    #[test]
    fn relative_override_paths_are_made_absolute() {
        let path = normalize_input_path(Path::new("python/tentgent-daemon"))
            .expect("normalize relative path");

        assert!(path.is_absolute());
        assert!(path.ends_with("python/tentgent-daemon"));
    }
}
