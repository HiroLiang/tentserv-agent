use console::{measure_text_width, style, Term};
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

const STATUS_FIELD_WIDTH: usize = 21;
const STATUS_MIN_VALUE_WIDTH: usize = 16;
const STATUS_MAX_VALUE_WIDTH: usize = 120;

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
            "runtime bootstrap failed{}\n\nNext steps:\n  tentgent runtime bootstrap --print-plan --profile {}\n  tentgent runtime status --profile {}",
            result
                .outcome
                .exit_code
                .map(|code| format!(" with exit code {code}"))
                .unwrap_or_default(),
            result.plan.profile.as_str(),
            result.plan.profile.as_str()
        ));
    }

    Ok(())
}

fn handle_status(
    runtime: &CliRuntimeKernel,
    command: super::commands::RuntimeStatusCommand,
) -> Result<()> {
    let selected_profile = command.profile.map(bootstrap_profile);
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

    let mut rows = Vec::new();
    push_status_row(
        &mut rows,
        "runtime_home",
        result.layout.home_dir.display().to_string(),
    );
    push_status_row(
        &mut rows,
        "python_env",
        path_with_presence(&result.state.python_env_dir, result.state.python.env_exists),
    );
    push_status_row(
        &mut rows,
        "python_binary",
        path_with_presence(
            &result.state.python.binary_path,
            result.state.python.binary_path.is_file(),
        ),
    );
    push_status_row(
        &mut rows,
        "python_version",
        result.state.python.version.as_deref().unwrap_or("unknown"),
    );
    if let Some(runtime) = result.runtime {
        push_status_row(&mut rows, "python_source", runtime.source.as_str());
        push_status_row(
            &mut rows,
            "python_project",
            path_with_presence(&runtime.project_dir, runtime.pyproject_path().is_file()),
        );
    } else {
        push_status_row(&mut rows, "python_source", "unresolved");
        push_status_row(&mut rows, "python_project", "unresolved");
    }
    push_status_row(
        &mut rows,
        "bootstrap_dir",
        result.state.bootstrap_dir.display().to_string(),
    );
    push_status_row(
        &mut rows,
        "uv_cache_dir",
        result.state.uv_cache_dir.display().to_string(),
    );
    add_profile_rows(&mut rows, &result.state, selected_profile);

    print!("{}", format_status_rows(&rows, status_value_width()));
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

fn add_profile_rows(
    rows: &mut Vec<(String, String)>,
    state: &RuntimeInitState,
    selected_profile: Option<BootstrapProfile>,
) {
    for profile in &state.profiles {
        if !profile_matches(selected_profile, profile.profile) {
            continue;
        }

        let value = match &profile.message {
            Some(message) => format!("{}: {message}", readiness_label(profile.readiness)),
            None => readiness_label(profile.readiness).to_string(),
        };
        push_status_row(rows, format!("profile_{}", profile.profile.as_str()), value);
    }
}

fn profile_matches(selected: Option<BootstrapProfile>, profile: BootstrapProfile) -> bool {
    match selected {
        Some(selected) => selected == profile,
        None => true,
    }
}

fn push_status_row(
    rows: &mut Vec<(String, String)>,
    field: impl Into<String>,
    value: impl Into<String>,
) {
    rows.push((field.into(), value.into()));
}

fn format_status_rows(rows: &[(String, String)], value_width: usize) -> String {
    let mut output = String::new();
    for (field, value) in rows {
        let label = format!("{field}:");
        let lines = wrap_status_value(value, value_width);
        output.push_str(&format!("{label:<STATUS_FIELD_WIDTH$}"));
        match lines.split_first() {
            Some((first, rest)) => {
                output.push(' ');
                output.push_str(first);
                output.push('\n');
                for line in rest {
                    output.push_str(&" ".repeat(STATUS_FIELD_WIDTH + 1));
                    output.push_str(line);
                    output.push('\n');
                }
            }
            None => output.push('\n'),
        }
    }
    output.push('\n');
    output
}

fn status_value_width() -> usize {
    let (_, columns) = Term::stdout().size();
    let available = (columns as usize).saturating_sub(STATUS_FIELD_WIDTH + 1);
    available
        .max(STATUS_MIN_VALUE_WIDTH)
        .min(STATUS_MAX_VALUE_WIDTH)
}

fn wrap_status_value(value: &str, max_width: usize) -> Vec<String> {
    let max_width = max_width.max(1);
    let mut lines = Vec::new();
    for segment in value.lines() {
        wrap_status_segment(segment, max_width, &mut lines);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn wrap_status_segment(segment: &str, max_width: usize, lines: &mut Vec<String>) {
    let mut current = String::new();

    for word in segment.split_whitespace() {
        if current.is_empty() {
            if measure_text_width(word) <= max_width {
                current.push_str(word);
            } else {
                hard_wrap_status_word(word, max_width, lines, &mut current);
            }
            continue;
        }

        let candidate = format!("{current} {word}");
        if measure_text_width(&candidate) <= max_width {
            current = candidate;
        } else {
            lines.push(current);
            current = String::new();
            if measure_text_width(word) <= max_width {
                current.push_str(word);
            } else {
                hard_wrap_status_word(word, max_width, lines, &mut current);
            }
        }
    }

    if !current.is_empty() || segment.is_empty() {
        lines.push(current);
    }
}

fn hard_wrap_status_word(
    word: &str,
    max_width: usize,
    lines: &mut Vec<String>,
    current: &mut String,
) {
    for ch in word.chars() {
        let mut buffer = [0; 4];
        let ch_width = measure_text_width(ch.encode_utf8(&mut buffer));
        let current_width = measure_text_width(current);
        if !current.is_empty() && current_width + ch_width > max_width {
            lines.push(std::mem::take(current));
        }
        current.push(ch);
    }
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

    #[test]
    fn runtime_status_profile_filter_matches_only_selected_profile() {
        assert!(profile_matches(None, BootstrapProfile::Training));
        assert!(profile_matches(
            Some(BootstrapProfile::Full),
            BootstrapProfile::Full
        ));
        assert!(!profile_matches(
            Some(BootstrapProfile::Full),
            BootstrapProfile::Training
        ));
    }

    #[test]
    fn runtime_status_formats_as_wrapped_key_value_blocks() {
        let rows = vec![(
            "profile_local-model".to_string(),
            "missing: missing Python modules: diffusers, transformers, torchvision".to_string(),
        )];

        let output = format_status_rows(&rows, 36);
        let continuation = format!("\n{}diffusers", " ".repeat(STATUS_FIELD_WIDTH + 1));

        assert!(output.starts_with("profile_local-model:"));
        assert!(output.contains(&continuation));
        assert!(!output.contains('╭'));
        for line in output.lines().filter(|line| !line.is_empty()) {
            assert!(measure_text_width(line) <= STATUS_FIELD_WIDTH + 1 + 36);
        }
    }

    #[test]
    fn runtime_status_hard_wraps_unbroken_values() {
        let lines = wrap_status_value("abcdefghijklmnopqrstuvwxyz", 8);

        assert_eq!(lines, vec!["abcdefgh", "ijklmnop", "qrstuvwx", "yz"]);
    }
}
