use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::capabilities::domain::{
    BackendCapability, BackendKind, CapabilityState, MachineCapabilities, RuntimeCapabilityState,
    RuntimeProfileCapability,
};
use crate::capabilities::infra::{
    FileCapabilityStateStore, StdCapabilityChecker, StdMachineCapabilitiesProbe,
};
use crate::capabilities::ports::{
    CapabilityChecker, CapabilityStateStore, MachineCapabilitiesProbe,
};
use crate::features::runtime::domain::{BootstrapProfile, RuntimeReadiness};
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
        schema_version: 1,
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
        .probe(&layout, &linux_platform())
        .expect("probe machine capabilities");

    assert_eq!(capabilities.schema_version, 1);
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
fn checker_maps_cached_runtime_and_backend_state() {
    let capabilities = MachineCapabilities {
        schema_version: 1,
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
