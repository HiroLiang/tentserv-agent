use console::style;
use miette::{miette, IntoDiagnostic, Result};
use tentgent_kernel::{
    capabilities::{
        infra::{FileCapabilityStateStore, StdMachineCapabilitiesProbe},
        usecases::StdMachineCapabilitiesResolver,
    },
    features::{
        doctor::{
            domain::{
                DoctorCheck, DoctorCheckCategory, DoctorCheckStatus, DoctorRepairIntent,
                DoctorReport, DoctorReportRequest,
            },
            infra::{
                StdDoctorCapabilityCheckMapper, StdDoctorCommandProbe, StdDoctorPathProbe,
                StdDoctorRepairPlanner, StdDoctorRuntimeCheckMapper,
            },
            usecases::{
                DoctorCapabilityReadPolicy, DoctorCommandCheckPolicy, DoctorRepairUseCase,
                DoctorRepairUseCaseRequest, DoctorReportUseCase, DoctorReportUseCaseRequest,
                StdDoctorRepairUseCase, StdDoctorReportUseCase,
            },
        },
        model::{
            infra::{FileModelCapabilityProofStore, FileModelCatalogStore},
            ports::ModelCapabilityProofStore,
            usecases::{ModelCatalogReadUseCase, ModelListRequest, StdModelCatalogReadUseCase},
        },
        runtime::{
            domain::{
                BootstrapProfile, BootstrapRuntimeInput, PythonRuntimeResolutionInput,
                RuntimeBootstrapStatus,
            },
            infra::{
                StdPythonRuntimeResolver, StdRuntimeBootstrapExecutor, StdRuntimeBootstrapPlanner,
                StdRuntimeStateProbe,
            },
            usecases::{
                RuntimeBootstrapResult, StdRuntimeBootstrapUseCase, StdRuntimeStateUseCase,
            },
        },
    },
    foundation::{
        layout::{LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver},
        platform::StdPlatformProbe,
    },
};

use super::{
    commands::DoctorCommand,
    model_support::{model_support_summaries, support_status_is_healthy},
    runtime_footprint::{collect_runtime_footprint_best_effort, FootprintEntry},
};

pub fn handle_doctor_command(command: DoctorCommand) -> Result<()> {
    let kernel = CliDoctorKernel::new();
    let report = if command.fix {
        handle_repair(&kernel)?
    } else {
        handle_report(&kernel)?
    };
    let report = append_model_support_checks(&kernel, report);

    render_checks(&report.checks);

    if report.summary.fail > 0 {
        return Err(miette!("doctor found {} failure(s)", report.summary.fail));
    }

    Ok(())
}

struct CliDoctorKernel {
    layout_resolver: StdRuntimeLayoutResolver,
    platform_probe: StdPlatformProbe,
    runtime_resolver: StdPythonRuntimeResolver,
    state_probe: StdRuntimeStateProbe,
    bootstrap_planner: StdRuntimeBootstrapPlanner,
    bootstrap_executor: StdRuntimeBootstrapExecutor,
    capability_state_store: FileCapabilityStateStore,
    capability_probe: StdMachineCapabilitiesProbe,
    model_catalog: FileModelCatalogStore,
    model_proofs: FileModelCapabilityProofStore,
    path_probe: StdDoctorPathProbe,
    command_probe: StdDoctorCommandProbe,
    runtime_mapper: StdDoctorRuntimeCheckMapper,
    capability_mapper: StdDoctorCapabilityCheckMapper,
    repair_planner: StdDoctorRepairPlanner,
}

impl CliDoctorKernel {
    fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            platform_probe: StdPlatformProbe,
            runtime_resolver: StdPythonRuntimeResolver,
            state_probe: StdRuntimeStateProbe,
            bootstrap_planner: StdRuntimeBootstrapPlanner,
            bootstrap_executor: StdRuntimeBootstrapExecutor,
            capability_state_store: FileCapabilityStateStore,
            capability_probe: StdMachineCapabilitiesProbe,
            model_catalog: FileModelCatalogStore,
            model_proofs: FileModelCapabilityProofStore,
            path_probe: StdDoctorPathProbe,
            command_probe: StdDoctorCommandProbe,
            runtime_mapper: StdDoctorRuntimeCheckMapper,
            capability_mapper: StdDoctorCapabilityCheckMapper,
            repair_planner: StdDoctorRepairPlanner,
        }
    }
}

