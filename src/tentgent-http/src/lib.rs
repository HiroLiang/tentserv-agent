use std::{net::SocketAddr, path::Path, time::Instant};

use miette::{miette, IntoDiagnostic};
use serde::Serialize;
use serde_json::json;
use tentgent_core::{
    adapter::{AdapterManager, AdapterSummary},
    daemon::DaemonInspection,
    dataset::{DatasetManager, DatasetSummary},
    model::{ModelManager, ModelSummary},
    server::{ServerError, ServerInspection, ServerManager, ServerProcessMetadata, ServerSummary},
    VERSION,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const SERVICE_NAME: &str = "tentgent-daemon";
const MAX_REQUEST_BYTES: usize = 16 * 1024;

#[derive(Debug)]
pub struct DaemonHttpServer {
    listener: TcpListener,
    host: String,
    port: u16,
}

impl DaemonHttpServer {
    pub async fn bind(host: String, port: u16) -> miette::Result<Self> {
        let listener = TcpListener::bind((host.as_str(), port))
            .await
            .map_err(|err| {
                miette!("failed to bind daemon HTTP listener on {host}:{port}: {err}")
            })?;
        let local_addr = listener.local_addr().into_diagnostic()?;

        Ok(Self {
            listener,
            host,
            port: local_addr.port(),
        })
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn bind_label(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub async fn serve(self, state: DaemonHttpState) -> miette::Result<()> {
        loop {
            let (stream, peer_addr) = self.listener.accept().await.into_diagnostic()?;
            let state = state.clone();
            tokio::spawn(async move {
                if let Err(error) = handle_connection(stream, peer_addr, state).await {
                    eprintln!("tentgent-http connection_error peer={peer_addr} error={error}");
                }
            });
        }
    }
}

#[derive(Debug, Clone)]
pub struct DaemonHttpState {
    inspection: DaemonInspection,
}

impl DaemonHttpState {
    pub fn new(inspection: DaemonInspection) -> Self {
        Self { inspection }
    }

    fn home_dir(&self) -> &Path {
        &self.inspection.home_dir
    }
}

#[derive(Debug, Serialize)]
struct HealthResponse<'a> {
    status: &'a str,
    service: &'a str,
    version: &'a str,
}

#[derive(Debug, Serialize)]
struct StatusResponse {
    service: &'static str,
    version: &'static str,
    status: &'static str,
    host: Option<String>,
    port: Option<u16>,
    pid: Option<u32>,
    started_at: Option<String>,
    runtime_home: String,
    runtime_dir: String,
    log_dir: String,
    process_path: String,
    pid_path: String,
}

#[derive(Debug, Serialize)]
struct ModelsResponse {
    models: Vec<ModelItem>,
}

#[derive(Debug, Serialize)]
struct ModelItem {
    model_ref: String,
    short_ref: String,
    store_path: String,
    file_count: usize,
    total_bytes: u64,
    imported_at: String,
    format: String,
    detected_formats: Vec<String>,
    source_kind: String,
    source_repo: Option<String>,
    source_revision: Option<String>,
    source_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct AdaptersResponse {
    adapters: Vec<AdapterItem>,
}

#[derive(Debug, Serialize)]
struct AdapterItem {
    adapter_ref: String,
    short_ref: String,
    store_path: String,
    file_count: usize,
    total_bytes: u64,
    imported_at: String,
    format: String,
    #[serde(rename = "type")]
    adapter_type: String,
    base_model_ref: Option<String>,
    base_model_source_repo: Option<String>,
    base_model_source_revision: Option<String>,
    model_family: Option<String>,
    backend_support: Vec<String>,
    source_kind: String,
    source_repo: Option<String>,
    source_revision: Option<String>,
    source_path: Option<String>,
    training_dataset_ref: Option<String>,
    training_run_ref: Option<String>,
    training_config_ref: Option<String>,
}

#[derive(Debug, Serialize)]
struct DatasetsResponse {
    datasets: Vec<DatasetItem>,
}

#[derive(Debug, Serialize)]
struct DatasetItem {
    dataset_ref: String,
    short_ref: String,
    store_path: String,
    file_count: usize,
    total_bytes: u64,
    imported_at: String,
    format: String,
    source_kind: String,
    source_path: Option<String>,
    source_repo: Option<String>,
    source_revision: Option<String>,
    tuning_ready: bool,
    splits: DatasetSplitsItem,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DatasetSplitsItem {
    train: Option<String>,
    validation: Option<String>,
    test: Option<String>,
    eval_cases: Option<String>,
    source_manifest: Option<String>,
}

#[derive(Debug, Serialize)]
struct ServersResponse {
    servers: Vec<ServerSummaryItem>,
}

#[derive(Debug, Serialize)]
struct ServerResponse {
    server: ServerInspectionItem,
}

#[derive(Debug, Serialize)]
struct ServerSummaryItem {
    server_ref: String,
    short_ref: String,
    runtime_kind: String,
    model_ref: Option<String>,
    provider: Option<String>,
    provider_model: Option<String>,
    host: String,
    port: u16,
    lazy_load: bool,
    idle_seconds: Option<u64>,
    created_at: String,
    running: bool,
    process: Option<ServerProcessItem>,
}

#[derive(Debug, Serialize)]
struct ServerInspectionItem {
    server_ref: String,
    short_ref: String,
    runtime_kind: String,
    model_ref: Option<String>,
    provider: Option<String>,
    provider_model: Option<String>,
    host: String,
    port: u16,
    lazy_load: bool,
    idle_seconds: Option<u64>,
    created_at: String,
    running: bool,
    process: Option<ServerProcessItem>,
    home_dir: String,
    server_dir: String,
    spec_path: String,
    process_path: String,
    stdout_log: String,
    stderr_log: String,
}

#[derive(Debug, Serialize)]
struct ServerProcessItem {
    pid: u32,
    launch_mode: String,
    started_at: String,
}

#[derive(Debug, Serialize)]
struct ErrorResponse<'a> {
    error: &'a str,
    message: String,
}

async fn handle_connection(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    state: DaemonHttpState,
) -> miette::Result<()> {
    let started = Instant::now();
    let request = read_request(&mut stream).await?;
    let response = route_request(&request, &state);
    eprintln!(
        "tentgent-http request peer={} method={} path={} status={} elapsed_ms={}",
        peer_addr,
        request.method_label(),
        request.path_label(),
        response.status_code,
        started.elapsed().as_millis()
    );
    write_response(&mut stream, response).await
}

async fn read_request(stream: &mut TcpStream) -> miette::Result<HttpRequest> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];

    loop {
        let read = stream.read(&mut chunk).await.into_diagnostic()?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if buffer.len() > MAX_REQUEST_BYTES {
            return Ok(HttpRequest::too_large());
        }
    }

    let request = String::from_utf8_lossy(&buffer);
    let Some(request_line) = request.lines().next() else {
        return Ok(HttpRequest::invalid());
    };

    let mut parts = request_line.split_whitespace();
    let Some(method) = parts.next() else {
        return Ok(HttpRequest::invalid());
    };
    let Some(target) = parts.next() else {
        return Ok(HttpRequest::invalid());
    };
    let Some(version) = parts.next() else {
        return Ok(HttpRequest::invalid());
    };

    Ok(HttpRequest {
        method: method.to_string(),
        path: target.split('?').next().unwrap_or(target).to_string(),
        version: version.to_string(),
        parse_error: None,
    })
}

fn route_request(request: &HttpRequest, state: &DaemonHttpState) -> HttpResponse {
    if let Some(error) = &request.parse_error {
        return json_response(
            400,
            ErrorResponse {
                error: "bad_request",
                message: error.clone(),
            },
        );
    }

    if request.version != "HTTP/1.1" && request.version != "HTTP/1.0" {
        return json_response(
            400,
            ErrorResponse {
                error: "bad_request",
                message: "unsupported HTTP version".to_string(),
            },
        );
    }

    if request.method != "GET" {
        return json_response(
            405,
            ErrorResponse {
                error: "method_not_allowed",
                message: format!("{} is not supported for {}", request.method, request.path),
            },
        );
    }

    match request.path.as_str() {
        "/healthz" => json_response(
            200,
            HealthResponse {
                status: "ok",
                service: SERVICE_NAME,
                version: VERSION,
            },
        ),
        "/v1/status" => json_response(200, status_response(&state.inspection)),
        "/v1/models" => list_models_response(state),
        "/v1/adapters" => list_adapters_response(state),
        "/v1/datasets" => list_datasets_response(state),
        "/v1/servers" => list_servers_response(state),
        path if path.starts_with("/v1/servers/") => {
            let reference = path.trim_start_matches("/v1/servers/");
            if reference.is_empty() {
                json_response(
                    404,
                    ErrorResponse {
                        error: "not_found",
                        message: format!("route `{}` was not found", request.path),
                    },
                )
            } else {
                inspect_server_response(state, reference)
            }
        }
        _ => json_response(
            404,
            ErrorResponse {
                error: "not_found",
                message: format!("route `{}` was not found", request.path),
            },
        ),
    }
}

fn list_models_response(state: &DaemonHttpState) -> HttpResponse {
    let manager = match ModelManager::open_readonly_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return manager_error_response("models", error),
    };
    match manager.list_models() {
        Ok(models) => json_response(
            200,
            ModelsResponse {
                models: models.into_iter().map(model_item).collect(),
            },
        ),
        Err(error) => manager_error_response("models", error),
    }
}

