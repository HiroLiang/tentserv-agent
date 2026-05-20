use crate::capabilities::domain::{BackendKind, CapabilityState, MachineCapabilities};
use crate::features::doctor::domain::{DoctorCheck, DoctorCheckCategory, DoctorCheckStatus};
use crate::features::doctor::ports::DoctorCapabilityCheckMapper;
use crate::foundation::error::KernelResult;
use crate::foundation::platform::{Architecture, OperatingSystem, PlatformFacts};

/// Maps capability snapshots into doctor diagnostic checks.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdDoctorCapabilityCheckMapper;

impl DoctorCapabilityCheckMapper for StdDoctorCapabilityCheckMapper {
    fn capability_checks(
        &self,
        platform: &PlatformFacts,
        capabilities: &MachineCapabilities,
    ) -> KernelResult<Vec<DoctorCheck>> {
        let mut checks = vec![
            DoctorCheck::pass(
                DoctorCheckCategory::Platform,
                "platform",
                platform_detail(platform),
            ),
            DoctorCheck::pass(
                DoctorCheckCategory::Capability,
                "capability schema",
                capabilities.schema_version.to_string(),
            ),
        ];

        checks.extend(capabilities.backends.iter().map(|backend| {
            DoctorCheck::with_status(
                DoctorCheckCategory::Capability,
                format!("backend {}", backend_label(backend.backend)),
                capability_status(backend.state),
                capability_detail(
                    backend.state,
                    backend.message.as_ref(),
                    backend.next_step.as_ref(),
                ),
            )
        }));

        Ok(checks)
    }
}

fn capability_status(state: CapabilityState) -> DoctorCheckStatus {
    match state {
        CapabilityState::Ready => DoctorCheckStatus::Pass,
        CapabilityState::Missing | CapabilityState::Blocked => DoctorCheckStatus::Fail,
        CapabilityState::Unsupported | CapabilityState::Stale | CapabilityState::Unknown => {
            DoctorCheckStatus::Warn
        }
    }
}

fn capability_detail(
    state: CapabilityState,
    message: Option<&String>,
    next_step: Option<&String>,
) -> String {
    match (message, next_step) {
        (Some(message), Some(next_step)) => format!("{state:?}: {message}; next step: {next_step}"),
        (Some(message), None) => format!("{state:?}: {message}"),
        (None, Some(next_step)) => format!("{state:?}; next step: {next_step}"),
        (None, None) => format!("{state:?}"),
    }
}

fn platform_detail(platform: &PlatformFacts) -> String {
    format!(
        "os={}; arch={}",
        os_label(&platform.os),
        arch_label(&platform.arch)
    )
}

fn os_label(os: &OperatingSystem) -> String {
    match os {
        OperatingSystem::Macos => "macos".to_string(),
        OperatingSystem::Linux => "linux".to_string(),
        OperatingSystem::Windows => "windows".to_string(),
        OperatingSystem::Other(value) => value.clone(),
    }
}

fn arch_label(arch: &Architecture) -> String {
    match arch {
        Architecture::Aarch64 => "aarch64".to_string(),
        Architecture::X86_64 => "x86_64".to_string(),
        Architecture::Other(value) => value.clone(),
    }
}

fn backend_label(backend: BackendKind) -> &'static str {
    match backend {
        BackendKind::CpuGguf => "cpu-gguf",
        BackendKind::SafetensorsPeft => "safetensors-peft",
        BackendKind::Mlx => "mlx",
        BackendKind::MlxVlm => "mlx-vlm",
        BackendKind::MlxAudio => "mlx-audio",
        BackendKind::Training => "training",
        BackendKind::Embedding => "embedding",
        BackendKind::Rerank => "rerank",
    }
}
