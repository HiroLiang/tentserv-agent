use std::{
    future::{Future, IntoFuture},
    net::SocketAddr,
    path::PathBuf,
};

use axum::{
    extract::State,
    middleware,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use tentgent_kernel::{
    features::{
        runtime::{
            domain::PythonRuntimeResolutionInput,
            infra::{
                ModelRuntimeDaemonLaunchPolicy, ModelRuntimeDaemonSupervisor,
                StdPythonRuntimeResolver, StdRuntimeExecutableResolver,
            },
            ports::PythonRuntimeResolver,
        },
        server::{domain::ServerCapability, options::LoadMode},
    },
    foundation::layout::{
        LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
    },
};

use super::{
    claude_messages,
    error::LocalServerError,
    gemini_generate_content, image_generations, managed_native_chat, managed_native_chat_stream,
    openai_chat_completions, openai_embeddings, proxy_request,
    startup::{admit_ready_request, prepare_local_startup, StartupReadiness},
};

pub(in crate::server) const PROXY_BODY_LIMIT_BYTES: usize = 256 * 1024 * 1024;
pub(in crate::server) const RUNTIME_CHAT_PATH: &str = "/v1/chat";
pub(in crate::server) const RUNTIME_CHAT_STREAM_PATH: &str = "/v1/chat/stream";
pub(in crate::server) const RUNTIME_EMBEDDINGS_PATH: &str = "/v1/embeddings";
pub(in crate::server) const RUNTIME_IMAGE_GENERATIONS_PATH: &str = "/v1/images/generations";

#[derive(Debug, Clone)]
pub struct LocalServerRuntimeConfig {
    pub server_ref: String,
    pub capability: ServerCapability,
    pub model_ref: String,
    pub runtime_profile: Option<String>,
    pub host: String,
    pub port: u16,
    pub runtime_home: Option<PathBuf>,
    pub runtime_idle_seconds: u64,
    pub model_idle_seconds: u64,
    pub load_mode: LoadMode,
}

#[derive(Clone)]
pub(in crate::server) struct LocalServerState {
    pub(in crate::server) config: LocalServerRuntimeConfig,
    pub(in crate::server) layout: tentgent_kernel::foundation::layout::RuntimeLayout,
    pub(in crate::server) runtime: tentgent_kernel::features::runtime::domain::PythonRuntimeLayout,
    pub(in crate::server) executable_resolver: StdRuntimeExecutableResolver,
    pub(in crate::server) supervisor: ModelRuntimeDaemonSupervisor,
    pub(in crate::server) client: reqwest::Client,
    pub(in crate::server) launch_policy: ModelRuntimeDaemonLaunchPolicy,
    pub(in crate::server) readiness: StartupReadiness,
}

pub async fn run_local_server_runtime(config: LocalServerRuntimeConfig) -> miette::Result<()> {
    config
        .load_mode
        .ensure_supported(config.capability)
        .map_err(|error| miette::miette!("{error}"))?;
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
        readiness: StartupReadiness::new(config.load_mode),
        launch_policy: ModelRuntimeDaemonLaunchPolicy::new(
            config.runtime_idle_seconds,
            config.model_idle_seconds,
        )
        .map_err(|error| miette::miette!(error))?,
        config,
        layout,
        runtime,
        executable_resolver: StdRuntimeExecutableResolver,
        supervisor: ModelRuntimeDaemonSupervisor::new(),
        client: reqwest::Client::new(),
    };
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|err| miette::miette!("local server proxy bind failed: {err}"))?;
    serve_with_startup(state.clone(), listener, prepare_local_startup(&state)).await
}

pub(super) fn local_router(state: LocalServerState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/chat/completions", post(openai_chat_completions))
        .route("/v1/chat", post(managed_native_chat))
        .route("/v1/chat/stream", post(managed_native_chat_stream))
        .route("/v1/messages", post(claude_messages))
        .route("/v1beta/models/{*operation}", post(gemini_generate_content))
        .route("/v1/embeddings", post(openai_embeddings))
        .route("/v1/images/generations", post(image_generations))
        .fallback(proxy_request)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            admit_ready_request,
        ))
        .with_state(state)
}

pub(super) async fn serve_with_startup(
    state: LocalServerState,
    listener: tokio::net::TcpListener,
    startup: impl Future<Output = Result<(), LocalServerError>>,
) -> miette::Result<()> {
    let readiness = state.readiness.clone();
    let serving = axum::serve(listener, local_router(state)).into_future();
    tokio::pin!(serving);
    tokio::select! {
        result = startup => {
            result.map_err(|error| miette::miette!("local server startup failed: {}", error.message))?;
            readiness.mark_ready();
        }
        result = &mut serving => return result.map_err(|err| miette::miette!("local server proxy failed: {err}")),
    }
    serving
        .await
        .map_err(|err| miette::miette!("local server proxy failed: {err}"))
}

async fn healthz(State(state): State<LocalServerState>) -> Json<serde_json::Value> {
    let ready = state.readiness.is_ready();
    Json(json!({
        "ok": ready,
        "ready": ready,
        "status": if ready { "ready" } else { "starting" },
        "load_mode": if state.config.load_mode == LoadMode::Lazy { "lazy" } else { "eager" },
        "runtime_kind": "local-proxy",
        "server_ref": state.config.server_ref,
        "process_token": tentgent_kernel::features::server::infra::server_process_token_from_env(),
        "runtime_home": state.layout.home_dir.display().to_string(),
        "capability": state.config.capability.as_str(),
        "model_ref": state.config.model_ref,
        "runtime_profile": state.config.runtime_profile,
        "runtime_idle_seconds": state.config.runtime_idle_seconds,
        "model_idle_seconds": state.config.model_idle_seconds,
        "backend": "model-runtime-daemon"
    }))
}
