use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::capabilities::domain::{
    BackendCapability, BackendKind, CapabilityState, MachineCapabilities, RuntimeCapabilityState,
    RuntimeProfileCapability, CAPABILITY_SCHEMA_VERSION,
};
use crate::capabilities::infra::{
    FileCapabilityStateStore, StdCapabilityChecker, StdMachineCapabilitiesProbe,
};
use crate::capabilities::ports::{
    CapabilityChecker, CapabilityStateStore, MachineCapabilitiesProbe,
};
use crate::features::runtime::domain::{BootstrapProfile, RuntimeReadiness};
use crate::features::runtime::domain::{PythonRuntimeLayout, PythonRuntimeSource};
use crate::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
};
use crate::foundation::platform::{
    Architecture, CpuFacts, GpuFacts, OperatingSystem, PlatformFacts,
};

#[test]
fn file_store_round_trips_capability_state() {
    let layout = test_layout("store-round-trip");
    let capabilities = MachineCapabilities {
        schema_version: CAPABILITY_SCHEMA_VERSION,
        generated_at: Some("2026-05-15T00:00:00Z".to_string()),
        platform: linux_platform(),
        runtime: RuntimeCapabilityState {
            home_dir: layout.home_dir.clone(),
            python_env_dir: layout.python_env_dir.clone(),
            profiles: vec![RuntimeProfileCapability {
                profile: BootstrapProfile::Base,
                readiness: RuntimeReadiness::Ready,
                message: None,
                next_step: None,
            }],
        },
        backends: vec![BackendCapability {
            backend: BackendKind::CpuGguf,
            state: CapabilityState::Ready,
            message: None,
            next_step: None,
        }],
    };

    FileCapabilityStateStore
        .save(&layout, &capabilities)
        .expect("save capability state");
    let loaded = FileCapabilityStateStore
        .load(&layout)
        .expect("load capability state")
        .expect("capability state exists");

    assert_eq!(loaded, capabilities);
}

#[test]
fn machine_capabilities_probe_uses_layout_and_platform_facts() {
    let layout = test_layout("probe");
    let capabilities = StdMachineCapabilitiesProbe
        .probe(&layout, None, &linux_platform())
        .expect("probe machine capabilities");

    assert_eq!(capabilities.schema_version, CAPABILITY_SCHEMA_VERSION);
    assert_eq!(capabilities.runtime.home_dir, layout.home_dir);
    assert_eq!(capabilities.runtime.python_env_dir, layout.python_env_dir);
    assert_eq!(
        capabilities
            .runtime
            .profiles
            .iter()
            .find(|profile| profile.profile == BootstrapProfile::Base)
            .expect("base profile")
            .readiness,
        RuntimeReadiness::Missing
    );
    assert_eq!(
        capabilities
            .backends
            .iter()
            .find(|backend| backend.backend == BackendKind::Mlx)
            .expect("mlx backend")
            .state,
        CapabilityState::Unsupported
    );
}

#[test]
fn machine_capabilities_probe_uses_selected_runtime_env_when_available() {
    let layout = test_layout("probe-runtime-env");
    let runtime = PythonRuntimeLayout {
        project_dir: layout.home_dir.join("python"),
        env_dir: layout.home_dir.join("selected-env"),
        source: PythonRuntimeSource::DevelopmentSource,
    };
    std::fs::create_dir_all(&runtime.env_dir).expect("create selected env");
    let python_dir = if cfg!(windows) {
        runtime.env_dir.join("Scripts")
    } else {
        runtime.env_dir.join("bin")
    };
    std::fs::create_dir_all(&python_dir).expect("create python bin dir");
    std::fs::write(
        python_dir.join(if cfg!(windows) {
            "python.exe"
        } else {
            "python"
        }),
        "",
    )
    .expect("write python binary marker");

    let capabilities = StdMachineCapabilitiesProbe
        .probe(&layout, Some(&runtime), &linux_platform())
        .expect("probe machine capabilities");

    assert_eq!(capabilities.runtime.python_env_dir, runtime.env_dir);
    assert_eq!(
        capabilities
            .runtime
            .profiles
            .iter()
            .find(|profile| profile.profile == BootstrapProfile::Base)
            .expect("base profile")
            .readiness,
        RuntimeReadiness::Ready
    );
}

