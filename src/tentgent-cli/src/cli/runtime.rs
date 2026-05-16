use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use miette::{miette, IntoDiagnostic, Result};
use tentgent_kernel::features::runtime::domain::{
    BootstrapProfile, BootstrapRuntimeInput, PythonRuntimeResolutionInput, RuntimeBootstrapStatus,
    RuntimeInitState, RuntimeReadiness,
};
use tentgent_kernel::features::runtime::infra::{
    StdPythonRuntimeResolver, StdRuntimeBootstrapExecutor, StdRuntimeBootstrapPlanner,
    StdRuntimeStateProbe,
};
use tentgent_kernel::features::runtime::usecases::{
    RuntimeBootstrapRequest, RuntimeBootstrapUseCase, RuntimeStateRequest, RuntimeStateUseCase,
    StdRuntimeBootstrapUseCase, StdRuntimeStateUseCase,
};
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver,
};
use tentgent_kernel::foundation::platform::StdPlatformProbe;

use super::commands::{RuntimeBootstrapCommand, RuntimeBootstrapProfile, RuntimeCommands};

pub fn handle_runtime_command(action: RuntimeCommands) -> Result<()> {
    let runtime = CliRuntimeKernel::new();

    match action {
        RuntimeCommands::Bootstrap(command) => handle_bootstrap(&runtime, command),
        RuntimeCommands::Status(command) => handle_status(&runtime, command),
    }
}

struct CliRuntimeKernel {
    layout_resolver: StdRuntimeLayoutResolver,
    platform_probe: StdPlatformProbe,
    runtime_resolver: StdPythonRuntimeResolver,
    bootstrap_planner: StdRuntimeBootstrapPlanner,
    bootstrap_executor: StdRuntimeBootstrapExecutor,
    state_probe: StdRuntimeStateProbe,
}

impl CliRuntimeKernel {
    fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            platform_probe: StdPlatformProbe,
            runtime_resolver: StdPythonRuntimeResolver,
            bootstrap_planner: StdRuntimeBootstrapPlanner,
            bootstrap_executor: StdRuntimeBootstrapExecutor,
            state_probe: StdRuntimeStateProbe,
        }
    }

    fn bootstrap_usecase(&self) -> StdRuntimeBootstrapUseCase<'_> {
        StdRuntimeBootstrapUseCase::new(
            &self.layout_resolver,
            &self.platform_probe,
            &self.runtime_resolver,
            &self.bootstrap_planner,
            &self.bootstrap_executor,
        )
    }

    fn state_usecase(&self) -> StdRuntimeStateUseCase<'_> {
        StdRuntimeStateUseCase::new(
            &self.layout_resolver,
            &self.runtime_resolver,
            &self.state_probe,
        )
    }
}

fn handle_bootstrap(runtime: &CliRuntimeKernel, command: RuntimeBootstrapCommand) -> Result<()> {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Tentgent runtime bootstrap").bold()
    );

    let request = RuntimeBootstrapRequest {
        layout: runtime_layout_input(bootstrap_layout_mode(command.print_plan)),
        runtime: PythonRuntimeResolutionInput {
            project_dir: command.project.clone(),
            python_env_dir: command.env.clone(),
        },
        bootstrap: BootstrapRuntimeInput {
            project_dir: command.project,
            python_env_dir: command.env,
            uv_path: command.uv,
            profile: bootstrap_profile(command.profile),
            dry_run: command.dry_run,
            print_plan: command.print_plan,
        },
    };

    let result = runtime
        .bootstrap_usecase()
        .bootstrap_runtime(request)
        .into_diagnostic()?;

    render_bootstrap_summary(&result.plan);

    if result.outcome.status != RuntimeBootstrapStatus::Succeeded {
        return Err(miette!(
            "runtime bootstrap failed{}",
            result
                .outcome
                .exit_code
                .map(|code| format!(" with exit code {code}"))
                .unwrap_or_default()
        ));
    }

    Ok(())
}