fn list_adapters_response(state: &DaemonHttpState) -> HttpResponse {
    let manager = match AdapterManager::open_readonly_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return manager_error_response("adapters", error),
    };
    match manager.list_adapters() {
        Ok(adapters) => json_response(
            200,
            AdaptersResponse {
                adapters: adapters.into_iter().map(adapter_item).collect(),
            },
        ),
        Err(error) => manager_error_response("adapters", error),
    }
}

fn list_datasets_response(state: &DaemonHttpState) -> HttpResponse {
    let manager = match DatasetManager::open_readonly_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return manager_error_response("datasets", error),
    };
    match manager.list_datasets() {
        Ok(datasets) => json_response(
            200,
            DatasetsResponse {
                datasets: datasets.into_iter().map(dataset_item).collect(),
            },
        ),
        Err(error) => manager_error_response("datasets", error),
    }
}

fn list_servers_response(state: &DaemonHttpState) -> HttpResponse {
    let manager = match ServerManager::open_readonly(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return server_error_response(error),
    };
    match manager.list() {
        Ok(servers) => json_response(
            200,
            ServersResponse {
                servers: servers.into_iter().map(server_summary_item).collect(),
            },
        ),
        Err(error) => server_error_response(error),
    }
}

fn inspect_server_response(state: &DaemonHttpState, reference: &str) -> HttpResponse {
    let manager = match ServerManager::open_readonly(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return server_error_response(error),
    };
    match manager.inspect(reference) {
        Ok(server) => json_response(
            200,
            ServerResponse {
                server: server_inspection_item(server),
            },
        ),
        Err(error) => server_error_response(error),
    }
}

