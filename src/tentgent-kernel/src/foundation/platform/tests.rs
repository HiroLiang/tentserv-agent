use crate::foundation::error::KernelResult;

use super::domain::{Architecture, CpuFacts, GpuFacts, OperatingSystem, PlatformFacts};
use super::infra::StdPlatformProbe;
use super::ports::PlatformProbe;

#[derive(Debug)]
struct FakePlatformProbe {
    facts: PlatformFacts,
}

impl PlatformProbe for FakePlatformProbe {
    fn probe(&self) -> KernelResult<PlatformFacts> {
        Ok(self.facts.clone())
    }
}

#[test]
fn fake_probe_returns_fixture_facts() {
    let facts = PlatformFacts {
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
    };

    let probe = FakePlatformProbe {
        facts: facts.clone(),
    };
    let output = probe.probe().expect("query fake platform facts");

    assert_eq!(output, facts);
}

#[test]
fn std_probe_detects_current_platform() {
    let facts = StdPlatformProbe
        .probe()
        .expect("query current platform facts");
    eprintln!("local platform facts detected by StdPlatformProbe:\n{facts:#?}");

    match facts.os {
        OperatingSystem::Macos | OperatingSystem::Linux | OperatingSystem::Windows => {}
        OperatingSystem::Other(ref value) => assert!(!value.is_empty()),
    }

    match facts.arch {
        Architecture::Aarch64 | Architecture::X86_64 => {}
        Architecture::Other(ref value) => assert!(!value.is_empty()),
    }

    let mut sorted = facts.cpu.features.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(facts.cpu.features, sorted);
}
