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

/// Probes filesystem paths and maps the observation into a doctor check.
pub trait DoctorPathProbe {
    /// Checks one path expectation such as the required directory, file, or executable.
    fn check_path(&self, request: DoctorPathCheck) -> KernelResult<DoctorCheck>;
}

/// Probes external command availability or version output for diagnostics.
pub trait DoctorCommandProbe {
    /// Checks one command invocation and maps missing or failed commands into doctor status.
    fn check_command(&self, request: DoctorCommandCheck) -> KernelResult<DoctorCheck>;
}

/// Maps runtime layout and initialization facts into doctor diagnostic checks.
pub trait DoctorRuntimeCheckMapper {
    /// Builds runtime checks without bootstrapping or repairing the runtime.
    fn runtime_checks(
        &self,
        layout: &RuntimeLayout,
        runtime: Option<&PythonRuntimeLayout>,
        state: Option<&RuntimeInitState>,
        mode: DoctorExecutionMode,
    ) -> KernelResult<Vec<DoctorCheck>>;
}

/// Maps platform and capability facts into doctor diagnostic checks.
pub trait DoctorCapabilityCheckMapper {
    /// Builds capability checks without reproving or mutating the stored capability snapshot.
    fn capability_checks(
        &self,
        platform: &PlatformFacts,
        capabilities: &MachineCapabilities,
    ) -> KernelResult<Vec<DoctorCheck>>;
}

/// Plans explicit doctor repair actions separately from observational checks.
pub trait DoctorRepairPlanner {
    /// Returns repair steps for a request; report-only requests must not imply mutation.
    fn plan_repair(&self, request: &DoctorReportRequest) -> KernelResult<DoctorRepairPlan>;
}
