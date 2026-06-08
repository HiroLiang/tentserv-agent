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

mod capability;
mod claude_messages;
mod error;
mod native;
mod openai_chat;
mod openai_embeddings;
mod openai_images;
mod proxy;
mod sse;

#[cfg(test)]
mod tests;

use claude_messages::claude_messages;
use openai_chat::openai_chat_completions;
use openai_embeddings::openai_embeddings;
use openai_images::image_generations;
use proxy::proxy_request;

pub(super) const PROXY_BODY_LIMIT_BYTES: usize = 256 * 1024 * 1024;
pub(super) const RUNTIME_CHAT_PATH: &str = "/v1/chat";
pub(super) const RUNTIME_CHAT_STREAM_PATH: &str = "/v1/chat/stream";
pub(super) const RUNTIME_EMBEDDINGS_PATH: &str = "/v1/embeddings";
pub(super) const RUNTIME_IMAGE_GENERATIONS_PATH: &str = "/v1/images/generations";

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
pub(super) struct LocalServerState {
    pub(super) config: LocalServerRuntimeConfig,
    pub(super) layout: tentgent_kernel::foundation::layout::RuntimeLayout,
    pub(super) runtime: tentgent_kernel::features::runtime::domain::PythonRuntimeLayout,
    pub(super) executable_resolver: StdRuntimeExecutableResolver,
    pub(super) supervisor: ModelRuntimeDaemonSupervisor,
    pub(super) client: reqwest::Client,
    pub(super) launch_policy: ModelRuntimeDaemonLaunchPolicy,
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
        .route("/v1/chat/completions", post(openai_chat_completions))
        .route("/v1/messages", post(claude_messages))
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
        "runtime_home": state.config.runtime_home.as_ref().map(|path| path.display().to_string()),
        "capability": state.config.capability.as_str(),
        "model_ref": state.config.model_ref,
        "idle_seconds": state.config.idle_seconds,
        "backend": "model-runtime-daemon"
    }))
}
