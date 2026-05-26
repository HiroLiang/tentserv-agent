use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::features::runtime::domain::{
    BootstrapProfile, BootstrapRuntimeInput, PythonRuntimeLayout, PythonRuntimeResolutionInput,
    PythonRuntimeSource, RuntimeEntrypoint, RuntimeReadiness,
};
use crate::features::runtime::ports::{
    PythonRuntimeResolver, RuntimeBootstrapPlanner, RuntimeExecutableResolver, RuntimeStateProbe,
};
use crate::foundation::error::KernelError;
use crate::foundation::layout::RuntimeLayout;
use crate::foundation::platform::{
    Architecture, CpuFacts, GpuFacts, LibcFacts, OperatingSystem, PlatformFacts,
};

use super::path::normalize_existing_path;
use super::{
    ModelRuntimeDaemonLaunchPolicy, StdPythonRuntimeResolver, StdRuntimeBootstrapPlanner,
    StdRuntimeExecutableResolver, StdRuntimeStateProbe,
};

#[test]
fn std_python_runtime_resolver_uses_explicit_project_and_env() {
    let root = temp_path("runtime-resolver-explicit");
    let project_dir = root.join("python");
    let env_dir = root.join("env");
    fs::create_dir_all(&project_dir).expect("create project dir");
    fs::create_dir_all(&env_dir).expect("create env dir");
    fs::write(
        project_dir.join("pyproject.toml"),
        "[project]\nname = \"tentgent-model-runtime\"\n",
    )
    .expect("write pyproject");
    let layout = runtime_layout(&root.join("home"));

    let runtime = StdPythonRuntimeResolver
        .resolve_python_runtime(
            &layout,
            PythonRuntimeResolutionInput {
                project_dir: Some(project_dir.clone()),
                python_env_dir: Some(env_dir.clone()),
            },
        )
        .expect("resolve runtime");

    assert_eq!(runtime.project_dir, normalize_existing_path(project_dir));
    assert_eq!(runtime.env_dir, normalize_existing_path(env_dir));
    assert_eq!(runtime.source, PythonRuntimeSource::EnvironmentOverride);
}

#[test]
fn std_runtime_executable_resolver_uses_platform_bin_layout() {
    let runtime = PythonRuntimeLayout {
        project_dir: PathBuf::from("/opt/tentgent/python"),
        env_dir: PathBuf::from("/var/tentgent/python-env"),
        source: PythonRuntimeSource::InstalledPrefix,
    };

    let python = StdRuntimeExecutableResolver
        .python_binary_path(&runtime)
        .expect("resolve python");
    let model_runtime = StdRuntimeExecutableResolver
        .entrypoint_path(&runtime, RuntimeEntrypoint::ModelRuntimeDaemon)
        .expect("resolve model runtime daemon");

    if cfg!(windows) {
        assert!(python.ends_with("Scripts/python.exe"));
        assert!(model_runtime.ends_with("Scripts/tentgent-model-runtime-daemon.exe"));
    } else {
        assert!(python.ends_with("bin/python"));
        assert!(model_runtime.ends_with("bin/tentgent-model-runtime-daemon"));
    }
}

#[test]
fn model_runtime_launch_policy_overrides_idle_keep_alive_only() {
    let policy = ModelRuntimeDaemonLaunchPolicy::with_idle_keep_alive_seconds(30);

    assert_eq!(policy.idle_keep_alive_seconds, "30");
    assert_eq!(policy.model_idle_timeout_seconds, "-1");
}

#[test]
fn std_bootstrap_planner_uses_runtime_layout_platform_and_installed_script() {
    let root = temp_path("runtime-bootstrap-planner");
    let project_dir = root.join("share/tentgent/python");
    let script_dir = root.join("share/tentgent/scripts");
    let script_path = script_dir.join("bootstrap-python-env.sh");
    fs::create_dir_all(&project_dir).expect("create project dir");
    fs::create_dir_all(&script_dir).expect("create script dir");
    fs::write(
        project_dir.join("pyproject.toml"),
        "[project]\nname = \"tentgent-model-runtime\"\n",
    )
    .expect("write pyproject");
    fs::write(&script_path, "#!/usr/bin/env bash\n").expect("write bootstrap script");

    let layout = runtime_layout(&root.join("home"));
    let runtime = PythonRuntimeLayout {
        project_dir: project_dir.clone(),
        env_dir: root.join("runtime/python-env"),
        source: PythonRuntimeSource::InstalledPrefix,
    };
    let uv_path = root.join("uv");

    let plan = StdRuntimeBootstrapPlanner
        .plan_bootstrap(
            &layout,
            &runtime,
            &platform_facts(OperatingSystem::Macos),
            BootstrapRuntimeInput {
                project_dir: None,
                python_env_dir: None,
                uv_path: Some(uv_path.clone()),
                profile: BootstrapProfile::Training,
                dry_run: true,
                print_plan: true,
            },
        )
        .expect("plan bootstrap");

    assert_eq!(plan.project_dir, project_dir);
    assert_eq!(plan.python_env_dir, runtime.env_dir);
    assert_eq!(plan.script_path, normalize_existing_path(script_path));
    assert_eq!(plan.uv_path, Some(uv_path));
    assert_eq!(plan.profile, BootstrapProfile::Training);
    assert!(plan.dry_run);
    assert!(plan.print_plan);
}

