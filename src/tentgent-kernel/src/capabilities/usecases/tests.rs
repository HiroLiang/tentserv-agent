use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::capabilities::domain::{
    BackendCapability, BackendKind, CapabilityState, MachineCapabilities, RuntimeCapabilityState,
    RuntimeProfileCapability, CAPABILITY_SCHEMA_VERSION,
};
use crate::capabilities::infra::{
    FileCapabilityStateStore, StdCapabilityChecker, StdMachineCapabilitiesProbe,
};
use crate::capabilities::ports::CapabilityStateStore;
use crate::capabilities::usecases::{
    CapabilityGate, MachineCapabilitiesInput, MachineCapabilitiesResolver, StdCapabilityGate,
    StdMachineCapabilitiesResolver,
};
use crate::features::runtime::domain::{BootstrapProfile, RuntimeReadiness};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
};
use crate::foundation::platform::{
    Architecture, CpuFacts, GpuFacts, OperatingSystem, PlatformFacts, PlatformProbe,
};

#[test]
fn resolver_current_returns_cached_snapshot_without_reprobing_capabilities() {
    let layout = test_layout("resolver-current");
    let cached = MachineCapabilities {
        schema_version: CAPABILITY_SCHEMA_VERSION,
        generated_at: Some("2026-05-15T00:00:00Z".to_string()),
        platform: linux_platform(),
        runtime: RuntimeCapabilityState {
            home_dir: layout.home_dir.clone(),
            python_env_dir: layout.python_env_dir.clone(),
            profiles: Vec::new(),
        },
        backends: Vec::new(),
    };
    FileCapabilityStateStore
        .save(&layout, &cached)
        .expect("save cached capability state");

    let platform_probe = StaticPlatformProbe {
        facts: linux_platform(),
    };
    let resolver = StdMachineCapabilitiesResolver::new(
        &StdRuntimeLayoutResolver,
        &platform_probe,
        &FileCapabilityStateStore,
        &StdMachineCapabilitiesProbe,
    );
    let snapshot = resolver
        .current(machine_capabilities_input(&layout))
        .expect("resolve current capability snapshot");

    assert!(snapshot.layout.capabilities_path.exists());
    assert_eq!(snapshot.platform, linux_platform());
    assert_eq!(snapshot.capabilities, cached);
}

#[test]
fn resolver_refresh_probes_and_saves_snapshot() {
    let layout = test_layout("resolver-refresh");
    let platform_probe = StaticPlatformProbe {
        facts: linux_platform(),
    };
    let resolver = StdMachineCapabilitiesResolver::new(
        &StdRuntimeLayoutResolver,
        &platform_probe,
        &FileCapabilityStateStore,
        &StdMachineCapabilitiesProbe,
    );
    let snapshot = resolver
        .refresh(machine_capabilities_input(&layout))
        .expect("refresh capability snapshot");
    let saved = FileCapabilityStateStore
        .load(&snapshot.layout)
        .expect("load refreshed capability state")
        .expect("refreshed capability state exists");

    assert_eq!(saved, snapshot.capabilities);
}

#[test]
fn gate_allows_ready_profile_and_rejects_missing_backend() {
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
                message: None,
                next_step: None,
            }],
        },
        backends: vec![BackendCapability {
            backend: BackendKind::CpuGguf,
            state: CapabilityState::Missing,
            message: None,
            next_step: Some("install dependency".to_string()),
        }],
    };
    let gate = StdCapabilityGate::new(&StdCapabilityChecker);

    gate.ensure_runtime_profile(&capabilities, BootstrapProfile::Base)
        .expect("ready profile");
    let error = gate
        .ensure_backend(&capabilities, BackendKind::CpuGguf)
        .expect_err("missing backend is rejected");

    match error {
        KernelError::BackendCapabilityNotReady { backend, next_step } => {
            assert_eq!(backend, "CpuGguf");
            assert_eq!(next_step, "install dependency");
        }
        other => panic!("unexpected error: {other}"),
    }
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

fn machine_capabilities_input(
    layout: &crate::foundation::layout::RuntimeLayout,
) -> MachineCapabilitiesInput {
    MachineCapabilitiesInput {
        layout: RuntimeLayoutInput {
            mode: LayoutResolveMode::ReadOnly,
            home_dir: Some(layout.home_dir.clone()),
            data_root_dir: Some(layout.data_root_dir.clone()),
        },
        runtime: None,
    }
}

fn temp_path(label: &str, root: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "tentgent-kernel-capabilities-usecase-{label}-{root}-{}-{nanos}",
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

struct StaticPlatformProbe {
    facts: PlatformFacts,
}

impl PlatformProbe for StaticPlatformProbe {
    fn probe(&self) -> KernelResult<PlatformFacts> {
        Ok(self.facts.clone())
    }
}
