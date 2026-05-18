use axum::{extract::State, Json};
use tentgent_kernel::features::{
    doctor::{
        domain::{DoctorCheck, DoctorReport, DoctorReportRequest, DoctorSummary},
        usecases::{
            DoctorCapabilityReadPolicy, DoctorCommandCheckPolicy, DoctorReportUseCase,
            DoctorReportUseCaseRequest,
        },
    },
    runtime::domain::PythonRuntimeResolutionInput,
};

use crate::transport::rest::{error::RestError, state::RestState};

use super::dto::{DoctorCheckItem, DoctorResponse, DoctorSummaryItem};

pub async fn report(State(state): State<RestState>) -> Result<Json<DoctorResponse>, RestError> {
    let result = state
        .app()
        .services()
        .kernel()
        .doctor_report_usecase()
        .doctor_report(DoctorReportUseCaseRequest {
            doctor: DoctorReportRequest::observational()
                .with_runtime_home(state.app().layout().home_dir.clone()),
            runtime: PythonRuntimeResolutionInput::default(),
            capabilities: DoctorCapabilityReadPolicy::Current,
            commands: DoctorCommandCheckPolicy::SkipOptional,
        })
        .map_err(|err| RestError::kernel("doctor_report_failed", err))?;

    Ok(Json(doctor_response(result.report)))
}

fn doctor_response(report: DoctorReport) -> DoctorResponse {
    DoctorResponse {
        status: report.status.as_str().to_string(),
        summary: doctor_summary_item(report.summary),
        checks: report.checks.into_iter().map(doctor_check_item).collect(),
    }
}

fn doctor_summary_item(summary: DoctorSummary) -> DoctorSummaryItem {
    DoctorSummaryItem {
        pass: summary.pass,
        warn: summary.warn,
        fail: summary.fail,
        skipped: summary.skipped,
    }
}

fn doctor_check_item(check: DoctorCheck) -> DoctorCheckItem {
    DoctorCheckItem {
        name: check.name,
        status: check.status.as_str().to_string(),
        detail: check.detail,
    }
}
