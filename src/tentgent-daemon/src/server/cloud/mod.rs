use std::net::SocketAddr;

use axum::{
    extract::{DefaultBodyLimit, State},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use tentgent_kernel::features::auth::domain::Provider;

mod claude_messages;
mod embeddings;
mod error;
mod gemini_generate;
mod images;
mod native_chat;
mod openai_chat;
mod stream;

#[cfg(test)]
mod tests;

use claude_messages::claude_messages;
use embeddings::embeddings;
use gemini_generate::gemini_generate_content;
use images::images;
use native_chat::chat;
use openai_chat::openai_chat;

use crate::transport::rest::limits::media_upload_body_limit_bytes;

#[derive(Debug, Clone)]
pub struct CloudServerRuntimeConfig {
    pub server_ref: String,
    pub provider: Provider,
    pub provider_model: String,
    pub host: String,
    pub port: u16,
    pub runtime_home: Option<String>,
}

#[derive(Clone)]
pub(super) struct CloudServerState {
    pub(super) config: CloudServerRuntimeConfig,
    pub(super) secret: String,
}

pub async fn run_cloud_server_runtime(config: CloudServerRuntimeConfig) -> miette::Result<()> {
    let secret = std::env::var(config.provider.env_var()).map_err(|_| {
        miette::miette!(
            "{} is required to launch cloud server {}",
            config.provider.env_var(),
            config.server_ref
        )
    })?;
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .map_err(|err| miette::miette!("invalid cloud server bind address: {err}"))?;
    let state = CloudServerState { config, secret };
    let router = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/chat", post(chat))
        .route("/v1/chat/completions", post(openai_chat))
        .route("/v1/messages", post(claude_messages))
        .route("/v1beta/models/{*operation}", post(gemini_generate_content))
        .route("/v1/embeddings", post(embeddings))
        .route("/v1/images/generations", post(images))
        .layer(DefaultBodyLimit::max(media_upload_body_limit_bytes()))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|err| miette::miette!("cloud server bind failed: {err}"))?;
    axum::serve(listener, router)
        .await
        .map_err(|err| miette::miette!("cloud server failed: {err}"))
}

async fn healthz(State(state): State<CloudServerState>) -> Json<serde_json::Value> {
    Json(json!({
        "ok": true,
        "runtime_kind": "cloud",
        "server_ref": state.config.server_ref,
        "runtime_home": state.config.runtime_home,
        "provider": state.config.provider.cli_name(),
        "model": state.config.provider_model,
    }))
}
