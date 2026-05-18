use std::path::PathBuf;

use crate::capabilities::domain::{
    BackendCapability, BackendKind, CapabilityState, MachineCapabilities, RuntimeCapabilityState,
};
use crate::features::runtime::domain::{
    BootstrapProfile, PythonRuntimeLayout, PythonRuntimeSource, PythonRuntimeState,
    RuntimeInitState, RuntimeProfileState, RuntimeReadiness,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;
use crate::foundation::platform::{
    Architecture, CpuFacts, GpuFacts, MetalFacts, OperatingSystem, PlatformFacts,
};

use super::domain::{
    DoctorCheck, DoctorCheckCategory, DoctorCheckStatus, DoctorCommandCheck, DoctorExecutionMode,
    DoctorPathCheck, DoctorPathExpectation, DoctorRepairIntent, DoctorRepairPlan, DoctorRepairStep,
    DoctorReport, DoctorReportRequest,
};
use super::ports::{
    DoctorCapabilityCheckMapper, DoctorCommandProbe, DoctorPathProbe, DoctorRepairPlanner,
    DoctorRuntimeCheckMapper,
};

#[test]
fn doctor_report_status_aggregates_fail_then_warn() {
    let report = DoctorReport::from_checks(vec![
        DoctorCheck::pass(DoctorCheckCategory::Cli, "cli version", "0.0.0"),
        DoctorCheck::warn(DoctorCheckCategory::Command, "uv", "missing"),
        DoctorCheck::fail(DoctorCheckCategory::Runtime, "python binary", "missing"),
        DoctorCheck::skipped(
            DoctorCheckCategory::Bootstrap,
            "bootstrap cache",
            "optional",
        ),
    ]);

    assert_eq!(report.status, DoctorCheckStatus::Fail);
    assert_eq!(report.summary.pass, 1);
    assert_eq!(report.summary.warn, 1);
    assert_eq!(report.summary.fail, 1);
    assert_eq!(report.summary.skipped, 1);
}

#[test]
fn observational_doctor_request_is_read_only_by_default() {
    let request =
        DoctorReportRequest::observational().with_runtime_home(PathBuf::from("/tmp/tentgent"));

    assert_eq!(request.mode, DoctorExecutionMode::Observational);
    assert!(!request.mode.allows_write_probes());
    assert_eq!(request.repair, DoctorRepairIntent::ReportOnly);
    assert!(!request.repair.mutates_local_state());
}

#[test]
fn local_cli_repair_intent_is_explicitly_mutating() {
    let request =
        DoctorReportRequest::local_cli().with_repair(DoctorRepairIntent::DeveloperPythonEnv);

    assert_eq!(request.mode, DoctorExecutionMode::LocalCli);
    assert!(request.mode.allows_write_probes());
    assert!(request.repair.mutates_local_state());
}

#[test]
fn doctor_ports_cover_path_command_runtime_capability_and_repair_boundaries() {
    let ports = FakeDoctorPorts;
    let layout = runtime_layout("/tmp/tentgent-home");
    let runtime = PythonRuntimeLayout {
        project_dir: PathBuf::from("/opt/tentgent/python"),
        env_dir: layout.python_env_dir.clone(),
        source: PythonRuntimeSource::InstalledPrefix,
    };
    let state = RuntimeInitState {
        home_dir: layout.home_dir.clone(),
        python_env_dir: runtime.env_dir.clone(),
        bootstrap_dir: layout.bootstrap_dir.clone(),
        uv_cache_dir: layout.bootstrap_uv_cache_dir.clone(),
        python: PythonRuntimeState {
            env_exists: true,
            binary_path: runtime.env_dir.join("bin/python"),
            version: Some("3.13".to_string()),
        },
        profiles: vec![RuntimeProfileState {
            profile: BootstrapProfile::Base,
            readiness: RuntimeReadiness::Ready,
            message: None,
        }],
    };

    let path_check = ports
        .check_path(DoctorPathCheck {
            name: "runtime home".to_string(),
            category: DoctorCheckCategory::RuntimeHome,
            path: layout.home_dir.clone(),
            expectation: DoctorPathExpectation::RequiredDirectory,
            mode: DoctorExecutionMode::Observational,
        })
        .expect("path check");
    assert_eq!(path_check.status, DoctorCheckStatus::Pass);

    let command_check = ports
        .check_command(DoctorCommandCheck {
            name: "uv dev bootstrap".to_string(),
            category: DoctorCheckCategory::Command,
            command: "uv".to_string(),
            args: vec!["--version".to_string()],
            missing_status: DoctorCheckStatus::Warn,
        })
        .expect("command check");
    assert_eq!(command_check.category, DoctorCheckCategory::Command);

    let runtime_checks = ports
        .runtime_checks(
            &layout,
            Some(&runtime),
            Some(&state),
            DoctorExecutionMode::Observational,
        )
        .expect("runtime checks");
    assert_eq!(runtime_checks[0].name, "python version");

    let capability_checks = ports
        .capability_checks(&platform_facts(), &machine_capabilities())
        .expect("capability checks");
    assert_eq!(
        capability_checks[0].category,
        DoctorCheckCategory::Capability
    );

    let repair = ports
        .plan_repair(
            &DoctorReportRequest::local_cli().with_repair(DoctorRepairIntent::DeveloperPythonEnv),
        )
        .expect("repair plan");
    assert!(repair.mutates_local_state);
    assert_eq!(repair.intent, DoctorRepairIntent::DeveloperPythonEnv);
}

struct FakeDoctorPorts;

impl DoctorPathProbe for FakeDoctorPorts {
    fn check_path(&self, request: DoctorPathCheck) -> KernelResult<DoctorCheck> {
        let detail = match request.mode {
            DoctorExecutionMode::Observational => {
                format!("observed: {}", request.path.display())
            }
            DoctorExecutionMode::LocalCli => format!("writable: {}", request.path.display()),
        };
        Ok(DoctorCheck::pass(request.category, request.name, detail))
    }
}

impl DoctorCommandProbe for FakeDoctorPorts {
    fn check_command(&self, request: DoctorCommandCheck) -> KernelResult<DoctorCheck> {
        Ok(DoctorCheck::with_status(
            request.category,
            request.name,
            request.missing_status,
            format!("{} {}", request.command, request.args.join(" ")),
        ))
    }
}

impl DoctorRuntimeCheckMapper for FakeDoctorPorts {
    fn runtime_checks(
        &self,
        _layout: &RuntimeLayout,
        _runtime: Option<&PythonRuntimeLayout>,
        state: Option<&RuntimeInitState>,
        _mode: DoctorExecutionMode,
    ) -> KernelResult<Vec<DoctorCheck>> {
        let detail = state
            .and_then(|state| state.python.version.clone())
            .unwrap_or_else(|| "unknown".to_string());
        Ok(vec![DoctorCheck::pass(
            DoctorCheckCategory::Runtime,
            "python version",
            detail,
        )])
    }
}

impl DoctorCapabilityCheckMapper for FakeDoctorPorts {
    fn capability_checks(
        &self,
        _platform: &PlatformFacts,
        capabilities: &MachineCapabilities,
    ) -> KernelResult<Vec<DoctorCheck>> {
        Ok(capabilities
            .backends
            .iter()
            .map(|backend| {
                DoctorCheck::with_status(
                    DoctorCheckCategory::Capability,
                    format!("backend {:?}", backend.backend),
                    DoctorCheckStatus::Pass,
                    format!("{:?}", backend.state),
                )
            })
            .collect())
    }
}

impl DoctorRepairPlanner for FakeDoctorPorts {
    fn plan_repair(&self, request: &DoctorReportRequest) -> KernelResult<DoctorRepairPlan> {
        Ok(DoctorRepairPlan {
            intent: request.repair,
            mutates_local_state: request.repair.mutates_local_state(),
            steps: vec![DoctorRepairStep {
                label: "sync developer Python env".to_string(),
                command: Some("uv --no-config sync".to_string()),
                detail: "developer-only repair".to_string(),
            }],
        })
    }
}

fn runtime_layout(root: &str) -> RuntimeLayout {
    let root = PathBuf::from(root);
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
            metal: Some(MetalFacts { visible: true }),
        },
    }
}

fn machine_capabilities() -> MachineCapabilities {
    MachineCapabilities {
        schema_version: 1,
        generated_at: None,
        platform: platform_facts(),
        runtime: RuntimeCapabilityState {
            home_dir: PathBuf::from("/tmp/tentgent-home"),
            python_env_dir: PathBuf::from("/tmp/tentgent-home/runtime/python-env"),
            profiles: Vec::new(),
        },
        backends: vec![BackendCapability {
            backend: BackendKind::Mlx,
            state: CapabilityState::Ready,
            message: None,
            next_step: None,
        }],
    }
}
