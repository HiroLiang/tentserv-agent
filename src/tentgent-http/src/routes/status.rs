use tentgent_core::VERSION;
use tentgent_kernel::{
    features::daemon::{
        domain::{DaemonInspection, DaemonWarning},
        infra::{
            daemon_runtime_layout_input as daemon_layout, StdDaemonKernel,
            DEFAULT_DAEMON_PROBE_TIMEOUT,
        },
        usecases::{DaemonInspectionMode, DaemonStatusRequest, DaemonStatusUseCase},
    },
    foundation::layout::LayoutResolveMode,
};

use crate::{
    app::DaemonHttpState,
    dto::{HealthResponse, StatusAuthItem, StatusResponse, StatusWarningItem},
    http::HttpResponse,
    response::json_response,
    routes::store::path_string,
};

pub(crate) const SERVICE_NAME: &str = "tentgent-daemon";

pub(crate) fn healthz_response() -> HttpResponse {
    json_response(
        200,
        HealthResponse {
            status: "ok",
            service: SERVICE_NAME,
            version: VERSION,
        },
    )
}

pub(crate) fn status_response(state: &DaemonHttpState) -> HttpResponse {
    json_response(200, status_item(state))
}

fn status_item(state: &DaemonHttpState) -> StatusResponse {
    let inspection = state.inspection();
    let process = inspection.process.as_ref();
    let warnings = dynamic_warnings(inspection);
    StatusResponse {
        service: SERVICE_NAME,
        version: VERSION,
        status: if inspection.running {
            "running"
        } else {
            "stopped"
        },
        auth: StatusAuthItem {
            token_enabled: state.security().token_enabled(),
        },
        host: process.map(|process| process.host.clone()),
        port: process.map(|process| process.port),
        pid: process.map(|process| process.pid),
        started_at: process.map(|process| process.started_at.clone()),
        runtime_home: path_string(&inspection.home_dir),
        runtime_dir: path_string(&inspection.runtime_dir),
        log_dir: path_string(&inspection.log_dir),
        process_path: path_string(&inspection.process_path),
        pid_path: path_string(&inspection.pid_path),
        warnings: warnings
            .iter()
            .map(|warning| StatusWarningItem {
                code: warning.code.clone(),
                message: warning.message.clone(),
                path: warning.path.as_ref().map(|path| path_string(path)),
            })
            .collect(),
    }
}

fn dynamic_warnings(inspection: &DaemonInspection) -> Vec<DaemonWarning> {
    let Ok(kernel) = StdDaemonKernel::new(DEFAULT_DAEMON_PROBE_TIMEOUT) else {
        return inspection.warnings.clone();
    };
    let daemon = kernel.usecase();
    daemon
        .daemon_status(DaemonStatusRequest {
            layout: daemon_layout(
                Some(inspection.home_dir.clone()),
                LayoutResolveMode::ReadOnly,
            ),
            mode: DaemonInspectionMode::Observational,
        })
        .map(|dynamic| dynamic.inspection.warnings)
        .unwrap_or_else(|_| inspection.warnings.clone())
}