fn handle_report(kernel: &CliDoctorKernel) -> Result<DoctorReport> {
    let runtime_state = StdRuntimeStateUseCase::new(
        &kernel.layout_resolver,
        &kernel.runtime_resolver,
        &kernel.state_probe,
    );
    let capabilities = StdMachineCapabilitiesResolver::new(
        &kernel.layout_resolver,
        &kernel.platform_probe,
        &kernel.capability_state_store,
        &kernel.capability_probe,
    );
    let report = StdDoctorReportUseCase::new(
        &runtime_state,
        &capabilities,
        &kernel.path_probe,
        &kernel.command_probe,
        &kernel.runtime_mapper,
        &kernel.capability_mapper,
    );

    Ok(report
        .doctor_report(report_request(DoctorRepairIntent::ReportOnly))
        .into_diagnostic()?
        .report)
}

fn handle_repair(kernel: &CliDoctorKernel) -> Result<DoctorReport> {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Tentgent doctor repair").bold()
    );

    let runtime_state = StdRuntimeStateUseCase::new(
        &kernel.layout_resolver,
        &kernel.runtime_resolver,
        &kernel.state_probe,
    );
    let capabilities = StdMachineCapabilitiesResolver::new(
        &kernel.layout_resolver,
        &kernel.platform_probe,
        &kernel.capability_state_store,
        &kernel.capability_probe,
    );
    let report = StdDoctorReportUseCase::new(
        &runtime_state,
        &capabilities,
        &kernel.path_probe,
        &kernel.command_probe,
        &kernel.runtime_mapper,
        &kernel.capability_mapper,
    );
    let bootstrap = StdRuntimeBootstrapUseCase::new(
        &kernel.layout_resolver,
        &kernel.platform_probe,
        &kernel.runtime_resolver,
        &kernel.bootstrap_planner,
        &kernel.bootstrap_executor,
    );
    let repair = StdDoctorRepairUseCase::new(&kernel.repair_planner, &bootstrap, &report);
    let result = repair
        .repair_doctor(DoctorRepairUseCaseRequest {
            report: report_request(DoctorRepairIntent::DeveloperPythonEnv),
            bootstrap: BootstrapRuntimeInput {
                project_dir: None,
                python_env_dir: None,
                uv_path: None,
                profile: BootstrapProfile::Base,
                dry_run: false,
                print_plan: false,
            },
        })
        .into_diagnostic()?;

    render_repair_summary(&result.plan.steps, result.bootstrap.as_ref());
    if let Some(bootstrap) = &result.bootstrap {
        if bootstrap.outcome.status != RuntimeBootstrapStatus::Succeeded {
            return Err(miette!(
                "doctor repair bootstrap failed{}",
                bootstrap
                    .outcome
                    .exit_code
                    .map(|code| format!(" with exit code {code}"))
                    .unwrap_or_default()
            ));
        }
    }

    Ok(result.report)
}

fn report_request(repair: DoctorRepairIntent) -> DoctorReportUseCaseRequest {
    DoctorReportUseCaseRequest {
        doctor: DoctorReportRequest::local_cli().with_repair(repair),
        runtime: PythonRuntimeResolutionInput::default(),
        capabilities: DoctorCapabilityReadPolicy::Current,
        commands: DoctorCommandCheckPolicy::IncludeDeveloperTools,
    }
}

fn append_model_support_checks(kernel: &CliDoctorKernel, report: DoctorReport) -> DoctorReport {
    let mut checks = report.checks;
    checks.extend(model_support_checks(kernel));
    DoctorReport::from_checks(checks)
}

fn model_support_checks(kernel: &CliDoctorKernel) -> Vec<DoctorCheck> {
    let catalog = StdModelCatalogReadUseCase::new(&kernel.layout_resolver, &kernel.model_catalog);
    let result = match catalog.list_models(ModelListRequest {
        layout: RuntimeLayoutInput {
            mode: LayoutResolveMode::ReadOnly,
            home_dir: None,
            data_root_dir: None,
        },
    }) {
        Ok(result) => result,
        Err(err) => {
            return vec![DoctorCheck::warn(
                DoctorCheckCategory::Capability,
                "model support",
                format!("model support checks unavailable: {err}"),
            )];
        }
    };

    let mut checks = Vec::new();
    let mut supported_count = 0usize;
    let mut tuple_count = 0usize;
    for model in result.models {
        let proofs = match kernel
            .model_proofs
            .list_capability_proofs(&result.store, &model.metadata.model_ref)
        {
            Ok(proofs) => proofs,
            Err(err) => {
                checks.push(DoctorCheck::warn(
                    DoctorCheckCategory::Capability,
                    format!("model support: {}", model.metadata.short_ref),
                    format!("proof lookup unavailable: {err}"),
                ));
                continue;
            }
        };

        for summary in model_support_summaries(&model.metadata, &proofs) {
            tuple_count += 1;
            if support_status_is_healthy(summary.status) {
                supported_count += 1;
            } else {
                checks.push(DoctorCheck::with_status(
                    DoctorCheckCategory::Capability,
                    format!(
                        "model support: {} {}",
                        model.metadata.short_ref,
                        summary.capability.as_str()
                    ),
                    DoctorCheckStatus::Warn,
                    format!(
                        "{} via {}: {}",
                        summary.status.as_str(),
                        summary.evidence.as_str(),
                        summary.short_reason()
                    ),
                ));
            }
        }
    }

    if tuple_count > 0 {
        checks.insert(
            0,
            DoctorCheck::pass(
                DoctorCheckCategory::Capability,
                "model support",
                format!(
                    "{supported_count}/{tuple_count} local model capability tuple(s) are verified or supported"
                ),
            ),
        );
    }

    checks
}

