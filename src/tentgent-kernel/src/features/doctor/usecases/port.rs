//! Doctor use case ports.

use crate::features::doctor::domain::{DoctorRepairPlan, DoctorReport, DoctorReportRequest};
use crate::features::runtime::domain::{BootstrapRuntimeInput, PythonRuntimeResolutionInput};
use crate::features::runtime::usecases::RuntimeBootstrapResult;
use crate::foundation::error::KernelResult;

/// Policy for reading capability state while building a doctor report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorCapabilityReadPolicy {
    /// Use the current cached capability state, probing only when no cache exists.
    Current,
    /// Reprobe and persist capability state before mapping it into doctor checks.
    Refresh,
}

impl Default for DoctorCapabilityReadPolicy {
    fn default() -> Self {
        Self::Current
    }
}

/// Policy for command probes that are useful diagnostics but not always required.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorCommandCheckPolicy {
    /// Skip optional command probes and only report resolved runtime/capability facts.
    SkipOptional,
    /// Include developer bootstrap tools such as `uv --version`.
    IncludeDeveloperTools,
}

impl Default for DoctorCommandCheckPolicy {
    fn default() -> Self {
        Self::IncludeDeveloperTools
    }
}

/// Request for assembling a doctor report from layout, runtime, and capability facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReportUseCaseRequest {
    pub doctor: DoctorReportRequest,
    pub runtime: PythonRuntimeResolutionInput,
    pub capabilities: DoctorCapabilityReadPolicy,
    pub commands: DoctorCommandCheckPolicy,
}

/// Result of assembling a doctor report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReportUseCaseResult {
    pub report: DoctorReport,
}

/// Request for applying an explicit doctor repair flow and reporting the result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorRepairUseCaseRequest {
    pub report: DoctorReportUseCaseRequest,
    pub bootstrap: BootstrapRuntimeInput,
}

/// Result of an explicit doctor repair flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorRepairUseCaseResult {
    pub plan: DoctorRepairPlan,
    pub bootstrap: Option<RuntimeBootstrapResult>,
    pub report: DoctorReport,
}

/// Use-case boundary for building a read-only or local doctor report.
pub trait DoctorReportUseCase {
    /// Builds a doctor report without executing repair actions.
    fn doctor_report(
        &self,
        request: DoctorReportUseCaseRequest,
    ) -> KernelResult<DoctorReportUseCaseResult>;
}

/// Use-case boundary for explicit doctor repair orchestration.
pub trait DoctorRepairUseCase {
    /// Plans repair, delegates any mutation to owned use cases, then returns a fresh report.
    fn repair_doctor(
        &self,
        request: DoctorRepairUseCaseRequest,
    ) -> KernelResult<DoctorRepairUseCaseResult>;
}
