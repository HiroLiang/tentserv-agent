use tentgent_kernel::{
    capabilities::MachineCapabilitiesResolver,
    features::{
        doctor::{
            infra::{
                StdDoctorCapabilityCheckMapper, StdDoctorCommandProbe, StdDoctorPathProbe,
                StdDoctorRepairPlanner, StdDoctorRuntimeCheckMapper,
            },
            usecases::{DoctorReportUseCase, StdDoctorRepairUseCase, StdDoctorReportUseCase},
        },
        runtime::usecases::{RuntimeBootstrapUseCase, RuntimeStateUseCase},
    },
};

pub struct DoctorKernelComponent {
    path_probe: StdDoctorPathProbe,
    command_probe: StdDoctorCommandProbe,
    runtime_mapper: StdDoctorRuntimeCheckMapper,
    capability_mapper: StdDoctorCapabilityCheckMapper,
    repair_planner: StdDoctorRepairPlanner,
}

impl DoctorKernelComponent {
    pub fn new() -> Self {
        Self {
            path_probe: StdDoctorPathProbe,
            command_probe: StdDoctorCommandProbe,
            runtime_mapper: StdDoctorRuntimeCheckMapper,
            capability_mapper: StdDoctorCapabilityCheckMapper,
            repair_planner: StdDoctorRepairPlanner,
        }
    }

    pub fn report_usecase<'a>(
        &'a self,
        runtime_state: &'a dyn RuntimeStateUseCase,
        capabilities: &'a dyn MachineCapabilitiesResolver,
    ) -> StdDoctorReportUseCase<'a> {
        StdDoctorReportUseCase::new(
            runtime_state,
            capabilities,
            &self.path_probe,
            &self.command_probe,
            &self.runtime_mapper,
            &self.capability_mapper,
        )
    }

    pub fn repair_usecase<'a>(
        &'a self,
        runtime_bootstrap: &'a dyn RuntimeBootstrapUseCase,
        report: &'a dyn DoctorReportUseCase,
    ) -> StdDoctorRepairUseCase<'a> {
        StdDoctorRepairUseCase::new(&self.repair_planner, runtime_bootstrap, report)
    }
}

impl Default for DoctorKernelComponent {
    fn default() -> Self {
        Self::new()
    }
}
