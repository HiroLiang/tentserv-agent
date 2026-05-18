use std::time::Duration;

use axum::{
    extract::{Path, State},
    Json,
};
use tentgent_kernel::features::server::domain::ServerInspection;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{sleep, timeout, Instant},
};

use crate::transport::rest::{error::RestError, state::RestState};

use super::{
    common::{inspect_server, now_rfc3339},
    dto::{server_health_server_item, ServerHealthResponse, ServerReadinessItem},
};

const SERVER_HEALTH_TIMEOUT: Duration = Duration::from_millis(750);
const SERVER_READINESS_POLL: Duration = Duration::from_millis(100);

pub async fn health(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<ServerHealthResponse>, RestError> {
    let inspection = inspect_server(&state, &reference)?;
    let probe = probe_server_health(&inspection).await;

    Ok(Json(ServerHealthResponse {
        server: server_health_server_item(&inspection),
        running: inspection.running,
        reachable: probe.reachable,
        target_url: server_health_url(&inspection),
        target_status: probe.target_status,
        target_health: probe.target_health,
        checked_at: probe.checked_at,
        error: probe.error,
    }))
}

pub(super) async fn wait_for_server_ready(
    inspection: &ServerInspection,
    timeout_seconds: u64,
) -> ServerReadinessItem {
    let deadline = Instant::now() + Duration::from_secs(timeout_seconds);
    loop {
        let probe = probe_server_health(inspection).await;
        if probe.reachable {
            return readiness_item(true, probe);
        }
        if Instant::now() >= deadline {
            let mut readiness = readiness_item(false, probe);
            readiness.error = Some(
                readiness
                    .error
                    .unwrap_or_else(|| "timed out waiting for server readiness".to_string()),
            );
            return readiness;
        }
        sleep(SERVER_READINESS_POLL).await;
    }
}

struct ServerHealthProbe {
    reachable: bool,
    target_status: Option<u16>,
    target_health: Option<serde_json::Value>,
    checked_at: String,
    error: Option<String>,
}

fn readiness_item(ready: bool, probe: ServerHealthProbe) -> ServerReadinessItem {
    ServerReadinessItem {
        ready,
        reachable: probe.reachable,
        target_status: probe.target_status,
        target_health: probe.target_health,
        checked_at: probe.checked_at,
        error: probe.error,
    }
}

async fn probe_server_health(inspection: &ServerInspection) -> ServerHealthProbe {
    if !inspection.running {
        return ServerHealthProbe {
            reachable: false,
            target_status: None,
            target_health: None,
            checked_at: now_rfc3339(),
            error: None,
        };
    }

    let target = socket_addr_text(&inspection.spec.host, inspection.spec.port);
    let mut stream = match timeout(SERVER_HEALTH_TIMEOUT, TcpStream::connect(&target)).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(err)) => return health_probe_error(format!("connect {target} failed: {err}")),
        Err(_) => return health_probe_error(format!("connect {target} timed out")),
    };
    let host = host_for_header(&inspection.spec.host, inspection.spec.port);
    let request = format!("GET /healthz HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    if let Err(err) = timeout(SERVER_HEALTH_TIMEOUT, stream.write_all(request.as_bytes())).await {
        return health_probe_error(format!("write health request timed out: {err}"));
    }
    let mut response = String::new();
    match timeout(SERVER_HEALTH_TIMEOUT, stream.read_to_string(&mut response)).await {
        Ok(Ok(_)) => {}
        Ok(Err(err)) => return health_probe_error(format!("read health response failed: {err}")),
        Err(_) => return health_probe_error("read health response timed out".to_string()),
    }

    let target_status = parse_http_status(&response);
    let target_health = response
        .split_once("\r\n\r\n")
        .and_then(|(_, body)| serde_json::from_str::<serde_json::Value>(body.trim()).ok());
    let reachable = target_status.is_some_and(|status| (200..300).contains(&status));
    let error = if reachable {
        None
    } else {
        Some(
            target_status
                .map(|status| format!("target health returned HTTP {status}"))
                .unwrap_or_else(|| {
                    "target health response did not include an HTTP status".to_string()
                }),
        )
    };

    ServerHealthProbe {
        reachable,
        target_status,
        target_health,
        checked_at: now_rfc3339(),
        error,
    }
}

fn health_probe_error(error: String) -> ServerHealthProbe {
    ServerHealthProbe {
        reachable: false,
        target_status: None,
        target_health: None,
        checked_at: now_rfc3339(),
        error: Some(error),
    }
}

fn parse_http_status(response: &str) -> Option<u16> {
    response
        .lines()
        .next()?
        .split_whitespace()
        .nth(1)?
        .parse::<u16>()
        .ok()
}

fn server_health_url(inspection: &ServerInspection) -> String {
    format!(
        "http://{}:{}/healthz",
        host_for_url(&inspection.spec.host),
        inspection.spec.port
    )
}

fn socket_addr_text(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn host_for_header(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn host_for_url(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}