#[test]
fn std_bootstrap_planner_marks_windows_shell_bootstrap_unsupported() {
    let root = temp_path("runtime-bootstrap-windows");
    let project_dir = root.join("share/tentgent/python");
    let script_dir = root.join("share/tentgent/scripts");
    fs::create_dir_all(&project_dir).expect("create project dir");
    fs::create_dir_all(&script_dir).expect("create script dir");
    fs::write(
        project_dir.join("pyproject.toml"),
        "[project]\nname = \"tentgent-model-runtime\"\n",
    )
    .expect("write pyproject");
    fs::write(
        script_dir.join("bootstrap-python-env.sh"),
        "#!/usr/bin/env bash\n",
    )
    .expect("write bootstrap script");

    let layout = runtime_layout(&root.join("home"));
    let runtime = PythonRuntimeLayout {
        project_dir,
        env_dir: root.join("runtime/python-env"),
        source: PythonRuntimeSource::InstalledPrefix,
    };

    let result = StdRuntimeBootstrapPlanner.plan_bootstrap(
        &layout,
        &runtime,
        &platform_facts(OperatingSystem::Windows),
        BootstrapRuntimeInput {
            project_dir: None,
            python_env_dir: None,
            uv_path: None,
            profile: BootstrapProfile::Base,
            dry_run: false,
            print_plan: false,
        },
    );

    assert!(matches!(result, Err(KernelError::UnsupportedTarget(_))));
}

#[test]
fn std_runtime_state_probe_reports_missing_runtime_without_mutation() {
    let root = temp_path("runtime-state-missing");
    let layout = runtime_layout(&root.join("home"));

    let state = StdRuntimeStateProbe
        .probe_runtime_state(&layout, None)
        .expect("probe state");

    assert_eq!(state.python_env_dir, layout.python_env_dir);
    assert!(!state.python.env_exists);
    assert_eq!(state.python.version, None);
    assert_eq!(state.profiles[0].readiness, RuntimeReadiness::Missing);
}

#[cfg(unix)]
#[test]
fn std_runtime_state_probe_marks_profiles_ready_when_dependencies_import() {
    let root = temp_path("runtime-state-dependencies");
    let env_dir = root.join("env");
    let bin_dir = env_dir.join("bin");
    fs::create_dir_all(&bin_dir).expect("create bin dir");
    write_fake_python(&bin_dir.join("python"), &[]).expect("write fake python");
    let layout = runtime_layout(&root.join("home"));
    let runtime = PythonRuntimeLayout {
        project_dir: root.join("python"),
        env_dir,
        source: PythonRuntimeSource::DevelopmentSource,
    };

    let state = StdRuntimeStateProbe
        .probe_runtime_state(&layout, Some(&runtime))
        .expect("probe state");

    assert_eq!(state.python.version.as_deref(), Some("Python 3.13.11"));
    assert!(state
        .profiles
        .iter()
        .all(|profile| profile.readiness == RuntimeReadiness::Ready));
}

fn platform_facts(os: OperatingSystem) -> PlatformFacts {
    PlatformFacts {
        os,
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

fn runtime_layout(root: &Path) -> RuntimeLayout {
    RuntimeLayout {
        home_dir: root.to_path_buf(),
        data_root_dir: root.to_path_buf(),
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

#[cfg(unix)]
fn write_fake_python(path: &Path, missing_modules: &[&str]) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let missing_cases = missing_modules
        .iter()
        .map(|module| {
            format!(
                r#""{module}") echo "{module} (ModuleNotFoundError: No module named {module})"; status=1 ;;"#
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        path,
        format!(
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "Python 3.13.11"
  exit 0
fi
if [ "$1" = "-c" ]; then
  shift
  shift
  status=0
  for module in "$@"; do
    case "$module" in
{missing_cases}
      *) ;;
    esac
  done
  exit "$status"
fi
exit 0
"#
        ),
    )?;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
}

fn temp_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time before epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("tentgent-kernel-{label}-{nanos}"))
}
