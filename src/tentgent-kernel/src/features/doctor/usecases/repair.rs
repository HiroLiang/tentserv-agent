//! Doctor repair use case.

use crate::features::doctor::ports::DoctorRepairPlanner;
use crate::features::runtime::usecases::{RuntimeBootstrapRequest, RuntimeBootstrapUseCase};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{LayoutResolveMode, RuntimeLayoutInput};

use super::port::{
    DoctorCapabilityReadPolicy, DoctorRepairUseCase, DoctorRepairUseCaseRequest,
    DoctorRepairUseCaseResult, DoctorReportUseCase,
};

/// Standard explicit doctor repair orchestration.
pub struct StdDoctorRepairUseCase<'a> {
    repair_planner: &'a dyn DoctorRepairPlanner,
    runtime_bootstrap: &'a dyn RuntimeBootstrapUseCase,
    report: &'a dyn DoctorReportUseCase,
}

impl<'a> StdDoctorRepairUseCase<'a> {
    pub fn new(
        repair_planner: &'a dyn DoctorRepairPlanner,
        runtime_bootstrap: &'a dyn RuntimeBootstrapUseCase,
        report: &'a dyn DoctorReportUseCase,
    ) -> Self {
        Self {
            repair_planner,
            runtime_bootstrap,
            report,
        }
    }
}

impl DoctorRepairUseCase for StdDoctorRepairUseCase<'_> {
    fn repair_doctor(
        &self,
        request: DoctorRepairUseCaseRequest,
    ) -> KernelResult<DoctorRepairUseCaseResult> {
        let plan = self.repair_planner.plan_repair(&request.report.doctor)?;
        let bootstrap = if plan.mutates_local_state {
            Some(
                self.runtime_bootstrap
                    .bootstrap_runtime(RuntimeBootstrapRequest {
                        layout: RuntimeLayoutInput {
                            mode: LayoutResolveMode::Create,
                            home_dir: request.report.doctor.runtime_home.clone(),
                            data_root_dir: None,
                        },
                        runtime: request.report.runtime.clone(),
                        bootstrap: request.bootstrap,
                    })?,
            )
        } else {
            None
        };

        let mut report_request = request.report;
        report_request.capabilities = DoctorCapabilityReadPolicy::Refresh;
        let report = self.report.doctor_report(report_request)?.report;

        Ok(DoctorRepairUseCaseResult {
            plan,
            bootstrap,
            report,
        })
    }
}
