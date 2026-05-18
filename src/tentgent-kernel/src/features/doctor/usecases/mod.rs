//! Doctor use case boundaries.

pub mod port;
pub mod repair;
pub mod report;

#[cfg(test)]
mod tests;

pub use port::{
    DoctorCapabilityReadPolicy, DoctorCommandCheckPolicy, DoctorRepairUseCase,
    DoctorRepairUseCaseRequest, DoctorRepairUseCaseResult, DoctorReportUseCase,
    DoctorReportUseCaseRequest, DoctorReportUseCaseResult,
};
pub use repair::StdDoctorRepairUseCase;
pub use report::StdDoctorReportUseCase;