fn model_item(summary: ModelSummary) -> ModelItem {
    let metadata = summary.metadata;
    ModelItem {
        model_ref: metadata.model_ref,
        short_ref: metadata.short_ref,
        store_path: path_string(&summary.store_path),
        file_count: metadata.file_count,
        total_bytes: metadata.total_bytes,
        imported_at: metadata.imported_at,
        format: metadata.primary_format.to_string(),
        detected_formats: metadata
            .detected_formats
            .into_iter()
            .map(|format| format.to_string())
            .collect(),
        source_kind: metadata.source_kind.to_string(),
        source_repo: metadata.source_repo,
        source_revision: metadata.source_revision,
        source_path: metadata.source_path,
    }
}

fn adapter_item(summary: AdapterSummary) -> AdapterItem {
    let metadata = summary.metadata;
    AdapterItem {
        adapter_ref: metadata.adapter_ref,
        short_ref: metadata.short_ref,
        store_path: path_string(&summary.store_path),
        file_count: metadata.file_count,
        total_bytes: metadata.total_bytes,
        imported_at: metadata.imported_at,
        format: metadata.adapter_format.to_string(),
        adapter_type: metadata.adapter_type.to_string(),
        base_model_ref: metadata.base_model_ref,
        base_model_source_repo: metadata.base_model_source_repo,
        base_model_source_revision: metadata.base_model_source_revision,
        model_family: metadata.model_family,
        backend_support: metadata.backend_support,
        source_kind: metadata.source_kind.to_string(),
        source_repo: metadata.source_repo,
        source_revision: metadata.source_revision,
        source_path: metadata.source_path,
        training_dataset_ref: metadata.training_dataset_ref,
        training_run_ref: metadata.training_run_ref,
        training_config_ref: metadata.training_config_ref,
    }
}

