use std::time::{Duration, Instant};

use serde_json::Value;
use tentgent_core::server::{
    ServerError, ServerInspection, ServerManager, ServerRunRequest, ServerStopOutcome,
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::time::sleep;

use crate::{
    app::DaemonHttpState,
    dto::{
        CreateServerRequest, CreateServerResponse, ServerHealthResponse, ServerReadinessItem,
        ServerStartResponse, StartServerOptions, StartServerRequest, StopServerResponse,
    },
    http::{HttpRequest, HttpResponse},
    response::{
        bad_request_response, json_response, parse_json_body, runtime_error_response,
        server_error_response,
    },
    routes::store::{server_inspection_item, server_prepare_item},
    server_runtime::{launch_background_server_runtime, resolve_server_runtime_auth},
};

const HEALTH_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const READINESS_DEFAULT_TIMEOUT_SECONDS: u64 = 30;
const READINESS_MAX_TIMEOUT_SECONDS: u64 = 120;
const READINESS_POLL_INTERVAL: Duration = Duration::from_millis(250);

pub(crate) async fn health_server_response(
    state: &DaemonHttpState,
    reference: &str,
) -> HttpResponse {
    let manager = match ServerManager::open_readonly(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return server_error_response(error),
    };
    let inspection = match manager.inspect(reference) {
        Ok(server) => server,
        Err(error) => return server_error_response(error),
    };
    let target_url = server_health_url(&inspection);
    let readiness = if inspection.running {
        probe_server_readiness(state, &inspection, HEALTH_PROBE_TIMEOUT).await
    } else {
        stopped_server_readiness()
    };
    let running = inspection.running;

    json_response(
        200,
        ServerHealthResponse {
            server: server_inspection_item(inspection),
            running,
            reachable: readiness.reachable,
            target_url,
            target_status: readiness.target_status,
            target_health: readiness.target_health,
            checked_at: readiness.checked_at,
            error: readiness.error,
        },
    )
}

pub(crate) fn create_server_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
) -> HttpResponse {
    let body = match parse_json_body::<CreateServerRequest>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let runtime_ref = body.runtime_ref.trim();
    if runtime_ref.is_empty() {
        return bad_request_response("`runtime_ref` must not be empty");
    }

    let manager = match ServerManager::new(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return server_error_response(error),
    };
    let outcome = match manager.prepare_run(ServerRunRequest {
        runtime_ref: runtime_ref.to_string(),
        host: body.host,
        port: body.port,
        lazy_load: body.lazy_load,
        idle_seconds: body.idle_seconds,
    }) {
        Ok(outcome) => outcome,
        Err(error @ (ServerError::EmptyHost | ServerError::EmptyCloudProviderModel { .. })) => {
            return bad_request_response(error.to_string())
        }
        Err(error) => return server_error_response(error),
    };
    let created = outcome.created;

    json_response(
        200,
        CreateServerResponse {
            created,
            server: server_prepare_item(outcome),
        },
    )
}

pub(crate) async fn start_server_response(
    state: &DaemonHttpState,
    reference: &str,
    request: &HttpRequest,
) -> HttpResponse {
    let options = match start_server_options(request) {
        Ok(options) => options,
        Err(response) => return response,
    };
    let manager = match ServerManager::new(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return server_error_response(error),
    };
    let inspection = match manager.resolve_for_start(reference) {
        Ok(inspection) => inspection,
        Err(error) => return server_error_response(error),
    };
    let cloud_auth = match resolve_server_runtime_auth(&inspection.spec).await {
        Ok(auth) => auth,
        Err(error) => return runtime_error_response(error),
    };
    match launch_background_server_runtime(&manager, &inspection, cloud_auth.as_ref()).await {
        Ok(server) => {
            let readiness = if options.wait_ready {
                Some(wait_for_server_readiness(state, &server, options.timeout).await)
            } else {
                None
            };
            json_response(
                200,
                ServerStartResponse {
                    server: server_inspection_item(server),
                    readiness,
                },
            )
        }
        Err(error) => runtime_error_response(error),
    }
}

pub(crate) fn stop_server_response(state: &DaemonHttpState, reference: &str) -> HttpResponse {
    let manager = match ServerManager::new(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return server_error_response(error),
    };
    match manager.stop(reference) {
        Ok(outcome) => stop_server_outcome_response(outcome),
        Err(error) => server_error_response(error),
    }
}

