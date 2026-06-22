use std::io::IsTerminal;

use console::style;
use miette::{miette, IntoDiagnostic, Result};
use tentgent_kernel::{
    capabilities::{
        infra::{FileCapabilityStateStore, StdMachineCapabilitiesProbe},
        usecases::StdMachineCapabilitiesResolver,
    },
    features::{
        auth::{
            domain::{
                AuthEnvLoadPolicy, AuthKeyStatus, AuthSourceMode, AuthValidationState, Provider,
            },
            infra::{FileAuthMetadataStore, StdAuthEnvSecretProbe, SystemKeychainAuthSecretStore},
            usecases::{AuthStatusRequest, AuthStatusUseCase, StdAuthStatusUseCase},
        },
        doctor::{
            domain::{
                DoctorCheck, DoctorCheckCategory, DoctorCheckStatus, DoctorNextAction,
                DoctorRepairIntent, DoctorReport, DoctorReportRequest,
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
        layout::{
            LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
        },
        platform::StdPlatformProbe,
    },
};

use super::{
    commands::DoctorCommand,
    model_support::{
        model_support_next_action, model_support_recovery_guidance, model_support_summaries,
        support_status_is_healthy, ModelSupportSummary,
    },
    runtime_footprint::{collect_runtime_footprint_best_effort, FootprintEntry},
};

pub fn handle_doctor_command(command: DoctorCommand) -> Result<()> {
    let kernel = CliDoctorKernel::new();
    let progress = DoctorProgress::auto();
    if command.fix {
        progress.step("checking and repairing runtime setup");
    } else {
        progress.step("checking runtime, dependencies, and capabilities");
    }
    let report = if command.fix {
        handle_repair(&kernel)?
    } else {
        handle_report(&kernel)?
    };
    progress.step("checking local model support");
    let report = append_model_support_checks(&kernel, report);
    progress.step("checking provider auth");
    let report = append_auth_checks(&kernel, report);

    progress.step("rendering doctor report");
    render_checks(&report.checks, &progress);

    if report.summary.fail > 0 {
        return Err(miette!("doctor found {} failure(s)", report.summary.fail));
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct DoctorProgress {
    enabled: bool,
}

impl DoctorProgress {
    fn auto() -> Self {
        Self {
            enabled: std::io::stderr().is_terminal(),
        }
    }

    #[cfg(test)]
    fn disabled() -> Self {
        Self { enabled: false }
    }

    fn step(self, message: &str) {
        if self.enabled {
            eprintln!("doctor: {message}...");
        }
    }
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
    auth_env_probe: StdAuthEnvSecretProbe,
    auth_keychain_store: SystemKeychainAuthSecretStore,
    auth_metadata_store: FileAuthMetadataStore,
}

impl CliDoctorKernel {
    fn new() -> Self {
        let layout_resolver = StdRuntimeLayoutResolver;
        let layout = layout_resolver
            .resolve(RuntimeLayoutInput {
                mode: LayoutResolveMode::ReadOnly,
                home_dir: None,
                data_root_dir: None,
            })
            .expect("runtime layout should resolve for doctor auth checks");
        Self {
            layout_resolver,
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
            auth_env_probe: StdAuthEnvSecretProbe,
            auth_keychain_store: SystemKeychainAuthSecretStore::new(),
            auth_metadata_store: FileAuthMetadataStore::from_layout(&layout),
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
                "doctor repair bootstrap failed{}\n\nNext steps:\n  tentgent runtime bootstrap --print-plan --profile {}\n  tentgent runtime status --profile {}",
                bootstrap
                    .outcome
                    .exit_code
                    .map(|code| format!(" with exit code {code}"))
                    .unwrap_or_default(),
                bootstrap.plan.profile.as_str(),
                bootstrap.plan.profile.as_str()
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

fn append_auth_checks(kernel: &CliDoctorKernel, report: DoctorReport) -> DoctorReport {
    let mut checks = report.checks;
    checks.extend(auth_checks(kernel));
    DoctorReport::from_checks(checks)
}

fn auth_checks(kernel: &CliDoctorKernel) -> Vec<DoctorCheck> {
    let auth = StdAuthStatusUseCase::new(
        &kernel.auth_env_probe,
        &kernel.auth_keychain_store,
        &kernel.auth_metadata_store,
    );
    match auth.status(AuthStatusRequest::all(AuthEnvLoadPolicy::CwdDotenvOverride)) {
        Ok(report) => vec![provider_auth_check(&report.statuses)],
        Err(err) => vec![DoctorCheck::warn(
            DoctorCheckCategory::Auth,
            "provider auth",
            format!("provider auth status unavailable: {err}"),
        )
        .with_next_action(DoctorNextAction::command(
            "Inspect provider auth",
            "tentgent auth status",
        ))],
    }
}

fn provider_auth_check(statuses: &[AuthKeyStatus]) -> DoctorCheck {
    let available = statuses
        .iter()
        .filter(|status| status.effective_source.is_some())
        .count();
    let disabled = statuses
        .iter()
        .filter(|status| status.preference.source_mode == AuthSourceMode::None)
        .count();
    let missing = statuses
        .iter()
        .filter(|status| {
            status.preference.source_mode != AuthSourceMode::None
                && status.effective_source.is_none()
        })
        .collect::<Vec<_>>();
    let needs_validation = statuses
        .iter()
        .filter(|status| {
            status.effective_source.is_some()
                && matches!(
                    status.validation,
                    AuthValidationState::Invalid { .. } | AuthValidationState::Unknown { .. }
                )
        })
        .collect::<Vec<_>>();

    let status = if missing.is_empty() && needs_validation.is_empty() {
        DoctorCheckStatus::Pass
    } else {
        DoctorCheckStatus::Warn
    };
    let mut detail = format!(
        "{available}/{} provider key(s) available locally",
        statuses.len()
    );
    if disabled > 0 {
        detail.push_str(&format!("; {disabled} provider(s) intentionally disabled"));
    }
    if !missing.is_empty() {
        detail.push_str("; missing: ");
        detail.push_str(&provider_list(missing.iter().map(|status| status.provider)));
    }
    if !needs_validation.is_empty() {
        detail.push_str("; validation needs attention: ");
        detail.push_str(&provider_list(
            needs_validation.iter().map(|status| status.provider),
        ));
    }
    detail.push_str("; doctor uses local env and cached keychain metadata only");

    let mut check =
        DoctorCheck::with_status(DoctorCheckCategory::Auth, "provider auth", status, detail);
    for status in missing {
        check = check.with_next_action(provider_auth_set_action(status.provider));
    }
    for status in needs_validation {
        check = check.with_next_action(
            provider_auth_set_action(status.provider)
                .with_detail("replace or validate the provider key"),
        );
    }
    check
}

fn provider_auth_set_action(provider: Provider) -> DoctorNextAction {
    DoctorNextAction::command(
        format!("Set {} API key", provider.display_name()),
        format!("tentgent auth {} set", provider.cli_name()),
    )
    .with_detail(format!(
        "or provide {} in the environment or configured auth file",
        provider.env_var()
    ))
}

fn provider_list(providers: impl IntoIterator<Item = Provider>) -> String {
    providers
        .into_iter()
        .map(|provider| provider.display_name())
        .collect::<Vec<_>>()
        .join(", ")
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
            } else if let Some(check) =
                model_support_warning_check(&model.metadata.short_ref, &summary)
            {
                checks.push(check);
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

fn model_support_warning_check(
    short_ref: &str,
    summary: &ModelSupportSummary,
) -> Option<DoctorCheck> {
    if support_status_is_healthy(summary.status) {
        return None;
    }

    let mut check = DoctorCheck::with_status(
        DoctorCheckCategory::Capability,
        format!(
            "model support: {} {}",
            short_ref,
            summary.capability.as_str()
        ),
        DoctorCheckStatus::Warn,
        model_support_warning_detail(summary),
    );
    if let Some(action) = model_support_next_action(summary, Some(short_ref)) {
        check = check.with_next_action(DoctorNextAction::command(
            "Recover model support proof",
            action,
        ));
    }
    Some(check)
}

fn model_support_warning_detail(summary: &ModelSupportSummary) -> String {
    let mut detail = format!(
        "{} via {}: {}; execution_backend: {}",
        summary.status.as_str(),
        summary.evidence.as_str(),
        summary.short_reason(),
        summary.backend,
    );
    if let Some(profile) = summary.runtime_profile_label() {
        detail.push_str("; runtime_profile: ");
        detail.push_str(&profile);
    }
    if let Some(guidance) = model_support_recovery_guidance(summary, None) {
        detail.push_str("; recovery: ");
        detail.push_str(&guidance);
    }
    detail
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

fn render_checks(checks: &[DoctorCheck], progress: &DoctorProgress) {
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
    progress.step("checking runtime footprint");
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
        for action in &check.next_actions {
            println!("   next: {}", action.label);
            if let Some(command) = &action.command {
                println!("   command: {command}");
            }
            if let Some(detail) = &action.detail {
                println!("   note: {detail}");
            }
        }
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
    if !check.next_actions.is_empty() {
        return true;
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use tentgent_kernel::features::auth::domain::{
        AuthProviderPreference, AuthSecretSource, KeychainPresence,
    };
    use tentgent_kernel::features::model::{
        domain::ModelCapability,
        support_status::{ModelSupportEvidenceKind, ModelSupportStatus},
    };

    #[test]
    fn model_support_warning_check_skips_healthy_statuses() {
        let summary = support_summary(ModelSupportStatus::Verified, None);

        assert!(model_support_warning_check("abc123abc123", &summary).is_none());
    }

    #[test]
    fn model_support_warning_check_reports_failed_status_with_reason() {
        let summary = support_summary(
            ModelSupportStatus::Failed,
            Some("runtime failed".to_string()),
        );
        let check = model_support_warning_check("abc123abc123", &summary)
            .expect("failed support status should warn");

        assert_eq!(check.status, DoctorCheckStatus::Warn);
        assert_eq!(check.category, DoctorCheckCategory::Capability);
        assert_eq!(check.name, "model support: abc123abc123 chat");
        assert_eq!(
            check.detail,
            "failed via local-proof: runtime failed; execution_backend: mlx-lm; recovery: fix the runtime/backend issue, clear the failed proof, then retry the route or rerun verification"
        );
        assert_eq!(check.next_actions.len(), 1);
        assert_eq!(
            check.next_actions[0].command.as_deref(),
            Some("tentgent model capability proof clear abc123abc123 chat")
        );
    }

    #[test]
    fn model_support_warning_check_reports_stale_status_with_reason() {
        let mut summary = support_summary(ModelSupportStatus::Stale, None);
        summary.stale_reason = Some("backend changed from old-backend to mlx-lm".to_string());
        let check = model_support_warning_check("abc123abc123", &summary)
            .expect("stale support status should warn");

        assert_eq!(check.status, DoctorCheckStatus::Warn);
        assert_eq!(
            check.detail,
            "stale via local-proof: backend changed from old-backend to mlx-lm; execution_backend: mlx-lm; recovery: refresh evidence for the current runtime tuple, or clear stale proof before retrying"
        );
        assert_eq!(
            check.next_actions[0].command.as_deref(),
            Some("tentgent model capability proof clear abc123abc123 chat")
        );
    }

    #[test]
    fn provider_auth_check_reports_missing_provider_next_actions() {
        let check = provider_auth_check(&[
            auth_status(
                Provider::OpenAI,
                AuthSourceMode::Auto,
                None,
                AuthValidationState::Missing,
            ),
            auth_status(
                Provider::Gemini,
                AuthSourceMode::Auto,
                Some(AuthSecretSource::Env),
                AuthValidationState::NotChecked,
            ),
        ]);

        assert_eq!(check.status, DoctorCheckStatus::Warn);
        assert_eq!(check.category, DoctorCheckCategory::Auth);
        assert_eq!(check.name, "provider auth");
        assert!(check.detail.contains("missing: OpenAI"));
        assert_eq!(check.next_actions.len(), 1);
        assert_eq!(
            check.next_actions[0].command.as_deref(),
            Some("tentgent auth openai set")
        );
        assert!(check.next_actions[0]
            .detail
            .as_deref()
            .expect("detail")
            .contains("OPENAI_API_KEY"));
    }

    #[test]
    fn details_are_notable_when_next_actions_exist() {
        let check = DoctorCheck::warn(DoctorCheckCategory::Runtime, "custom", "short")
            .with_next_action(DoctorNextAction::command("Fix it", "tentgent doctor"));

        assert!(should_show_detail(&check));
    }

    #[test]
    fn disabled_progress_is_noop() {
        DoctorProgress::disabled().step("checking test progress");
    }

    fn support_summary(
        status: ModelSupportStatus,
        failure_reason: Option<String>,
    ) -> ModelSupportSummary {
        ModelSupportSummary {
            capability: ModelCapability::Chat,
            declared: true,
            status,
            evidence: ModelSupportEvidenceKind::LocalProof,
            backend: "mlx-lm".to_string(),
            mlx_runtime_family: None,
            runtime_version: None,
            runtime_profile: None,
            runtime_profile_version: None,
            reason: "latest local proof failed chat".to_string(),
            stale_reason: None,
            failure_reason,
        }
    }

    fn auth_status(
        provider: Provider,
        source_mode: AuthSourceMode,
        effective_source: Option<AuthSecretSource>,
        validation: AuthValidationState,
    ) -> AuthKeyStatus {
        AuthKeyStatus {
            provider,
            preference: AuthProviderPreference {
                provider,
                source_mode,
                env_file: None,
            },
            env_present: effective_source == Some(AuthSecretSource::Env),
            keychain_presence: match effective_source {
                Some(AuthSecretSource::Keychain) => KeychainPresence::Present,
                _ => KeychainPresence::Unknown,
            },
            effective_source,
            validation,
        }
    }
}