fn dataset_item(summary: DatasetSummary) -> DatasetItem {
    let metadata = summary.metadata;
    let package = metadata.package;
    DatasetItem {
        dataset_ref: metadata.dataset_ref,
        short_ref: metadata.short_ref,
        store_path: path_string(&summary.store_path),
        file_count: metadata.file_count,
        total_bytes: metadata.total_bytes,
        imported_at: metadata.imported_at,
        format: metadata.dataset_format.to_string(),
        source_kind: metadata.source_kind.to_string(),
        source_path: metadata.source_path,
        source_repo: metadata.source_repo,
        source_revision: metadata.source_revision,
        tuning_ready: package.tuning_ready,
        splits: DatasetSplitsItem {
            train: package.splits.train,
            validation: package.splits.validation,
            test: package.splits.test,
            eval_cases: package.splits.eval_cases,
            source_manifest: package.splits.source_manifest,
        },
        warnings: package.warnings,
    }
}

fn server_summary_item(summary: ServerSummary) -> ServerSummaryItem {
    let spec = summary.spec;
    ServerSummaryItem {
        server_ref: spec.server_ref,
        short_ref: spec.short_ref,
        runtime_kind: spec.runtime_kind.to_string(),
        model_ref: spec.model_ref,
        provider: spec.provider.map(|provider| provider.to_string()),
        provider_model: spec.provider_model,
        host: spec.host,
        port: spec.port,
        lazy_load: spec.lazy_load,
        idle_seconds: spec.idle_seconds,
        created_at: spec.created_at,
        running: summary.running,
        process: summary.process.map(server_process_item),
    }
}

fn server_inspection_item(inspection: ServerInspection) -> ServerInspectionItem {
    let spec = inspection.spec;
    ServerInspectionItem {
        server_ref: spec.server_ref,
        short_ref: spec.short_ref,
        runtime_kind: spec.runtime_kind.to_string(),
        model_ref: spec.model_ref,
        provider: spec.provider.map(|provider| provider.to_string()),
        provider_model: spec.provider_model,
        host: spec.host,
        port: spec.port,
        lazy_load: spec.lazy_load,
        idle_seconds: spec.idle_seconds,
        created_at: spec.created_at,
        running: inspection.running,
        process: inspection.process.map(server_process_item),
        home_dir: path_string(&inspection.home_dir),
        server_dir: path_string(&inspection.server_dir),
        spec_path: path_string(&inspection.spec_path),
        process_path: path_string(&inspection.process_path),
        stdout_log: path_string(&inspection.stdout_log_path),
        stderr_log: path_string(&inspection.stderr_log_path),
    }
}

fn server_process_item(process: ServerProcessMetadata) -> ServerProcessItem {
    ServerProcessItem {
        pid: process.pid,
        launch_mode: process.launch_mode.to_string(),
        started_at: process.started_at,
    }
}

fn status_response(inspection: &DaemonInspection) -> StatusResponse {
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

fn path_string(path: &Path) -> String {
    path.display().to_string()
}

fn manager_error_response(context: &str, error: impl std::fmt::Display) -> HttpResponse {
    json_response(
        500,
        ErrorResponse {
            error: "store_read_failed",
            message: format!("failed to read {context}: {error}"),
        },
    )
}

fn server_error_response(error: ServerError) -> HttpResponse {
    match error {
        ServerError::NotFound(reference) => json_response(
            404,
            ErrorResponse {
                error: "not_found",
                message: format!("server reference `{reference}` was not found"),
            },
        ),
        ServerError::AmbiguousRef(reference) => json_response(
            409,
            ErrorResponse {
                error: "ambiguous_ref",
                message: format!(
                    "server reference `{reference}` is ambiguous; use a longer prefix"
                ),
            },
        ),
        other => json_response(
            500,
            ErrorResponse {
                error: "server_read_failed",
                message: format!("failed to read servers: {other}"),
            },
        ),
    }
}

fn json_response(status_code: u16, body: impl Serialize) -> HttpResponse {
    let body = serde_json::to_vec(&body).unwrap_or_else(|_| {
        json!({
            "error": "response_encoding_failed",
            "message": "failed to encode JSON response"
        })
        .to_string()
        .into_bytes()
    });

    HttpResponse { status_code, body }
}

async fn write_response(stream: &mut TcpStream, response: HttpResponse) -> miette::Result<()> {
    let reason = reason_phrase(response.status_code);
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status_code,
        reason,
        response.body.len()
    );
    stream
        .write_all(header.as_bytes())
        .await
        .into_diagnostic()?;
    stream.write_all(&response.body).await.into_diagnostic()?;
    stream.shutdown().await.into_diagnostic()?;
    Ok(())
}