#[cfg(unix)]
#[test]
fn machine_capabilities_probe_maps_backend_import_checks() {
    let layout = test_layout("probe-backend-imports");
    let runtime = PythonRuntimeLayout {
        project_dir: layout.home_dir.join("python"),
        env_dir: layout.home_dir.join("selected-env"),
        source: PythonRuntimeSource::DevelopmentSource,
    };
    let bin_dir = runtime.env_dir.join("bin");
    std::fs::create_dir_all(&bin_dir).expect("create selected env");
    write_fake_python(&bin_dir.join("python"), &["llama_cpp"]).expect("write fake python");

    let capabilities = StdMachineCapabilitiesProbe
        .probe(&layout, Some(&runtime), &macos_apple_silicon_platform())
        .expect("probe machine capabilities");

    assert_eq!(
        backend_state(&capabilities, BackendKind::CpuGguf),
        CapabilityState::Missing
    );
    assert_eq!(
        backend_state(&capabilities, BackendKind::SafetensorsPeft),
        CapabilityState::Ready
    );
    assert_eq!(
        backend_state(&capabilities, BackendKind::Embedding),
        CapabilityState::Ready
    );
    assert_eq!(
        backend_state(&capabilities, BackendKind::Mlx),
        CapabilityState::Ready
    );
    assert_eq!(
        backend_state(&capabilities, BackendKind::Training),
        CapabilityState::Ready
    );
}

#[test]
fn checker_maps_cached_runtime_and_backend_state() {
    let capabilities = MachineCapabilities {
        schema_version: CAPABILITY_SCHEMA_VERSION,
        generated_at: None,
        platform: linux_platform(),
        runtime: RuntimeCapabilityState {
            home_dir: PathBuf::from("/tmp/tentgent-home"),
            python_env_dir: PathBuf::from("/tmp/tentgent-home/runtime/python-env"),
            profiles: vec![RuntimeProfileCapability {
                profile: BootstrapProfile::Base,
                readiness: RuntimeReadiness::Ready,
                message: Some("ready".to_string()),
                next_step: None,
            }],
        },
        backends: vec![BackendCapability {
            backend: BackendKind::CpuGguf,
            state: CapabilityState::Missing,
            message: Some("missing dependency".to_string()),
            next_step: Some("install dependency".to_string()),
        }],
    };

    let backend = StdCapabilityChecker
        .check_backend(&capabilities, BackendKind::CpuGguf)
        .expect("check backend");
    assert_eq!(backend.state, CapabilityState::Missing);
    assert_eq!(backend.next_step.as_deref(), Some("install dependency"));

    let profile = StdCapabilityChecker
        .check_runtime_profile(&capabilities, BootstrapProfile::Base)
        .expect("check profile");
    assert_eq!(profile.state, CapabilityState::Ready);
    assert_eq!(profile.message.as_deref(), Some("ready"));
}

fn backend_state(capabilities: &MachineCapabilities, backend: BackendKind) -> CapabilityState {
    capabilities
        .backends
        .iter()
        .find(|candidate| candidate.backend == backend)
        .expect("backend capability")
        .state
}

fn test_layout(label: &str) -> crate::foundation::layout::RuntimeLayout {
    StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: Some(temp_path(label, "home")),
            data_root_dir: Some(temp_path(label, "data")),
        })
        .expect("resolve test layout")
}

fn temp_path(label: &str, root: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "tentgent-kernel-capabilities-{label}-{root}-{}-{nanos}",
        std::process::id()
    ))
}

fn linux_platform() -> PlatformFacts {
    PlatformFacts {
        os: OperatingSystem::Linux,
        arch: Architecture::X86_64,
        libc: None,
        cpu: CpuFacts {
            vendor: Some("GenuineIntel".to_string()),
            brand: Some("fixture cpu".to_string()),
            features: vec!["avx2".to_string()],
        },
        gpu: GpuFacts {
            devices: Vec::new(),
            cuda: None,
            metal: None,
        },
    }
}

fn macos_apple_silicon_platform() -> PlatformFacts {
    PlatformFacts {
        os: OperatingSystem::Macos,
        arch: Architecture::Aarch64,
        libc: None,
        cpu: CpuFacts {
            vendor: Some("Apple".to_string()),
            brand: Some("fixture cpu".to_string()),
            features: Vec::new(),
        },
        gpu: GpuFacts {
            devices: Vec::new(),
            cuda: None,
            metal: None,
        },
    }
}

#[cfg(unix)]
fn write_fake_python(path: &std::path::Path, missing_modules: &[&str]) -> std::io::Result<()> {
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
    std::fs::write(
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
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions)
}
