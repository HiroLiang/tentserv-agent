//! Doctor diagnostics package ports.

use crate::capabilities::domain::MachineCapabilities;
use crate::features::runtime::domain::{PythonRuntimeLayout, RuntimeInitState};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;
use crate::foundation::platform::PlatformFacts;

use super::domain::{
    DoctorCheck, DoctorCommandCheck, DoctorExecutionMode, DoctorPathCheck, DoctorRepairPlan,
    DoctorReportRequest,
};

pub trait DoctorPathProbe {
    fn check_path(&self, request: DoctorPathCheck) -> KernelResult<DoctorCheck>;
}

pub trait DoctorCommandProbe {
    fn check_command(&self, request: DoctorCommandCheck) -> KernelResult<DoctorCheck>;
}

pub trait DoctorRuntimeCheckMapper {
    fn runtime_checks(
        &self,
        layout: &RuntimeLayout,
        runtime: Option<&PythonRuntimeLayout>,
        state: Option<&RuntimeInitState>,
        mode: DoctorExecutionMode,
    ) -> KernelResult<Vec<DoctorCheck>>;
}

pub trait DoctorCapabilityCheckMapper {
    fn capability_checks(
        &self,
        platform: &PlatformFacts,
        capabilities: &MachineCapabilities,
    ) -> KernelResult<Vec<DoctorCheck>>;
}

pub trait DoctorRepairPlanner {
    fn plan_repair(&self, request: &DoctorReportRequest) -> KernelResult<DoctorRepairPlan>;
}
