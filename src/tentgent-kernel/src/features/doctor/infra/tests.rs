use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::capabilities::domain::{
    BackendCapability, BackendKind, CapabilityState, MachineCapabilities, RuntimeCapabilityState,
    RuntimeProfileCapability,
};
use crate::features::doctor::domain::{
    DoctorCheckCategory, DoctorCheckStatus, DoctorCommandCheck, DoctorExecutionMode,
    DoctorPathCheck, DoctorPathExpectation, DoctorRepairIntent, DoctorReportRequest,
};
use crate::features::doctor::ports::{
    DoctorCapabilityCheckMapper, DoctorCommandProbe, DoctorPathProbe, DoctorRepairPlanner,
    DoctorRuntimeCheckMapper,
};
use crate::features::runtime::domain::{
    BootstrapProfile, PythonRuntimeLayout, PythonRuntimeSource, PythonRuntimeState,
    RuntimeInitState, RuntimeProfileState, RuntimeReadiness,
};
use crate::foundation::layout::RuntimeLayout;
use crate::foundation::platform::{
    Architecture, CpuFacts, GpuFacts, LibcFacts, OperatingSystem, PlatformFacts,
};

use super::{
    StdDoctorCapabilityCheckMapper, StdDoctorCommandProbe, StdDoctorPathProbe,
    StdDoctorRepairPlanner, StdDoctorRuntimeCheckMapper,
};

#[test]
fn path_probe_checks_observational_and_local_cli_directory_modes() {
    let root = temp_path("path-probe");
    fs::create_dir_all(&root).expect("create temp dir");

    let observed = StdDoctorPathProbe
        .check_path(DoctorPathCheck {
            name: "runtime home".to_string(),
            category: DoctorCheckCategory::RuntimeHome,
            path: root.clone(),
            expectation: DoctorPathExpectation::RequiredDirectory,
            mode: DoctorExecutionMode::Observational,
        })
        .expect("check observed path");
    let writable = StdDoctorPathProbe
        .check_path(DoctorPathCheck {
            name: "runtime home".to_string(),
            category: DoctorCheckCategory::RuntimeHome,
            path: root.clone(),
            expectation: DoctorPathExpectation::RequiredDirectory,
            mode: DoctorExecutionMode::LocalCli,
        })
        .expect("check writable path");
    let optional_missing = StdDoctorPathProbe
        .check_path(DoctorPathCheck {
            name: "bootstrap cache".to_string(),
            category: DoctorCheckCategory::Bootstrap,
            path: root.join("missing"),
            expectation: DoctorPathExpectation::OptionalDirectory,
            mode: DoctorExecutionMode::Observational,
        })
        .expect("check optional path");

    assert_eq!(observed.status, DoctorCheckStatus::Pass);
    assert_eq!(writable.status, DoctorCheckStatus::Pass);
    assert_eq!(optional_missing.status, DoctorCheckStatus::Skipped);
}

#[test]
fn command_probe_maps_missing_command_to_requested_status() {
    let check = StdDoctorCommandProbe
        .check_command(DoctorCommandCheck {
            name: "missing command".to_string(),
            category: DoctorCheckCategory::Command,
            command: format!("tentgent-missing-command-{}", std::process::id()),
            args: Vec::new(),
            missing_status: DoctorCheckStatus::Warn,
        })
        .expect("check command");

    assert_eq!(check.status, DoctorCheckStatus::Warn);
    assert!(check.detail.contains("not available on PATH"));
}

#[test]
fn runtime_mapper_builds_checks_from_runtime_and_state_without_bootstrap() {
    let root = temp_path("runtime-mapper");
    let project_dir = root.join("python");
    let env_dir = root.join("env");
    let bin_dir = env_dir.join(if cfg!(windows) { "Scripts" } else { "bin" });
    fs::create_dir_all(project_dir.join("src")).expect("create python package");
    fs::create_dir_all(&bin_dir).expect("create bin dir");
    fs::write(
        project_dir.join("pyproject.toml"),
        "[project]\nname = \"tentgent-model-runtime\"\n",
    )
    .expect("write pyproject");
    fs::write(bin_dir.join(script_name("python")), "").expect("write python");
    fs::write(
        bin_dir.join(script_name("tentgent-model-runtime-daemon")),
        "",
    )
    .expect("write model runtime entrypoint");

    let layout = runtime_layout(root.join("home"));
    let runtime = PythonRuntimeLayout {
        project_dir,
        env_dir: env_dir.clone(),
        source: PythonRuntimeSource::DevelopmentSource,
    };
    let state = RuntimeInitState {
        home_dir: layout.home_dir.clone(),
        python_env_dir: env_dir.clone(),
        bootstrap_dir: layout.bootstrap_dir.clone(),
        uv_cache_dir: layout.bootstrap_uv_cache_dir.clone(),
        python: PythonRuntimeState {
            env_exists: true,
            binary_path: bin_dir.join(script_name("python")),
            version: Some("Python 3.13.11".to_string()),
        },
        profiles: vec![RuntimeProfileState {
            profile: BootstrapProfile::Base,
            readiness: RuntimeReadiness::Ready,
            message: Some("base runtime is ready".to_string()),
        }],
    };

    let checks = StdDoctorRuntimeCheckMapper
        .runtime_checks(
            &layout,
            Some(&runtime),
            Some(&state),
            DoctorExecutionMode::Observational,
        )
        .expect("runtime checks");

    assert!(checks
        .iter()
        .any(|check| check.name == "python version" && check.status == DoctorCheckStatus::Pass));
    assert!(checks.iter().any(
        |check| check.name == "runtime profile base" && check.status == DoctorCheckStatus::Pass
    ));
    assert!(checks
        .iter()
        .any(|check| check.name == "entrypoint tentgent-model-runtime-daemon"));
}