fn reason_phrase(status_code: u16) -> &'static str {
    match status_code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "Internal Server Error",
    }
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    version: String,
    parse_error: Option<String>,
}

impl HttpRequest {
    fn method_label(&self) -> &str {
        if self.method.is_empty() {
            "(invalid)"
        } else {
            &self.method
        }
    }

    fn path_label(&self) -> &str {
        if self.path.is_empty() {
            "(invalid)"
        } else {
            &self.path
        }
    }

    fn invalid() -> Self {
        Self {
            method: String::new(),
            path: String::new(),
            version: String::new(),
            parse_error: Some("invalid HTTP request line".to_string()),
        }
    }

    fn too_large() -> Self {
        Self {
            method: String::new(),
            path: String::new(),
            version: String::new(),
            parse_error: Some("request headers exceeded the size limit".to_string()),
        }
    }
}

#[derive(Debug)]
struct HttpResponse {
    status_code: u16,
    body: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use serde_json::Value;
    use tentgent_core::{
        daemon::DaemonProcessMetadata,
        server::{ServerManager, ServerRunRequest},
    };

    use super::*;

    #[test]
    fn healthz_returns_ok_payload() {
        let request = HttpRequest {
            method: "GET".to_string(),
            path: "/healthz".to_string(),
            version: "HTTP/1.1".to_string(),
            parse_error: None,
        };
        let state = state_for(unique_home("healthz"));
        let response = route_request(&request, &state);
        let body: Value = serde_json::from_slice(&response.body).expect("json");

        assert_eq!(response.status_code, 200);
        assert_eq!(body["status"], "ok");
        assert_eq!(body["service"], "tentgent-daemon");
    }

    #[test]
    fn status_returns_daemon_metadata() {
        let request = HttpRequest {
            method: "GET".to_string(),
            path: "/v1/status".to_string(),
            version: "HTTP/1.1".to_string(),
            parse_error: None,
        };
        let state = state_for(unique_home("status"));
        let response = route_request(&request, &state);
        let body: Value = serde_json::from_slice(&response.body).expect("json");

        assert_eq!(response.status_code, 200);
        assert_eq!(body["status"], "running");
        assert_eq!(body["host"], "127.0.0.1");
        assert_eq!(body["port"], 8790);
        assert_eq!(body["pid"], 1234);
    }

    #[test]
    fn unknown_route_returns_json_error() {
        let request = HttpRequest {
            method: "GET".to_string(),
            path: "/v1/missing".to_string(),
            version: "HTTP/1.1".to_string(),
            parse_error: None,
        };
        let state = state_for(unique_home("missing-route"));
        let response = route_request(&request, &state);
        let body: Value = serde_json::from_slice(&response.body).expect("json");

        assert_eq!(response.status_code, 404);
        assert_eq!(body["error"], "not_found");
    }

    #[test]
    fn models_returns_empty_array_for_isolated_home() {
        let request = get("/v1/models");
        let state = state_for(unique_home("models-empty"));
        let response = route_request(&request, &state);
        let body: Value = serde_json::from_slice(&response.body).expect("json");

        assert_eq!(response.status_code, 200);
        assert_eq!(body["models"].as_array().expect("models").len(), 0);
    }

    #[test]
    fn adapters_returns_empty_array_for_isolated_home() {
        let request = get("/v1/adapters");
        let state = state_for(unique_home("adapters-empty"));
        let response = route_request(&request, &state);
        let body: Value = serde_json::from_slice(&response.body).expect("json");

        assert_eq!(response.status_code, 200);
        assert_eq!(body["adapters"].as_array().expect("adapters").len(), 0);
    }

