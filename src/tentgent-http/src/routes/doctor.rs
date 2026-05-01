use tentgent_core::doctor::{build_doctor_report, DoctorCheck, DoctorOptions};

use crate::{
    app::DaemonHttpState,
    dto::{DoctorCheckItem, DoctorResponse, DoctorSummaryItem},
    http::HttpResponse,
    response::json_response,
};

pub(crate) fn doctor_response(state: &DaemonHttpState) -> HttpResponse {
    let report = build_doctor_report(
        DoctorOptions::observational().with_runtime_home(state.home_dir().to_path_buf()),
    );
    json_response(
        200,
        DoctorResponse {
            status: report.status.as_str().to_string(),
            summary: DoctorSummaryItem {
                pass: report.summary.pass,
                warn: report.summary.warn,
                fail: report.summary.fail,
                skipped: report.summary.skipped,
            },
            checks: report.checks.into_iter().map(doctor_check_item).collect(),
        },
    )
}

fn doctor_check_item(check: DoctorCheck) -> DoctorCheckItem {
    DoctorCheckItem {
        name: check.name,
        status: check.status.as_str().to_string(),
        detail: check.detail,
    }
}