fn stop_server_outcome_response(outcome: ServerStopOutcome) -> HttpResponse {
    json_response(
        200,
        StopServerResponse {
            stopped_pid: outcome.stopped_pid,
            server: server_inspection_item(outcome.inspection),
        },
    )
}

pub(crate) fn start_server_options(
    request: &HttpRequest,
) -> Result<StartServerOptions, HttpResponse> {
    if request.body.is_empty() {
        return Ok(StartServerOptions::default());
    }

    let body = parse_json_body::<StartServerRequest>(request)?;
    let timeout_seconds = body
        .timeout_seconds
        .unwrap_or(READINESS_DEFAULT_TIMEOUT_SECONDS);
    if timeout_seconds == 0 || timeout_seconds > READINESS_MAX_TIMEOUT_SECONDS {
        return Err(bad_request_response(format!(
            "`timeout_seconds` must be between 1 and {READINESS_MAX_TIMEOUT_SECONDS}"
        )));
    }

    Ok(StartServerOptions {
        wait_ready: body.wait_ready.unwrap_or(false),
        timeout: Duration::from_secs(timeout_seconds),
    })
}

impl Default for StartServerOptions {
    fn default() -> Self {
        Self {
            wait_ready: false,
            timeout: Duration::from_secs(READINESS_DEFAULT_TIMEOUT_SECONDS),
        }
    }
}

pub(crate) async fn wait_for_server_readiness(
    state: &DaemonHttpState,
    server: &ServerInspection,
    timeout: Duration,
) -> ServerReadinessItem {
    let started = Instant::now();
    loop {
        let remaining = timeout.saturating_sub(started.elapsed());
        let probe_timeout = remaining.min(HEALTH_PROBE_TIMEOUT);
        let probe = probe_server_readiness(state, server, probe_timeout).await;
        if probe.ready || started.elapsed() >= timeout {
            return if probe.ready {
                probe
            } else {
                ServerReadinessItem {
                    ready: false,
                    error: Some(format!(
                        "server `{}` did not become ready within {} second(s)",
                        server.spec.short_ref,
                        timeout.as_secs()
                    )),
                    ..probe
                }
            };
        }

        let sleep_for = READINESS_POLL_INTERVAL.min(timeout.saturating_sub(started.elapsed()));
        if sleep_for.is_zero() {
            return ServerReadinessItem {
                ready: false,
                reachable: probe.reachable,
                target_status: probe.target_status,
                target_health: probe.target_health,
                checked_at: probe.checked_at,
                error: Some(format!(
                    "server `{}` did not become ready within {} second(s)",
                    server.spec.short_ref,
                    timeout.as_secs()
                )),
            };
        }
        sleep(sleep_for).await;
    }
}

async fn probe_server_readiness(
    state: &DaemonHttpState,
    server: &ServerInspection,
    timeout: Duration,
) -> ServerReadinessItem {
    let target_url = server_health_url(server);
    let response = state
        .http_client()
        .get(target_url)
        .timeout(timeout)
        .send()
        .await;
    let checked_at = checked_at_now();

    match response {
        Ok(response) => {
            let status = response.status();
            let status_code = status.as_u16();
            match response.bytes().await {
                Ok(bytes) => {
                    let target_health = serde_json::from_slice::<Value>(&bytes).ok();
                    ServerReadinessItem {
                        ready: status.is_success(),
                        reachable: true,
                        target_status: Some(status_code),
                        target_health,
                        checked_at,
                        error: if status.is_success() {
                            None
                        } else {
                            Some(format!("target health returned HTTP {status_code}"))
                        },
                    }
                }
                Err(error) => ServerReadinessItem {
                    ready: false,
                    reachable: true,
                    target_status: Some(status_code),
                    target_health: None,
                    checked_at,
                    error: Some(format!("failed to read target health response: {error}")),
                },
            }
        }
        Err(error) => ServerReadinessItem {
            ready: false,
            reachable: false,
            target_status: None,
            target_health: None,
            checked_at,
            error: Some(format!("target health is unreachable: {error}")),
        },
    }
}

fn stopped_server_readiness() -> ServerReadinessItem {
    ServerReadinessItem {
        ready: false,
        reachable: false,
        target_status: None,
        target_health: None,
        checked_at: checked_at_now(),
        error: Some("server process is not running".to_string()),
    }
}

fn server_health_url(server: &ServerInspection) -> String {
    format!("http://{}:{}/healthz", server.spec.host, server.spec.port)
}

fn checked_at_now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}
