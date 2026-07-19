use axum::{extract::State, Json};
use tentgent_kernel::{
    features::{
        cluster::usecases::{
            cluster_readiness_doctor_checks, ClusterReadinessListRequest, ClusterReadinessUseCase,
        },
        doctor::{
            domain::{
                DoctorCheck, DoctorCheckCategory, DoctorCheckDetail, DoctorNextAction,
                DoctorReport, DoctorReportRequest, DoctorSummary,
            },
            usecases::{
                DoctorCapabilityReadPolicy, DoctorCommandCheckPolicy, DoctorReportUseCase,
                DoctorReportUseCaseRequest,
            },
        },
        runtime::domain::PythonRuntimeResolutionInput,
        runtime_ownership::{RuntimeOwnershipStatus, StdRuntimeOwnershipUseCase},
    },
    foundation::layout::LayoutResolveMode,
};

use crate::transport::rest::{error::RestError, state::RestState};

use super::dto::{
    DoctorCheckDetailItem, DoctorCheckItem, DoctorNextActionItem, DoctorResponse, DoctorSummaryItem,
};

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

    let report = append_cluster_readiness_checks(&state, result.report);
    Ok(Json(doctor_response(append_runtime_ownership_check(
        &state, report,
    ))))
}

fn append_runtime_ownership_check(state: &RestState, report: DoctorReport) -> DoctorReport {
    let mut checks = report.checks;
    let check = match StdRuntimeOwnershipUseCase::default()
        .summarize_runtime_ownership(state.app().layout())
    {
        Ok(inspection) => {
            let detail = format!(
                "{} route claim(s), {} active generation(s), {} stale record(s), {} malformed record(s)",
                inspection.summary.route_claim_count,
                inspection.summary.active_generation_count,
                inspection.summary.stale_record_count,
                inspection.summary.malformed_record_count
            );
            match inspection.summary.status {
                RuntimeOwnershipStatus::Healthy => DoctorCheck::pass(
                    DoctorCheckCategory::RuntimeOwnership,
                    "runtime ownership",
                    detail,
                ),
                RuntimeOwnershipStatus::Attention => DoctorCheck::warn(
                    DoctorCheckCategory::RuntimeOwnership,
                    "runtime ownership",
                    detail,
                ),
                RuntimeOwnershipStatus::Blocked => DoctorCheck::fail(
                    DoctorCheckCategory::RuntimeOwnership,
                    "runtime ownership",
                    detail,
                ),
            }
        }
        Err(error) => DoctorCheck::warn(
            DoctorCheckCategory::RuntimeOwnership,
            "runtime ownership",
            format!("runtime ownership check unavailable: {error}"),
        ),
    };
    checks.push(check);
    DoctorReport::from_checks(checks)
}

fn append_cluster_readiness_checks(state: &RestState, report: DoctorReport) -> DoctorReport {
    let mut checks = report.checks;
    checks.extend(cluster_readiness_checks(state));
    DoctorReport::from_checks(checks)
}

fn cluster_readiness_checks(state: &RestState) -> Vec<DoctorCheck> {
    let result = match state
        .app()
        .services()
        .kernel()
        .cluster_readiness_usecase()
        .list_cluster_readiness(ClusterReadinessListRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
        }) {
        Ok(result) => result,
        Err(err) => {
            return vec![DoctorCheck::warn(
                DoctorCheckCategory::Cluster,
                "cluster readiness",
                format!("cluster readiness checks unavailable: {err}"),
            )];
        }
    };

    cluster_readiness_doctor_checks(result.clusters.iter().map(|cluster| {
        (
            &cluster.inspection.definition.cluster_ref,
            &cluster.readiness,
        )
    }))
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
        category: check.category.as_str().to_string(),
        status: check.status.as_str().to_string(),
        description: check.description,
        detail: check.detail,
        flags: check.flags,
        details: check
            .details
            .into_iter()
            .map(doctor_check_detail_item)
            .collect(),
        next_actions: check
            .next_actions
            .into_iter()
            .map(doctor_next_action_item)
            .collect(),
    }
}

fn doctor_check_detail_item(detail: DoctorCheckDetail) -> DoctorCheckDetailItem {
    DoctorCheckDetailItem {
        name: detail.name,
        description: detail.description,
        flags: detail.flags,
    }
}

fn doctor_next_action_item(action: DoctorNextAction) -> DoctorNextActionItem {
    DoctorNextActionItem {
        label: action.label,
        code: action.code,
        command: action.command,
        detail: action.detail,
    }
}
