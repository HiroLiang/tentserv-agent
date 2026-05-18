use crate::features::model::domain::ModelFormat;
use crate::features::train::domain::{select_backend, LoraTrainBackend, LoraTrainBackendRequest};
use crate::foundation::platform::{
    Architecture, CpuFacts, GpuFacts, OperatingSystem, PlatformFacts,
};

#[test]
fn safetensors_auto_selects_peft_backend() {
    let (backend, reason, blockers) = select_backend(
        &linux_platform(),
        ModelFormat::Safetensors,
        LoraTrainBackendRequest::Auto,
    );

    assert_eq!(backend, Some(LoraTrainBackend::Peft));
    assert!(reason.contains("safetensors"));
    assert!(blockers.is_empty());
}

fn linux_platform() -> PlatformFacts {
    PlatformFacts {
        os: OperatingSystem::Linux,
        arch: Architecture::X86_64,
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