fn render_repair_summary(
    steps: &[tentgent_kernel::features::doctor::domain::DoctorRepairStep],
    bootstrap: Option<&RuntimeBootstrapResult>,
) {
    for step in steps {
        println!("repair: {}", step.label);
        if let Some(command) = &step.command {
            println!("command: {command}");
        }
    }
    if let Some(bootstrap) = bootstrap {
        println!("profile: {}", bootstrap.plan.profile);
        println!("status: {}", bootstrap.outcome.status.as_str());
    }
    println!();
}

fn render_checks(checks: &[DoctorCheck]) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Tentgent doctor").bold()
    );

    for check in checks {
        println!(
            "{} {:<34} {}",
            status_marker(check.status),
            check.name,
            short_summary(&check.detail)
        );
    }
    render_details(checks);
    render_runtime_footprint(&collect_runtime_footprint_best_effort());

    let failures = checks
        .iter()
        .filter(|check| check.status == DoctorCheckStatus::Fail)
        .count();
    let warnings = checks
        .iter()
        .filter(|check| check.status == DoctorCheckStatus::Warn)
        .count();
    let result = if failures > 0 {
        format!("blocked with {failures} failure(s) and {warnings} warning(s)")
    } else if warnings > 0 {
        format!("ready with {warnings} warning(s)")
    } else {
        "ready".to_string()
    };

    println!("Result: {result}");
    println!();
}

fn render_details(checks: &[DoctorCheck]) {
    let notable = checks
        .iter()
        .filter(|check| check.status != DoctorCheckStatus::Pass || should_show_detail(check))
        .collect::<Vec<_>>();
    if notable.is_empty() {
        return;
    }

    println!();
    println!("{}", style("Details").bold());
    for check in notable {
        println!(
            "{} {}: {}",
            status_marker(check.status),
            style(&check.name).bold(),
            check.detail
        );
    }
}

fn render_runtime_footprint(entries: &[FootprintEntry]) {
    if entries.is_empty() {
        return;
    }

    println!();
    println!("{}", style("Runtime footprint").bold());
    for entry in entries {
        println!(
            "{} {:<34} {}",
            style("info").cyan().bold(),
            entry.title,
            entry.render_value()
        );
        if entry.field == "bootstrap_uv_cache_size" {
            if let Some(guidance) = entry.guidance() {
                println!("   {:<34} {guidance}", "note");
            }
        }
    }
}

fn should_show_detail(check: &DoctorCheck) -> bool {
    matches!(
        check.name.as_str(),
        "runtime home"
            | "bootstrap cache"
            | "python source"
            | "python pyproject"
            | "python env"
            | "python binary"
            | "media decoder ffmpeg"
    )
}

fn short_summary(detail: &str) -> String {
    let summary = detail.split(':').next().unwrap_or(detail).trim();
    truncate_middle(summary, 42)
}

fn truncate_middle(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_string();
    }

    let head_len = max_chars.saturating_sub(1) / 2;
    let tail_len = max_chars.saturating_sub(1 + head_len);
    let head = value.chars().take(head_len).collect::<String>();
    let tail = value
        .chars()
        .rev()
        .take(tail_len)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{head}…{tail}")
}

fn status_marker(status: DoctorCheckStatus) -> console::StyledObject<&'static str> {
    match status {
        DoctorCheckStatus::Pass => style("ok").green().bold(),
        DoctorCheckStatus::Warn => style("warn").yellow().bold(),
        DoctorCheckStatus::Fail => style("fail").red().bold(),
        DoctorCheckStatus::Skipped => style("skip").dim(),
    }
}
