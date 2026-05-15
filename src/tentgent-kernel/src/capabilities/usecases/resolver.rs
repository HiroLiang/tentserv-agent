//! Machine capability resolution use case.

use crate::capabilities::domain::MachineCapabilities;
use crate::capabilities::ports::{
    CapabilityStateStore, MachineCapabilitiesProbe, MachineCapabilitiesResolver,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput, RuntimeLayoutResolver};
use crate::foundation::platform::{PlatformFacts, PlatformProbe};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineCapabilitiesInput {
    pub layout: RuntimeLayoutInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineCapabilitiesSnapshot {
    pub layout: RuntimeLayout,
    pub platform: PlatformFacts,
    pub capabilities: MachineCapabilities,
}

pub struct StdMachineCapabilitiesResolver<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    platform_probe: &'a dyn PlatformProbe,
    state_store: &'a dyn CapabilityStateStore,
    capabilities_probe: &'a dyn MachineCapabilitiesProbe,
}

impl<'a> StdMachineCapabilitiesResolver<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        platform_probe: &'a dyn PlatformProbe,
        state_store: &'a dyn CapabilityStateStore,
        capabilities_probe: &'a dyn MachineCapabilitiesProbe,
    ) -> Self {
        Self {
            layout_resolver,
            platform_probe,
            state_store,
            capabilities_probe,
        }
    }
}

impl MachineCapabilitiesResolver for StdMachineCapabilitiesResolver<'_> {
    fn current(
        &self,
        input: MachineCapabilitiesInput,
    ) -> KernelResult<MachineCapabilitiesSnapshot> {
        let layout = self.layout_resolver.resolve(input.layout)?;
        let platform = self.platform_probe.probe()?;
        let capabilities = match self.state_store.load(&layout)? {
            Some(capabilities) => capabilities,
            None => self.capabilities_probe.probe(&layout, &platform)?,
        };

        Ok(MachineCapabilitiesSnapshot {
            layout,
            platform,
            capabilities,
        })
    }

    fn refresh(
        &self,
        input: MachineCapabilitiesInput,
    ) -> KernelResult<MachineCapabilitiesSnapshot> {
        let layout = self.layout_resolver.resolve(input.layout)?;
        let platform = self.platform_probe.probe()?;
        let capabilities = self.capabilities_probe.probe(&layout, &platform)?;
        self.state_store.save(&layout, &capabilities)?;

        Ok(MachineCapabilitiesSnapshot {
            layout,
            platform,
            capabilities,
        })
    }
}
