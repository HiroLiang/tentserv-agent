//! Query current platform facts.

use crate::foundation::error::KernelResult;
use crate::foundation::platform::domain::PlatformFacts;
use crate::foundation::platform::probe::PlatformProbe;

pub struct QueryPlatformFacts<'a, P> {
    pub probe: &'a P,
}

pub trait PlatformFactsQuery {
    fn run(&self) -> KernelResult<PlatformFacts>;
}

impl<'a, P> QueryPlatformFacts<'a, P> {
    pub fn new(probe: &'a P) -> Self {
        Self { probe }
    }
}

impl<P> QueryPlatformFacts<'_, P>
where
    P: PlatformProbe,
{
    pub fn run(&self) -> KernelResult<PlatformFacts> {
        self.probe.query_platform_facts()
    }
}

impl<P> PlatformFactsQuery for QueryPlatformFacts<'_, P>
where
    P: PlatformProbe,
{
    fn run(&self) -> KernelResult<PlatformFacts> {
        self.probe.query_platform_facts()
    }
}

#[cfg(test)]
mod tests {
    use crate::foundation::error::KernelResult;
    use crate::foundation::platform::domain::{
        Architecture, CpuFacts, GpuFacts, OperatingSystem, PlatformFacts,
    };
    use crate::foundation::platform::probe::{PlatformProbe, StdPlatformProbe};

    use super::QueryPlatformFacts;

    #[derive(Debug)]
    struct FakePlatformProbe {
        facts: PlatformFacts,
    }

    impl PlatformProbe for FakePlatformProbe {
        fn query_platform_facts(&self) -> KernelResult<PlatformFacts> {
            Ok(self.facts.clone())
        }
    }

    #[test]
    fn returns_probe_facts() {
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
        let output = QueryPlatformFacts::new(&probe).run().expect("query facts");

        assert_eq!(output, facts);
    }

    #[test]
    fn std_probe_detects_current_platform() {
        let facts = QueryPlatformFacts::new(&StdPlatformProbe)
            .run()
            .expect("query current platform facts");
        eprintln!("{facts:#?}");

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
}