    #[test]
    fn datasets_returns_empty_array_for_isolated_home() {
        let request = get("/v1/datasets");
        let state = state_for(unique_home("datasets-empty"));
        let response = route_request(&request, &state);
        let body: Value = serde_json::from_slice(&response.body).expect("json");

        assert_eq!(response.status_code, 200);
        assert_eq!(body["datasets"].as_array().expect("datasets").len(), 0);
    }

    #[test]
    fn servers_returns_stored_server_summaries() {
        let home = unique_home("servers-list");
        let manager = ServerManager::new(Some(&home)).expect("server manager");
        let outcome = manager
            .prepare_run(ServerRunRequest {
                runtime_ref: "openai:gpt-4.1-mini".to_string(),
                host: Some("127.0.0.1".to_string()),
                port: Some(8791),
                lazy_load: true,
                idle_seconds: Some(60),
            })
            .expect("server spec");

        let request = get("/v1/servers");
        let state = state_for(home);
        let response = route_request(&request, &state);
        let body: Value = serde_json::from_slice(&response.body).expect("json");
        let servers = body["servers"].as_array().expect("servers");

        assert_eq!(response.status_code, 200);
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0]["server_ref"], outcome.spec.server_ref);
        assert_eq!(servers[0]["runtime_kind"], "cloud");
        assert_eq!(servers[0]["provider"], "openai");
        assert_eq!(servers[0]["provider_model"], "gpt-4.1-mini");
        assert_eq!(servers[0]["running"], false);
    }

    #[test]
    fn server_inspect_accepts_short_ref() {
        let home = unique_home("server-inspect");
        let manager = ServerManager::new(Some(&home)).expect("server manager");
        let outcome = manager
            .prepare_run(ServerRunRequest {
                runtime_ref: "anthropic:claude-3-5-sonnet-latest".to_string(),
                host: Some("127.0.0.1".to_string()),
                port: Some(8792),
                lazy_load: false,
                idle_seconds: None,
            })
            .expect("server spec");

        let request = get(&format!("/v1/servers/{}", outcome.spec.short_ref));
        let state = state_for(home.clone());
        let response = route_request(&request, &state);
        let body: Value = serde_json::from_slice(&response.body).expect("json");

        assert_eq!(response.status_code, 200);
        assert_eq!(body["server"]["server_ref"], outcome.spec.server_ref);
        assert_eq!(body["server"]["home_dir"], path_string(&home));
        assert_eq!(body["server"]["runtime_kind"], "cloud");
        assert_eq!(body["server"]["provider"], "anthropic");
    }

    #[test]
    fn missing_server_returns_json_404() {
        let request = get("/v1/servers/missing");
        let state = state_for(unique_home("server-missing"));
        let response = route_request(&request, &state);
        let body: Value = serde_json::from_slice(&response.body).expect("json");

        assert_eq!(response.status_code, 404);
        assert_eq!(body["error"], "not_found");
    }

    fn get(path: &str) -> HttpRequest {
        HttpRequest {
            method: "GET".to_string(),
            path: path.to_string(),
            version: "HTTP/1.1".to_string(),
            parse_error: None,
        }
    }

    fn state_for(home: PathBuf) -> DaemonHttpState {
        DaemonHttpState::new(inspection(home))
    }

    fn inspection(home: PathBuf) -> DaemonInspection {
        DaemonInspection {
            home_dir: home.clone(),
            runtime_dir: home.join("runtime"),
            log_dir: home.join("logs"),
            process_path: home.join("runtime/daemon.toml"),
            pid_path: home.join("runtime/tentgent.pid"),
            stdout_log_path: home.join("logs/daemon.stdout.log"),
            stderr_log_path: home.join("logs/daemon.stderr.log"),
            running: true,
            process: Some(DaemonProcessMetadata {
                pid: 1234,
                host: "127.0.0.1".to_string(),
                port: 8790,
                started_at: "2026-05-01T00:00:00Z".to_string(),
            }),
        }
    }

    fn unique_home(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("tentgent-http-test-{label}-{nanos}"))
    }
}
