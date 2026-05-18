use tentgent_kernel::{
    capabilities::{
        FileCapabilityStateStore, MachineCapabilitiesInput, MachineCapabilitiesResolver,
        MachineCapabilitiesSnapshot, StdCapabilityChecker, StdCapabilityGate,
        StdMachineCapabilitiesProbe, StdMachineCapabilitiesResolver,
    },
    foundation::{
        error::KernelResult, layout::StdRuntimeLayoutResolver, platform::StdPlatformProbe,
    },
};

pub struct CapabilityKernelComponent {
    layout_resolver: StdRuntimeLayoutResolver,
    platform_probe: StdPlatformProbe,
    state_store: FileCapabilityStateStore,
    capabilities_probe: StdMachineCapabilitiesProbe,
    checker: StdCapabilityChecker,
}

impl CapabilityKernelComponent {
    pub fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            platform_probe: StdPlatformProbe,
            state_store: FileCapabilityStateStore,
            capabilities_probe: StdMachineCapabilitiesProbe,
            checker: StdCapabilityChecker,
        }
    }

    pub fn resolver_usecase(&self) -> StdMachineCapabilitiesResolver<'_> {
        StdMachineCapabilitiesResolver::new(
            &self.layout_resolver,
            &self.platform_probe,
            &self.state_store,
            &self.capabilities_probe,
        )
    }

    pub fn gate_usecase(&self) -> StdCapabilityGate<'_> {
        StdCapabilityGate::new(&self.checker)
    }
}

impl Default for CapabilityKernelComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl MachineCapabilitiesResolver for CapabilityKernelComponent {
    fn current(
        &self,
        input: MachineCapabilitiesInput,
    ) -> KernelResult<MachineCapabilitiesSnapshot> {
        self.resolver_usecase().current(input)
    }

    fn refresh(
        &self,
        input: MachineCapabilitiesInput,
    ) -> KernelResult<MachineCapabilitiesSnapshot> {
        self.resolver_usecase().refresh(input)
    }
}
