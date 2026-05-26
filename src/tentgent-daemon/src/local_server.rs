use std::net::SocketAddr;
use std::path::PathBuf;

use axum::{
    body::{to_bytes, Body},
    extract::{Request as AxumRequest, State},
    http::{header, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;
use tentgent_kernel::{
    features::{
        runtime::{
            domain::PythonRuntimeResolutionInput,
            infra::{
                ModelRuntimeCapability, ModelRuntimeDaemonLaunchPolicy,
                ModelRuntimeDaemonSupervisor, StdPythonRuntimeResolver,
                StdRuntimeExecutableResolver,
            },
            ports::PythonRuntimeResolver,
        },
        server::domain::ServerCapability,
    },
    foundation::layout::{
        LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
    },
};

const PROXY_BODY_LIMIT_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct LocalServerRuntimeConfig {
    pub server_ref: String,
    pub capability: ServerCapability,
    pub model_ref: String,
    pub host: String,
    pub port: u16,
    pub runtime_home: Option<PathBuf>,
    pub idle_seconds: Option<u64>,
}

#[derive(Clone)]
struct LocalServerState {
    config: LocalServerRuntimeConfig,
    layout: tentgent_kernel::foundation::layout::RuntimeLayout,
    runtime: tentgent_kernel::features::runtime::domain::PythonRuntimeLayout,
    executable_resolver: StdRuntimeExecutableResolver,
    supervisor: ModelRuntimeDaemonSupervisor,
    client: reqwest::Client,
    launch_policy: ModelRuntimeDaemonLaunchPolicy,
}

pub async fn run_local_server_runtime(config: LocalServerRuntimeConfig) -> miette::Result<()> {
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .map_err(|err| miette::miette!("invalid local server bind address: {err}"))?;
    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: config.runtime_home.clone(),
            data_root_dir: None,
        })
        .map_err(|err| miette::miette!("{err}"))?;
    let runtime = StdPythonRuntimeResolver
        .resolve_python_runtime(&layout, PythonRuntimeResolutionInput::default())
        .map_err(|err| miette::miette!("{err}"))?;
    let state = LocalServerState {
        launch_policy: config
            .idle_seconds
            .map(ModelRuntimeDaemonLaunchPolicy::with_idle_keep_alive_seconds)
            .unwrap_or_default(),
        config,
        layout,
        runtime,
        executable_resolver: StdRuntimeExecutableResolver,
        supervisor: ModelRuntimeDaemonSupervisor::new(),
        client: reqwest::Client::new(),
    };
    let router = Router::new()
        .route("/healthz", get(healthz))
        .fallback(proxy_request)
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|err| miette::miette!("local server proxy bind failed: {err}"))?;
    axum::serve(listener, router)
        .await
        .map_err(|err| miette::miette!("local server proxy failed: {err}"))
}

async fn healthz(State(state): State<LocalServerState>) -> Json<serde_json::Value> {
    Json(json!({
        "ok": true,
        "runtime_kind": "local-proxy",
        "server_ref": state.config.server_ref,
        "runtime_home": state.config.runtime_home.as_ref().map(|path| path.display().to_string()),
        "capability": state.config.capability.as_str(),
        "model_ref": state.config.model_ref,
        "idle_seconds": state.config.idle_seconds,
        "backend": "model-runtime-daemon"
    }))
}

async fn proxy_request(
    State(state): State<LocalServerState>,
    request: AxumRequest,
) -> Result<Response, LocalServerError> {
    let capability = model_runtime_capability(state.config.capability);
    let endpoint = state
        .supervisor
        .ensure_model_bound_with_policy(
            &state.layout,
            &state.runtime,
            &state.executable_resolver,
            capability,
            &state.config.model_ref,
            &state.launch_policy,
        )
        .await
        .map_err(|err| LocalServerError::internal(err.to_string()))?;
    let path_and_query = request
        .uri()
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or("/");
    let target_url = format!(
        "{}{}",
        endpoint.base_url.trim_end_matches('/'),
        path_and_query
    );
    forward_to_runtime(&state.client, request, &target_url).await
}

async fn forward_to_runtime(
    client: &reqwest::Client,
    request: Request<Body>,
    target_url: &str,
) -> Result<Response, LocalServerError> {
    let (parts, body) = request.into_parts();
    let body = to_bytes(body, PROXY_BODY_LIMIT_BYTES)
        .await
        .map_err(|err| {
            LocalServerError::bad_gateway(format!("read proxy request body failed: {err}"))
        })?;
    let method = reqwest::Method::from_bytes(parts.method.as_str().as_bytes())
        .map_err(|err| LocalServerError::bad_gateway(format!("invalid proxy method: {err}")))?;
    let mut builder = client.request(method, target_url).body(body);
    for (name, value) in &parts.headers {
        if should_proxy_request_header(name.as_str()) {
            builder = builder.header(name.as_str(), value);
        }
    }
    let upstream = builder.send().await.map_err(|err| {
        LocalServerError::bad_gateway(format!("model runtime proxy failed: {err}"))
    })?;
    response_from_upstream(upstream)
}

