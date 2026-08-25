use std::{net::SocketAddr, path::PathBuf};

use axum::{
    extract::State,
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
        server::domain::ServerCapability,
    },
    foundation::layout::{
        LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
    },
};

use super::{
    claude_messages, gemini_generate_content, image_generations, managed_native_chat,
    managed_native_chat_stream, openai_chat_completions, openai_embeddings, proxy_request,
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
    let router = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/chat/completions", post(openai_chat_completions))
        .route("/v1/chat", post(managed_native_chat))
        .route("/v1/chat/stream", post(managed_native_chat_stream))
        .route("/v1/messages", post(claude_messages))
        .route("/v1beta/models/{*operation}", post(gemini_generate_content))
        .route("/v1/embeddings", post(openai_embeddings))
        .route("/v1/images/generations", post(image_generations))
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
        "process_token": tentgent_kernel::features::server::infra::server_process_token_from_env(),
        "runtime_home": state.config.runtime_home.as_ref().map(|path| path.display().to_string()),
        "capability": state.config.capability.as_str(),
        "model_ref": state.config.model_ref,
        "runtime_profile": state.config.runtime_profile,
        "runtime_idle_seconds": state.config.runtime_idle_seconds,
        "model_idle_seconds": state.config.model_idle_seconds,
        "backend": "model-runtime-daemon"
    }))
}