#[test]
fn capability_mapper_maps_platform_profiles_and_backends() {
    let capabilities = MachineCapabilities {
        schema_version: 1,
        generated_at: Some("2026-05-17T00:00:00Z".to_string()),
        platform: platform_facts(),
        runtime: RuntimeCapabilityState {
            home_dir: PathBuf::from("/tmp/tentgent"),
            python_env_dir: PathBuf::from("/tmp/tentgent/runtime/python-env"),
            profiles: vec![
                RuntimeProfileCapability {
                    profile: BootstrapProfile::Base,
                    readiness: RuntimeReadiness::Ready,
                    message: Some("ready".to_string()),
                    next_step: None,
                },
                RuntimeProfileCapability {
                    profile: BootstrapProfile::Training,
                    readiness: RuntimeReadiness::Unknown,
                    message: Some("not probed".to_string()),
                    next_step: Some("refresh capability state".to_string()),
                },
            ],
        },
        backends: vec![
            BackendCapability {
                backend: BackendKind::Mlx,
                state: CapabilityState::Ready,
                message: None,
                next_step: None,
            },
            BackendCapability {
                backend: BackendKind::MlxVlm,
                state: CapabilityState::Ready,
                message: None,
                next_step: None,
            },
            BackendCapability {
                backend: BackendKind::MlxAudio,
                state: CapabilityState::Ready,
                message: None,
                next_step: None,
            },
            BackendCapability {
                backend: BackendKind::MlxDiffusion,
                state: CapabilityState::Ready,
                message: None,
                next_step: None,
            },
            BackendCapability {
                backend: BackendKind::Training,
                state: CapabilityState::Missing,
                message: Some("dependencies missing".to_string()),
                next_step: Some("bootstrap training profile".to_string()),
            },
        ],
    };

    let checks = StdDoctorCapabilityCheckMapper
        .capability_checks(&platform_facts(), &capabilities)
        .expect("capability checks");

    assert!(checks
        .iter()
        .any(|check| check.name == "platform" && check.status == DoctorCheckStatus::Pass));
    assert!(checks
        .iter()
        .any(|check| check.name == "backend mlx" && check.status == DoctorCheckStatus::Pass));
    assert!(checks
        .iter()
        .any(|check| check.name == "backend mlx-vlm" && check.status == DoctorCheckStatus::Pass));
    assert!(checks
        .iter()
        .any(|check| check.name == "backend mlx-audio" && check.status == DoctorCheckStatus::Pass));
    assert!(checks
        .iter()
        .any(|check| check.name == "backend mlx-diffusion"
            && check.status == DoctorCheckStatus::Pass));
    assert!(checks
        .iter()
        .any(|check| check.name == "backend training" && check.status == DoctorCheckStatus::Fail));
}

#[test]
fn repair_planner_keeps_report_only_read_only_and_delegates_explicit_fix() {
    let report_only = StdDoctorRepairPlanner
        .plan_repair(&DoctorReportRequest::observational())
        .expect("report-only plan");
    let fix = StdDoctorRepairPlanner
        .plan_repair(
            &DoctorReportRequest::local_cli().with_repair(DoctorRepairIntent::DeveloperPythonEnv),
        )
        .expect("fix plan");

    assert!(!report_only.mutates_local_state);
    assert!(report_only.steps.is_empty());
    assert!(fix.mutates_local_state);
    assert_eq!(
        fix.steps[0].command.as_deref(),
        Some("tentgent runtime bootstrap --profile base")
    );
}

fn runtime_layout(root: PathBuf) -> RuntimeLayout {
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
        libc: None::<LibcFacts>,
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

fn script_name(name: &str) -> String {
    if cfg!(windows) && !name.ends_with(".exe") {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn temp_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time before epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("tentgent-doctor-infra-{label}-{nanos}"))
}