fn response_from_upstream(upstream: reqwest::Response) -> Result<Response, LocalServerError> {
    let status = upstream.status();
    let status = StatusCode::from_u16(status.as_u16())
        .map_err(|err| LocalServerError::bad_gateway(format!("invalid upstream status: {err}")))?;
    let mut response = Response::builder().status(status);
    for (name, value) in upstream.headers() {
        if should_proxy_response_header(name.as_str()) {
            response = response.header(name.as_str(), value);
        }
    }
    response
        .body(Body::from_stream(upstream.bytes_stream()))
        .map_err(|err| LocalServerError::bad_gateway(format!("build proxy response failed: {err}")))
}

fn should_proxy_request_header(name: &str) -> bool {
    !is_hop_by_hop_header(name) && !name.eq_ignore_ascii_case(header::HOST.as_str())
}

fn should_proxy_response_header(name: &str) -> bool {
    !is_hop_by_hop_header(name)
        && !name.eq_ignore_ascii_case(header::CONTENT_LENGTH.as_str())
        && !name.eq_ignore_ascii_case(header::TRANSFER_ENCODING.as_str())
}

fn is_hop_by_hop_header(name: &str) -> bool {
    name.eq_ignore_ascii_case(header::CONNECTION.as_str())
        || name.eq_ignore_ascii_case("keep-alive")
        || name.eq_ignore_ascii_case(header::PROXY_AUTHENTICATE.as_str())
        || name.eq_ignore_ascii_case(header::PROXY_AUTHORIZATION.as_str())
        || name.eq_ignore_ascii_case(header::TE.as_str())
        || name.eq_ignore_ascii_case(header::TRAILER.as_str())
        || name.eq_ignore_ascii_case(header::TRANSFER_ENCODING.as_str())
        || name.eq_ignore_ascii_case(header::UPGRADE.as_str())
}

fn model_runtime_capability(capability: ServerCapability) -> ModelRuntimeCapability {
    match capability {
        ServerCapability::AudioSpeech => ModelRuntimeCapability::AudioSpeech,
        ServerCapability::AudioTranscription => ModelRuntimeCapability::AudioTranscription,
        ServerCapability::Chat => ModelRuntimeCapability::Chat,
        ServerCapability::Embedding => ModelRuntimeCapability::Embedding,
        ServerCapability::ImageGeneration => ModelRuntimeCapability::ImageGeneration,
        ServerCapability::Rerank => ModelRuntimeCapability::Rerank,
        ServerCapability::VideoUnderstanding => ModelRuntimeCapability::VideoUnderstanding,
        ServerCapability::VisionChat => ModelRuntimeCapability::VisionChat,
    }
}

#[derive(Debug)]
struct LocalServerError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl LocalServerError {
    fn internal(message: String) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "local_proxy_failed",
            message,
        }
    }

    fn bad_gateway(message: String) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "model_runtime_proxy_failed",
            message,
        }
    }
}

impl IntoResponse for LocalServerError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({
                "error": self.code,
                "message": self.message,
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        extract::OriginalUri,
        http::{HeaderMap, Method, Request},
        routing::post,
        Router,
    };
    use serde_json::Value;
    use tentgent_kernel::foundation::net::http_url_from_host_port;

    use super::*;

    #[tokio::test]
    async fn forward_to_runtime_preserves_path_query_body_and_headers() {
        async fn echo(
            OriginalUri(uri): OriginalUri,
            headers: HeaderMap,
            body: String,
        ) -> Json<Value> {
            Json(json!({
                "path_query": uri.path_and_query().map(|value| value.as_str()).unwrap_or(""),
                "content_type": headers.get(header::CONTENT_TYPE).and_then(|value| value.to_str().ok()),
                "body": body,
            }))
        }

        let (base_url, _task) =
            spawn_test_server(Router::new().route("/v1/chat", post(echo))).await;
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/chat?trace=1")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"hello":"proxy"}"#))
            .expect("request");

        let response = forward_to_runtime(
            &reqwest::Client::new(),
            request,
            &format!("{base_url}/v1/chat?trace=1"),
        )
        .await
        .expect("proxy response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), PROXY_BODY_LIMIT_BYTES)
            .await
            .expect("body");
        let value: Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(value["path_query"], "/v1/chat?trace=1");
        assert_eq!(value["content_type"], "application/json");
        assert_eq!(value["body"], r#"{"hello":"proxy"}"#);
    }

    #[tokio::test]
    async fn forward_to_runtime_streams_upstream_body() {
        async fn stream() -> Response {
            use futures_util::stream;

            let chunks = stream::iter([
                Ok::<_, std::convert::Infallible>("event: delta\n"),
                Ok("data: one\n\n"),
                Ok("event: done\n"),
                Ok("data: {}\n\n"),
            ]);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/event-stream")
                .body(Body::from_stream(chunks))
                .expect("stream response")
        }

        let (base_url, _task) =
            spawn_test_server(Router::new().route("/v1/chat/stream", post(stream))).await;
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/chat/stream")
            .body(Body::from("{}"))
            .expect("request");

        let response = forward_to_runtime(
            &reqwest::Client::new(),
            request,
            &format!("{base_url}/v1/chat/stream"),
        )
        .await
        .expect("proxy response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("text/event-stream")
        );
        let body = to_bytes(response.into_body(), PROXY_BODY_LIMIT_BYTES)
            .await
            .expect("body");
        assert_eq!(
            std::str::from_utf8(&body).expect("utf8"),
            "event: delta\ndata: one\n\nevent: done\ndata: {}\n\n"
        );
    }

    async fn spawn_test_server(router: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.expect("serve");
        });
        (http_url_from_host_port("127.0.0.1", port), task)
    }
}
