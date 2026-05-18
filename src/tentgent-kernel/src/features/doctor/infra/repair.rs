use crate::features::doctor::domain::{
    DoctorRepairIntent, DoctorRepairPlan, DoctorRepairStep, DoctorReportRequest,
};
use crate::features::doctor::ports::DoctorRepairPlanner;
use crate::foundation::error::KernelResult;

/// Plans explicit doctor repair actions without executing them.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdDoctorRepairPlanner;

impl DoctorRepairPlanner for StdDoctorRepairPlanner {
    fn plan_repair(&self, request: &DoctorReportRequest) -> KernelResult<DoctorRepairPlan> {
        let mutates_local_state =
            request.mode.allows_write_probes() && request.repair.mutates_local_state();
        let steps = match request.repair {
            DoctorRepairIntent::ReportOnly => Vec::new(),
            DoctorRepairIntent::DeveloperPythonEnv if mutates_local_state => {
                vec![DoctorRepairStep {
                    label: "bootstrap managed Python runtime".to_string(),
                    command: Some("tentgent runtime bootstrap --profile base".to_string()),
                    detail: "delegate repair to the runtime bootstrap flow".to_string(),
                }]
            }
            DoctorRepairIntent::DeveloperPythonEnv => {
                vec![DoctorRepairStep {
                    label: "bootstrap managed Python runtime skipped".to_string(),
                    command: None,
                    detail: "observational doctor mode cannot mutate local state".to_string(),
                }]
            }
        };

        Ok(DoctorRepairPlan {
            intent: request.repair,
            mutates_local_state,
            steps,
        })
    }
}
