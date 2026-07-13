use std::net::SocketAddr;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use tentgent_kernel::{
    features::runtime::{
        domain::PythonRuntimeResolutionInput,
        infra::{StdPythonRuntimeResolver, StdRuntimeExecutableResolver},
        ports::PythonRuntimeResolver,
    },
    foundation::layout::{
        LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
    },
};

mod cache;
mod error;
mod handlers;
mod state;

#[cfg(test)]
mod tests;

pub use state::ClusterServerRuntimeConfig;

use cache::ClusterDefinitionCache;
use handlers::{
    audio_transcription, claude_messages, embeddings, gemini_generate_content, native_chat,
    openai_chat_completions, rerank, unsupported_path, vision_chat,
};
use state::ClusterServerState;

pub async fn run_cluster_server_runtime(config: ClusterServerRuntimeConfig) -> miette::Result<()> {
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .map_err(|err| miette::miette!("invalid cluster server bind address: {err}"))?;
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
    let definitions = ClusterDefinitionCache::load(&layout, config.cluster_ref.clone())
        .map_err(|err| miette::miette!("{err}"))?;
    let state = ClusterServerState {
        launch_policy: config
            .idle_seconds
            .map(tentgent_kernel::features::runtime::infra::ModelRuntimeDaemonLaunchPolicy::with_idle_keep_alive_seconds)
            .unwrap_or_default(),
        config,
        layout,
        runtime,
        executable_resolver: StdRuntimeExecutableResolver,
        supervisor: tentgent_kernel::features::runtime::infra::ModelRuntimeDaemonSupervisor::new(),
        client: reqwest::Client::new(),
        definitions,
    };
    let router = cluster_router(state);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|err| miette::miette!("cluster server proxy bind failed: {err}"))?;
    axum::serve(listener, router)
        .await
        .map_err(|err| miette::miette!("cluster server proxy failed: {err}"))
}

fn cluster_router(state: ClusterServerState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/chat/completions", post(openai_chat_completions))
        .route("/v1/messages", post(claude_messages))
        .route("/v1beta/models/{*operation}", post(gemini_generate_content))
        .route("/v1/chat", post(native_chat))
        .route("/v1/chat/stream", post(native_chat))
        .route("/v1/embeddings", post(embeddings))
        .route("/v1/rerank", post(rerank))
        .route("/v1/audio/transcriptions", post(audio_transcription))
        .route("/v1/vision/chat", post(vision_chat))
        .fallback(unsupported_path)
        .with_state(state)
}

async fn healthz(State(state): State<ClusterServerState>) -> Response {
    match state.definitions.current() {
        Ok(snapshot) => {
            let chat_target = snapshot
                .definition
                .routes
                .get(&tentgent_kernel::features::cluster::domain::ClusterRouteKey::Chat);
            let chat_local = matches!(
                chat_target,
                Some(
                    tentgent_kernel::features::cluster::domain::ClusterRouteTarget::LocalModel { .. }
                )
            );
            let status = if chat_local {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            };
            (
                status,
                Json(json!({
                    "ok": chat_local,
                    "runtime_kind": "cluster-proxy",
                    "server_ref": state.config.server_ref,
                    "runtime_home": state.layout.home_dir.display().to_string(),
                    "cluster_ref": state.config.cluster_ref,
                    "definition_hash": snapshot.hash,
                    "routes": snapshot.definition.routes.keys().map(|route| route.as_str()).collect::<Vec<_>>(),
                    "backend": "model-runtime-daemon"
                })),
            )
                .into_response()
        }
        Err(error) => error.into_response(),
    }
}
