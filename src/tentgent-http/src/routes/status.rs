use tentgent_core::{daemon::DaemonInspection, VERSION};

use crate::{
    app::DaemonHttpState,
    dto::{HealthResponse, StatusResponse},
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
    json_response(200, status_item(state.inspection()))
}

fn status_item(inspection: &DaemonInspection) -> StatusResponse {
    let process = inspection.process.as_ref();
    StatusResponse {
        service: SERVICE_NAME,
        version: VERSION,
        status: if inspection.running {
            "running"
        } else {
            "stopped"
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
    }
}