fn handle_status(
    runtime: &CliRuntimeKernel,
    command: super::commands::RuntimeStatusCommand,
) -> Result<()> {
    let result = runtime
        .state_usecase()
        .runtime_state(RuntimeStateRequest {
            layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
            runtime: PythonRuntimeResolutionInput {
                project_dir: command.project,
                python_env_dir: command.env,
            },
        })
        .into_diagnostic()?;

    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Tentgent runtime status").bold()
    );

    let mut table = base_table();
    table.add_row(vec![
        Cell::new("runtime_home"),
        Cell::new(result.layout.home_dir.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("python_env"),
        Cell::new(path_with_presence(
            &result.state.python_env_dir,
            result.state.python.env_exists,
        )),
    ]);
    table.add_row(vec![
        Cell::new("python_binary"),
        Cell::new(path_with_presence(
            &result.state.python.binary_path,
            result.state.python.binary_path.is_file(),
        )),
    ]);
    table.add_row(vec![
        Cell::new("python_version"),
        Cell::new(result.state.python.version.as_deref().unwrap_or("unknown")),
    ]);
    if let Some(runtime) = result.runtime {
        table.add_row(vec![
            Cell::new("python_source"),
            Cell::new(runtime.source.as_str()),
        ]);
        table.add_row(vec![
            Cell::new("python_project"),
            Cell::new(path_with_presence(
                &runtime.project_dir,
                runtime.pyproject_path().is_file(),
            )),
        ]);
    } else {
        table.add_row(vec![Cell::new("python_source"), Cell::new("unresolved")]);
        table.add_row(vec![Cell::new("python_project"), Cell::new("unresolved")]);
    }
    table.add_row(vec![
        Cell::new("bootstrap_dir"),
        Cell::new(result.state.bootstrap_dir.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("uv_cache_dir"),
        Cell::new(result.state.uv_cache_dir.display().to_string()),
    ]);
    add_profile_rows(&mut table, &result.state);

    println!("{table}");
    println!();
    Ok(())
}

fn render_bootstrap_summary(
    plan: &tentgent_kernel::features::runtime::domain::RuntimeBootstrapPlan,
) {
    println!("project: {}", plan.project_dir.display());
    println!("env: {}", plan.python_env_dir.display());
    println!("script: {}", plan.script_path.display());
    println!("profile: {}", plan.profile);
    if let Some(uv_path) = &plan.uv_path {
        println!("uv: {}", uv_path.display());
    }
    if plan.dry_run {
        println!("dry_run: true");
    }
    if plan.print_plan {
        println!("print_plan: true");
    }
}

fn add_profile_rows(table: &mut Table, state: &RuntimeInitState) {
    for profile in &state.profiles {
        let value = match &profile.message {
            Some(message) => format!("{}: {message}", readiness_label(profile.readiness)),
            None => readiness_label(profile.readiness).to_string(),
        };
        table.add_row(vec![
            Cell::new(format!("profile_{}", profile.profile.as_str())),
            Cell::new(value),
        ]);
    }
}

fn base_table() -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["Field", "Value"]);
    table
}

fn path_with_presence(path: &std::path::Path, present: bool) -> String {
    let label = if present { "present" } else { "missing" };
    format!("{label}: {}", path.display())
}

fn readiness_label(readiness: RuntimeReadiness) -> &'static str {
    match readiness {
        RuntimeReadiness::Ready => "ready",
        RuntimeReadiness::Missing => "missing",
        RuntimeReadiness::Stale => "stale",
        RuntimeReadiness::Unsupported => "unsupported",
        RuntimeReadiness::Unknown => "unknown",
    }
}

fn runtime_layout_input(mode: LayoutResolveMode) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode,
        home_dir: None,
        data_root_dir: None,
    }
}

fn bootstrap_layout_mode(print_plan: bool) -> LayoutResolveMode {
    if print_plan {
        LayoutResolveMode::ReadOnly
    } else {
        LayoutResolveMode::Create
    }
}

fn bootstrap_profile(profile: RuntimeBootstrapProfile) -> BootstrapProfile {
    match profile {
        RuntimeBootstrapProfile::Base => BootstrapProfile::Base,
        RuntimeBootstrapProfile::LocalModel => BootstrapProfile::LocalModel,
        RuntimeBootstrapProfile::Training => BootstrapProfile::Training,
        RuntimeBootstrapProfile::Full => BootstrapProfile::Full,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_bootstrap_profile_maps_to_kernel_profile() {
        assert_eq!(
            bootstrap_profile(RuntimeBootstrapProfile::Base),
            BootstrapProfile::Base
        );
        assert_eq!(
            bootstrap_profile(RuntimeBootstrapProfile::LocalModel),
            BootstrapProfile::LocalModel
        );
        assert_eq!(
            bootstrap_profile(RuntimeBootstrapProfile::Training),
            BootstrapProfile::Training
        );
        assert_eq!(
            bootstrap_profile(RuntimeBootstrapProfile::Full),
            BootstrapProfile::Full
        );
    }

    #[test]
    fn runtime_bootstrap_print_plan_does_not_request_layout_creation() {
        assert_eq!(bootstrap_layout_mode(true), LayoutResolveMode::ReadOnly);
        assert_eq!(bootstrap_layout_mode(false), LayoutResolveMode::Create);
    }
}
