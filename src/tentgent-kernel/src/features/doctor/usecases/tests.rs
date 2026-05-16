use crate::features::doctor::domain::{
    DoctorCheck, DoctorCheckCategory, DoctorCheckStatus, DoctorRepairIntent, DoctorRepairPlan,
    DoctorRepairStep, DoctorReport, DoctorReportRequest, DoctorSummary,
};
use crate::features::runtime::domain::{
    BootstrapProfile, BootstrapRuntimeInput, PythonRuntimeResolutionInput, RuntimeBootstrapOutcome,
    RuntimeBootstrapPlan, RuntimeBootstrapStatus,
};
use crate::features::runtime::usecases::RuntimeBootstrapResult;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;
use crate::foundation::platform::{
    Architecture, CpuFacts, GpuFacts, OperatingSystem, PlatformFacts,
};

use super::{
    DoctorCapabilityReadPolicy, DoctorCommandCheckPolicy, DoctorRepairUseCase,
    DoctorRepairUseCaseRequest, DoctorRepairUseCaseResult, DoctorReportUseCase,
    DoctorReportUseCaseRequest, DoctorReportUseCaseResult,
};

#[test]
fn doctor_report_usecase_port_covers_report_inputs() {
    let usecase = FakeDoctorUseCase;
    let result = usecase
        .doctor_report(DoctorReportUseCaseRequest {
            doctor: DoctorReportRequest::observational(),
            runtime: PythonRuntimeResolutionInput::default(),
            capabilities: DoctorCapabilityReadPolicy::Current,
            commands: DoctorCommandCheckPolicy::IncludeDeveloperTools,
        })
        .expect("doctor report");

    assert_eq!(result.report.status, DoctorCheckStatus::Pass);
    assert_eq!(result.report.summary.pass, 1);
}

#[test]
fn doctor_repair_usecase_port_separates_fix_from_report() {
    let usecase = FakeDoctorUseCase;
    let result = usecase
        .repair_doctor(DoctorRepairUseCaseRequest {
            report: DoctorReportUseCaseRequest {
                doctor: DoctorReportRequest::local_cli()
                    .with_repair(DoctorRepairIntent::DeveloperPythonEnv),
                runtime: PythonRuntimeResolutionInput::default(),
                capabilities: DoctorCapabilityReadPolicy::Refresh,
                commands: DoctorCommandCheckPolicy::IncludeDeveloperTools,
            },
            bootstrap: BootstrapRuntimeInput {
                project_dir: None,
                python_env_dir: None,
                uv_path: None,
                profile: BootstrapProfile::Base,
                dry_run: false,
                print_plan: false,
            },
        })
        .expect("doctor repair");

    assert!(result.plan.mutates_local_state);
    assert!(result.bootstrap.is_some());
    assert_eq!(result.report.status, DoctorCheckStatus::Pass);
}

struct FakeDoctorUseCase;

impl DoctorReportUseCase for FakeDoctorUseCase {
    fn doctor_report(
        &self,
        request: DoctorReportUseCaseRequest,
    ) -> KernelResult<DoctorReportUseCaseResult> {
        let detail = format!(
            "mode={}; capabilities={:?}; commands={:?}; runtime_project={:?}",
            request.doctor.mode,
            request.capabilities,
            request.commands,
            request.runtime.project_dir
        );
        Ok(DoctorReportUseCaseResult {
            report: DoctorReport {
                status: DoctorCheckStatus::Pass,
                summary: DoctorSummary {
                    pass: 1,
                    warn: 0,
                    fail: 0,
                    skipped: 0,
                },
                checks: vec![DoctorCheck::pass(
                    DoctorCheckCategory::Runtime,
                    "fake doctor report",
                    detail,
                )],
            },
        })
    }
}

impl DoctorRepairUseCase for FakeDoctorUseCase {
    fn repair_doctor(
        &self,
        request: DoctorRepairUseCaseRequest,
    ) -> KernelResult<DoctorRepairUseCaseResult> {
        let plan = DoctorRepairPlan {
            intent: request.report.doctor.repair,
            mutates_local_state: request.report.doctor.repair.mutates_local_state(),
            steps: vec![DoctorRepairStep {
                label: "bootstrap managed Python runtime".to_string(),
                command: Some("tentgent runtime bootstrap --profile base".to_string()),
                detail: "delegated to runtime bootstrap".to_string(),
            }],
        };
        let bootstrap = Some(RuntimeBootstrapResult {
            layout: runtime_layout(),
            platform: platform_facts(),
            runtime: crate::features::runtime::domain::PythonRuntimeLayout {
                project_dir: "/tmp/tentgent/python".into(),
                env_dir: "/tmp/tentgent/runtime/python-env".into(),
                source: crate::features::runtime::domain::PythonRuntimeSource::DevelopmentSource,
            },
            plan: RuntimeBootstrapPlan {
                project_dir: "/tmp/tentgent/python".into(),
                python_env_dir: "/tmp/tentgent/runtime/python-env".into(),
                script_path: "/tmp/tentgent/python/scripts/bootstrap-runtime.py".into(),
                uv_path: request.bootstrap.uv_path,
                profile: request.bootstrap.profile,
                dry_run: request.bootstrap.dry_run,
                print_plan: request.bootstrap.print_plan,
            },
            outcome: RuntimeBootstrapOutcome {
                status: RuntimeBootstrapStatus::Succeeded,
                exit_code: Some(0),
            },
        });
        let report = self.doctor_report(request.report)?.report;

        Ok(DoctorRepairUseCaseResult {
            plan,
            bootstrap,
            report,
        })
    }
}

fn runtime_layout() -> RuntimeLayout {
    let root = std::path::PathBuf::from("/tmp/tentgent");
    RuntimeLayout {
        home_dir: root.clone(),
        data_root_dir: root.clone(),
        config_path: root.join("config.toml"),
        models_dir: root.join("models"),
        adapters_dir: root.join("adapters"),
        datasets_dir: root.join("datasets"),
        sessions_dir: root.join("sessions"),
        servers_dir: root.join("servers"),
        train_dir: root.join("train"),
        cache_dir: root.join("cache"),
        runtime_dir: root.join("runtime"),
        logs_dir: root.join("logs"),
        locks_dir: root.join("locks"),
        python_env_dir: root.join("runtime/python-env"),
        bootstrap_dir: root.join("runtime/bootstrap"),
        bootstrap_uv_dir: root.join("runtime/bootstrap/uv"),
        bootstrap_uv_cache_dir: root.join("runtime/bootstrap/uv-cache"),
        capabilities_path: root.join("runtime/capabilities.toml"),
        auth_metadata_path: root.join("runtime/auth.toml"),
    }
}

fn platform_facts() -> PlatformFacts {
    PlatformFacts {
        os: OperatingSystem::Macos,
        arch: Architecture::Aarch64,
        libc: None,
        cpu: CpuFacts {
            vendor: None,
            brand: None,
            features: Vec::new(),
        },
        gpu: GpuFacts {
            devices: Vec::new(),
            cuda: None,
            metal: None,
        },
    }
}
