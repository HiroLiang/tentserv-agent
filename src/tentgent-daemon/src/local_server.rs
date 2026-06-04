use std::{collections::VecDeque, convert::Infallible, net::SocketAddr, path::PathBuf};

use axum::{
    body::{to_bytes, Body, Bytes},
    extract::{Request as AxumRequest, State},
    http::{header, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures_util::{stream, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tentgent_kernel::{
    features::{
        embedding::domain::EmbeddingInput,
        runtime::{
            domain::PythonRuntimeResolutionInput,
            infra::{
                ModelRuntimeCapability, ModelRuntimeDaemonEndpoint, ModelRuntimeDaemonLaunchPolicy,
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

use crate::{
    provider_compat::{
        OpenAiChatCompatFields, OpenAiTextMessage, ProviderChatTextMessage, ProviderCompatRejection,
    },
    time::unix_timestamp_seconds,
};

const PROXY_BODY_LIMIT_BYTES: usize = 256 * 1024 * 1024;
const RUNTIME_CHAT_PATH: &str = "/internal/v1/chat";
const RUNTIME_CHAT_STREAM_PATH: &str = "/internal/v1/chat/stream";
const RUNTIME_EMBEDDINGS_PATH: &str = "/internal/v1/embeddings";

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
        .route("/v1/chat/completions", post(openai_chat_completions))
        .route("/v1/embeddings", post(openai_embeddings))
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
    let endpoint = ensure_model_endpoint(&state).await?;
    let path_and_query = request
        .uri()
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or("/");
    let path_and_query = runtime_upstream_path_and_query(path_and_query);
    let target_url = format!(
        "{}{}",
        endpoint.base_url.trim_end_matches('/'),
        path_and_query
    );
    forward_to_runtime(&state.client, request, &target_url).await
}

async fn openai_chat_completions(
    State(state): State<LocalServerState>,
    Json(request): Json<LocalOpenAiChatCompletionRequest>,
) -> Result<Response, LocalServerError> {
    ensure_local_provider_capability(
        state.config.capability,
        ServerCapability::Chat,
        "OpenAI-compatible local chat completions",
    )?;
    let endpoint = ensure_model_endpoint(&state).await?;
    openai_chat_completions_to_upstream(
        &state.client,
        request,
        &endpoint.base_url,
        &state.config.model_ref,
        state.config.capability,
    )
    .await
}

async fn openai_embeddings(
    State(state): State<LocalServerState>,
    Json(request): Json<Value>,
) -> Result<Response, LocalServerError> {
    ensure_local_provider_capability(
        state.config.capability,
        ServerCapability::Embedding,
        "local embeddings",
    )?;
    let endpoint = ensure_model_endpoint(&state).await?;
    if local_embedding_request_uses_openai_shape(&request) {
        let request = LocalOpenAiEmbeddingRequest::from_value(request)?;
        return openai_embeddings_to_upstream(
            &state.client,
            request,
            &endpoint.base_url,
            &state.config.model_ref,
            state.config.capability,
        )
        .await;
    }
    native_embedding_to_upstream(&state.client, request, &endpoint.base_url).await
}

async fn ensure_model_endpoint(
    state: &LocalServerState,
) -> Result<ModelRuntimeDaemonEndpoint, LocalServerError> {
    let capability = model_runtime_capability(state.config.capability);
    state
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
        .map_err(|err| LocalServerError::internal(err.to_string()))
}

fn ensure_local_provider_capability(
    actual: ServerCapability,
    required: ServerCapability,
    route: &str,
) -> Result<(), LocalServerError> {
    if actual == required {
        return Ok(());
    }
    Err(ProviderCompatRejection::unsupported_capability(format!(
        "{route} requires a {} local server; this server is bound to {}",
        required.as_str(),
        actual.as_str()
    ))
    .into())
}

async fn openai_chat_completions_to_upstream(
    client: &reqwest::Client,
    request: LocalOpenAiChatCompletionRequest,
    upstream_base_url: &str,
    bound_model_ref: &str,
    capability: ServerCapability,
) -> Result<Response, LocalServerError> {
    if capability != ServerCapability::Chat {
        return Err(ProviderCompatRejection::unsupported_capability(format!(
            "OpenAI-compatible local chat requires a chat server; this server is bound to {}",
            capability.as_str()
        ))
        .into());
    }
    let stream = request.stream.unwrap_or(false);
    let native_request = request.into_native_chat_request()?;
    let route = if stream {
        RUNTIME_CHAT_STREAM_PATH
    } else {
        RUNTIME_CHAT_PATH
    };
    let upstream = client
        .post(format!(
            "{}{}",
            upstream_base_url.trim_end_matches('/'),
            route
        ))
        .json(&native_request)
        .send()
        .await
        .map_err(|err| {
            LocalServerError::bad_gateway(format!("model runtime proxy failed: {err}"))
        })?;
    if stream {
        openai_stream_response_from_upstream(upstream, bound_model_ref).await
    } else {
        openai_response_from_upstream(upstream, bound_model_ref).await
    }
}

async fn openai_embeddings_to_upstream(
    client: &reqwest::Client,
    request: LocalOpenAiEmbeddingRequest,
    upstream_base_url: &str,
    bound_model_ref: &str,
    capability: ServerCapability,
) -> Result<Response, LocalServerError> {
    if capability != ServerCapability::Embedding {
        return Err(ProviderCompatRejection::unsupported_capability(format!(
            "OpenAI-compatible local embeddings require an embedding server; this server is bound to {}",
            capability.as_str()
        ))
        .into());
    }
    let upstream = client
        .post(format!(
            "{}{}",
            upstream_base_url.trim_end_matches('/'),
            RUNTIME_EMBEDDINGS_PATH
        ))
        .json(&NativeLocalEmbeddingRequest {
            input: request.input,
        })
        .send()
        .await
        .map_err(|err| {
            LocalServerError::bad_gateway(format!("model runtime proxy failed: {err}"))
        })?;
    openai_embedding_response_from_upstream(upstream, bound_model_ref).await
}

async fn native_embedding_to_upstream(
    client: &reqwest::Client,
    request: Value,
    upstream_base_url: &str,
) -> Result<Response, LocalServerError> {
    let upstream = client
        .post(format!(
            "{}{}",
            upstream_base_url.trim_end_matches('/'),
            RUNTIME_EMBEDDINGS_PATH
        ))
        .json(&request)
        .send()
        .await
        .map_err(|err| {
            LocalServerError::bad_gateway(format!("model runtime proxy failed: {err}"))
        })?;
    response_from_upstream(upstream)
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

fn runtime_upstream_path_and_query(path_and_query: &str) -> String {
    let (path, query) = path_and_query
        .split_once('?')
        .map_or((path_and_query, None), |(path, query)| (path, Some(query)));
    let path = match path {
        "/v1/audio/transcriptions" => "/internal/v1/audio/transcriptions",
        "/v1/audio/speech" => "/internal/v1/audio/speech",
        "/v1/chat" => RUNTIME_CHAT_PATH,
        "/v1/chat/stream" => RUNTIME_CHAT_STREAM_PATH,
        "/v1/embeddings" => RUNTIME_EMBEDDINGS_PATH,
        "/v1/images/generations" => "/internal/v1/images/generations",
        "/v1/images/transforms" => "/internal/v1/images/transforms",
        "/v1/images/inpaint" => "/internal/v1/images/inpaint",
        "/v1/images/control" => "/internal/v1/images/control",
        "/v1/rerank" => "/internal/v1/rerank",
        "/v1/tuning/lora/runs" => "/internal/v1/tuning/lora/runs",
        "/v1/video/understanding" => "/internal/v1/video/understanding",
        "/v1/vision/chat" => "/internal/v1/vision/chat",
        _ => path,
    };
    match query {
        Some(query) => format!("{path}?{query}"),
        None => path.to_string(),
    }
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

async fn openai_embedding_response_from_upstream(
    upstream: reqwest::Response,
    bound_model_ref: &str,
) -> Result<Response, LocalServerError> {
    if !upstream.status().is_success() {
        return response_from_upstream(upstream);
    }
    let response = upstream
        .json::<NativeLocalEmbeddingResponse>()
        .await
        .map_err(|err| {
            LocalServerError::bad_gateway(format!("decode embedding response failed: {err}"))
        })?;
    Ok(Json(json!({
        "object": "list",
        "data": response.data.into_iter().map(|item| {
            json!({
                "object": "embedding",
                "index": item.index,
                "embedding": item.embedding
            })
        }).collect::<Vec<_>>(),
        "model": if response.model_ref.is_empty() {
            bound_model_ref.to_string()
        } else {
            response.model_ref
        },
        "usage": null
    }))
    .into_response())
}

async fn openai_response_from_upstream(
    upstream: reqwest::Response,
    bound_model_ref: &str,
) -> Result<Response, LocalServerError> {
    if !upstream.status().is_success() {
        return response_from_upstream(upstream);
    }
    let response = upstream
        .json::<NativeLocalChatResponse>()
        .await
        .map_err(|err| {
            LocalServerError::bad_gateway(format!("decode chat response failed: {err}"))
        })?;
    Ok(Json(json!({
        "id": format!("chatcmpl-{}", unix_timestamp_seconds()),
        "object": "chat.completion",
        "created": unix_timestamp_seconds(),
        "model": bound_model_ref,
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": response.text},
            "finish_reason": "stop",
            "logprobs": null
        }],
        "usage": null
    }))
    .into_response())
}

async fn openai_stream_response_from_upstream(
    upstream: reqwest::Response,
    bound_model_ref: &str,
) -> Result<Response, LocalServerError> {
    if !upstream.status().is_success() {
        return response_from_upstream(upstream);
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from_stream(openai_stream_from_local_sse(
            upstream.bytes_stream(),
            bound_model_ref.to_string(),
        )))
        .map_err(|err| LocalServerError::bad_gateway(format!("build chat stream failed: {err}")))
}

fn openai_stream_from_local_sse<S, E>(
    upstream: S,
    bound_model_ref: String,
) -> impl Stream<Item = Result<Bytes, Infallible>>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
    E: std::fmt::Display,
{
    let mut pending = VecDeque::new();
    pending.push_back(openai_sse_json_string(&openai_stream_chunk(
        &bound_model_ref,
        Some(json!({"role": "assistant"})),
        None,
    )));
    stream::unfold(
        LocalOpenAiStreamState {
            upstream,
            bound_model_ref,
            buffer: String::new(),
            pending,
            upstream_done: false,
            sent_done: false,
        },
        |mut state| async move {
            loop {
                if let Some(chunk) = state.pending.pop_front() {
                    return Some((Ok(Bytes::from(chunk)), state));
                }
                if state.upstream_done {
                    if !state.sent_done {
                        state.sent_done = true;
                        return Some((
                            Ok(Bytes::from(openai_stream_done_string(
                                &state.bound_model_ref,
                                "stop",
                            ))),
                            state,
                        ));
                    }
                    return None;
                }
                match state.upstream.next().await {
                    Some(Ok(bytes)) => match std::str::from_utf8(&bytes) {
                        Ok(text) => {
                            state.buffer.push_str(text);
                            state.drain_complete_events();
                        }
                        Err(error) => {
                            state.push_error("chat_stream_failed", error.to_string());
                            state.upstream_done = true;
                        }
                    },
                    Some(Err(error)) => {
                        state.push_error("chat_stream_failed", error.to_string());
                        state.upstream_done = true;
                    }
                    None => {
                        state.upstream_done = true;
                        state.drain_remainder();
                    }
                }
            }
        },
    )
}

struct LocalOpenAiStreamState<S> {
    upstream: S,
    bound_model_ref: String,
    buffer: String,
    pending: VecDeque<String>,
    upstream_done: bool,
    sent_done: bool,
}

impl<S> LocalOpenAiStreamState<S> {
    fn drain_complete_events(&mut self) {
        while let Some(index) = self.buffer.find("\n\n") {
            let block = self.buffer[..index].to_string();
            self.buffer.drain(..index + 2);
            self.push_event_block(&block);
        }
    }

    fn drain_remainder(&mut self) {
        if self.buffer.trim().is_empty() {
            self.buffer.clear();
            return;
        }
        let block = std::mem::take(&mut self.buffer);
        self.push_event_block(&block);
    }

    fn push_event_block(&mut self, block: &str) {
        if let Some((event, data)) = local_sse_event(block) {
            let done = openai_chunks_for_local_event(
                &mut self.pending,
                &self.bound_model_ref,
                &event,
                data.as_ref(),
            );
            self.sent_done |= done;
        }
    }

    fn push_error(&mut self, code: &str, message: String) {
        self.pending.push_back(openai_sse_json_string(&json!({
            "error": {
                "message": message,
                "type": code,
                "code": code
            }
        })));
        self.pending.push_back(openai_done_marker_string());
        self.sent_done = true;
    }
}

fn local_sse_event(block: &str) -> Option<(String, Option<Value>)> {
    let mut event = None;
    let mut data_lines = Vec::new();
    for line in block.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.trim());
        }
    }
    event.map(|event| {
        let data = data_lines.join("\n");
        let data = if data.is_empty() {
            None
        } else {
            serde_json::from_str(&data).ok()
        };
        (event, data)
    })
}

fn openai_chunks_for_local_event(
    pending: &mut VecDeque<String>,
    bound_model_ref: &str,
    event: &str,
    data: Option<&Value>,
) -> bool {
    match event {
        "delta" => {
            let text = data
                .and_then(|value| value.get("text"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !text.is_empty() {
                pending.push_back(openai_sse_json_string(&openai_stream_chunk(
                    bound_model_ref,
                    Some(json!({"content": text})),
                    None,
                )));
            }
            false
        }
        "done" => {
            pending.push_back(openai_stream_done_string(bound_model_ref, "stop"));
            true
        }
        "error" | "canceled" => {
            pending.push_back(openai_sse_json_string(&openai_stream_error(data)));
            pending.push_back(openai_done_marker_string());
            true
        }
        _ => false,
    }
}

fn openai_stream_done_string(bound_model_ref: &str, finish_reason: &str) -> String {
    let mut output = openai_sse_json_string(&openai_stream_chunk(
        bound_model_ref,
        Some(json!({})),
        Some(finish_reason),
    ));
    output.push_str(&openai_done_marker_string());
    output
}

fn openai_done_marker_string() -> String {
    "data: [DONE]\n\n".to_string()
}

fn openai_sse_json_string(value: &Value) -> String {
    let mut output = String::new();
    output.push_str("data: ");
    output.push_str(&value.to_string());
    output.push_str("\n\n");
    output
}

fn openai_stream_chunk(
    bound_model_ref: &str,
    delta: Option<Value>,
    finish_reason: Option<&str>,
) -> Value {
    json!({
        "id": format!("chatcmpl-{}", unix_timestamp_seconds()),
        "object": "chat.completion.chunk",
        "created": unix_timestamp_seconds(),
        "model": bound_model_ref,
        "choices": [{
            "index": 0,
            "delta": delta,
            "finish_reason": finish_reason,
            "logprobs": null
        }],
        "usage": null
    })
}

fn openai_stream_error(data: Option<&Value>) -> Value {
    let code = data
        .and_then(|value| value.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("chat_model_failed");
    let message = data
        .and_then(|value| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("local chat stream failed");
    json!({
        "error": {
            "message": message,
            "type": code,
            "code": code
        }
    })
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

#[derive(Debug, Deserialize)]
struct LocalOpenAiChatCompletionRequest {
    model: Option<String>,
    messages: Vec<OpenAiTextMessage>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    stream: Option<bool>,
    adapter_ref: Option<String>,
    #[serde(flatten)]
    compat: OpenAiChatCompatFields,
}

impl LocalOpenAiChatCompletionRequest {
    fn into_native_chat_request(self) -> Result<NativeLocalChatRequest, LocalServerError> {
        let _caller_model = self.model.as_deref();
        self.compat.reject_unsupported()?;
        if self.adapter_ref.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "local OpenAI-compatible chat does not support adapter_ref yet",
            )
            .into());
        }
        if self.messages.is_empty() {
            return Err(LocalServerError::bad_request(
                "bad_request",
                "OpenAI-compatible chat requests must contain at least one message",
            ));
        }
        let max_tokens = self.max_tokens.or(self.compat.max_completion_tokens());
        let messages = self
            .messages
            .into_iter()
            .map(OpenAiTextMessage::into_text_message)
            .map(|message| message.map(NativeLocalChatMessage::from))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(NativeLocalChatRequest {
            messages,
            max_tokens,
            temperature: self.temperature,
        })
    }
}

#[derive(Debug)]
struct LocalOpenAiEmbeddingRequest {
    input: Vec<String>,
}

impl LocalOpenAiEmbeddingRequest {
    fn from_value(value: Value) -> Result<Self, LocalServerError> {
        let object = value.as_object().ok_or_else(|| {
            LocalServerError::bad_request("bad_request", "request body must be a JSON object")
        })?;
        let unknown = object
            .keys()
            .filter(|key| {
                !matches!(
                    key.as_str(),
                    "model" | "input" | "dimensions" | "encoding_format" | "user"
                )
            })
            .map(String::as_str)
            .collect::<Vec<_>>();
        if !unknown.is_empty() {
            return Err(ProviderCompatRejection::unsupported_field(format!(
                "unsupported OpenAI-compatible embedding request fields: {}",
                unknown.join(", ")
            ))
            .into());
        }
        if object.get("model").is_some_and(|value| !value.is_string()) {
            return Err(LocalServerError::bad_request(
                "bad_request",
                "`model` must be a string when present",
            ));
        }
        reject_unsupported_local_embedding_fields(object)?;
        let input = object
            .get("input")
            .ok_or_else(|| LocalServerError::bad_request("bad_request", "`input` is required"))?;
        Ok(Self {
            input: local_embedding_input(input)?,
        })
    }
}

fn local_embedding_request_uses_openai_shape(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object.get("model").is_some_and(Value::is_string)
        || object.get("dimensions").is_some()
        || object.get("encoding_format").is_some()
        || object.get("user").is_some()
}

fn reject_unsupported_local_embedding_fields(
    object: &serde_json::Map<String, Value>,
) -> Result<(), LocalServerError> {
    if object.get("dimensions").is_some() {
        return Err(ProviderCompatRejection::unsupported_field(
            "OpenAI-compatible local embeddings do not support dimensions overrides yet",
        )
        .into());
    }
    if let Some(format) = object.get("encoding_format") {
        match format.as_str() {
            Some("float") => {}
            Some("base64") => {
                return Err(ProviderCompatRejection::unsupported_field(
                    "OpenAI-compatible local embeddings do not support base64 encoding yet",
                )
                .into())
            }
            _ => {
                return Err(LocalServerError::bad_request(
                    "bad_request",
                    "`encoding_format` must be `float` or `base64`",
                ))
            }
        }
    }
    if object.get("user").is_some() {
        return Err(ProviderCompatRejection::unsupported_field(
            "OpenAI-compatible local embeddings do not support user tracking metadata",
        )
        .into());
    }
    Ok(())
}

fn local_embedding_input(value: &Value) -> Result<Vec<String>, LocalServerError> {
    let items = match value {
        Value::String(item) => vec![item.clone()],
        Value::Array(items) => items
            .iter()
            .map(|item| {
                item.as_str().map(str::to_string).ok_or_else(|| {
                    LocalServerError::bad_request(
                        "bad_request",
                        "`input` must be a string or string array",
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(LocalServerError::bad_request(
                "bad_request",
                "`input` must be a string or string array",
            ))
        }
    };

    EmbeddingInput::new(items)
        .map(|input| input.items)
        .map_err(|err| LocalServerError::bad_request("bad_request", err.to_string()))
}

#[derive(Debug, Serialize)]
struct NativeLocalEmbeddingRequest {
    input: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct NativeLocalEmbeddingResponse {
    model_ref: String,
    data: Vec<NativeLocalEmbeddingItem>,
}

#[derive(Debug, Deserialize)]
struct NativeLocalEmbeddingItem {
    index: usize,
    embedding: Vec<f32>,
}

#[derive(Debug, Serialize)]
struct NativeLocalChatRequest {
    messages: Vec<NativeLocalChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
struct NativeLocalChatMessage {
    role: String,
    content: String,
}

impl From<ProviderChatTextMessage> for NativeLocalChatMessage {
    fn from(message: ProviderChatTextMessage) -> Self {
        Self {
            role: message.role,
            content: message.content,
        }
    }
}

#[derive(Debug, Deserialize)]
struct NativeLocalChatResponse {
    text: String,
}

#[derive(Debug)]
struct LocalServerError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl LocalServerError {
    fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code,
            message: message.into(),
        }
    }

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

impl From<ProviderCompatRejection> for LocalServerError {
    fn from(rejection: ProviderCompatRejection) -> Self {
        let (code, message) = rejection.into_parts();
        Self::bad_request(code, message)
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
    use std::sync::{Arc, Mutex};

    use axum::{
        body::Body,
        extract::{OriginalUri, State as AxumState},
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

    #[tokio::test]
    async fn openai_chat_completions_maps_local_request_and_response() {
        async fn chat(body: String) -> Json<Value> {
            Json(json!({
                "task_ref": "task-1",
                "status": "completed",
                "text": body,
            }))
        }

        let (base_url, _task) =
            spawn_test_server(Router::new().route(RUNTIME_CHAT_PATH, post(chat))).await;
        let request: LocalOpenAiChatCompletionRequest = serde_json::from_value(json!({
            "messages": [
                {"role": "developer", "content": [{"type": "text", "text": "Follow policy."}]},
                {"role": "user", "content": [{"type": "text", "text": "hi"}]}
            ],
            "max_completion_tokens": 12,
            "temperature": 0.2,
            "response_format": {"type": "text"},
            "modalities": ["text"],
            "tool_choice": "none",
            "function_call": "none",
            "parallel_tool_calls": false,
            "n": 1,
            "store": false
        }))
        .expect("request");

        let response = openai_chat_completions_to_upstream(
            &reqwest::Client::new(),
            request,
            &base_url,
            "local-model-ref",
            ServerCapability::Chat,
        )
        .await
        .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), PROXY_BODY_LIMIT_BYTES)
            .await
            .expect("body");
        let value: Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(value["object"], "chat.completion");
        assert_eq!(value["model"], "local-model-ref");

        let native_body = value["choices"][0]["message"]["content"]
            .as_str()
            .expect("native body");
        let native_body: Value = serde_json::from_str(native_body).expect("native json");
        assert_eq!(native_body["messages"][0]["role"], "system");
        assert_eq!(native_body["messages"][0]["content"], "Follow policy.");
        assert_eq!(native_body["messages"][1]["role"], "user");
        assert_eq!(native_body["messages"][1]["content"], "hi");
        assert_eq!(native_body["max_tokens"], 12);
        assert_eq!(native_body["temperature"], 0.2);
        assert!(native_body.get("model").is_none());
    }

    #[tokio::test]
    async fn openai_chat_completions_maps_local_stream_response() {
        async fn stream(
            AxumState(captured): AxumState<Arc<Mutex<Option<String>>>>,
            body: String,
        ) -> Response {
            use futures_util::stream;

            *captured.lock().expect("lock") = Some(body);
            let chunks = stream::iter([
                Ok::<_, std::convert::Infallible>(
                    "event: started\ndata: {\"task_ref\":\"task-1\"}\n\n",
                ),
                Ok("event: delta\ndata: {\"text\":\"one\"}\n\n"),
                Ok("event: delta\ndata: {\"text\":\" two\"}\n\n"),
                Ok("event: done\ndata: {\"task_ref\":\"task-1\",\"text\":\"one two\"}\n\n"),
            ]);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/event-stream")
                .body(Body::from_stream(chunks))
                .expect("stream response")
        }

        let captured = Arc::new(Mutex::new(None));
        let (base_url, _task) = spawn_test_server(
            Router::new()
                .route(RUNTIME_CHAT_STREAM_PATH, post(stream))
                .with_state(captured.clone()),
        )
        .await;
        let request: LocalOpenAiChatCompletionRequest = serde_json::from_value(json!({
            "messages": [{"role": "user", "content": "hi"}],
            "stream": true,
            "max_tokens": 8
        }))
        .expect("request");

        let response = openai_chat_completions_to_upstream(
            &reqwest::Client::new(),
            request,
            &base_url,
            "local-model-ref",
            ServerCapability::Chat,
        )
        .await
        .expect("response");

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
        let body = std::str::from_utf8(&body).expect("utf8");
        assert!(body.contains(r#""object":"chat.completion.chunk""#));
        assert!(body.contains(r#""model":"local-model-ref""#));
        assert!(body.contains(r#""role":"assistant""#));
        assert!(body.contains(r#""content":"one""#));
        assert!(body.contains(r#""content":" two""#));
        assert!(body.contains("data: [DONE]"));
        assert!(!body.contains("event: delta"));

        let captured = captured.lock().expect("lock").clone().expect("captured");
        let captured: Value = serde_json::from_str(&captured).expect("native json");
        assert_eq!(captured["messages"][0]["role"], "user");
        assert_eq!(captured["messages"][0]["content"], "hi");
        assert_eq!(captured["max_tokens"], 8);
    }

    #[tokio::test]
    async fn openai_chat_completions_rejects_non_chat_local_server() {
        let request: LocalOpenAiChatCompletionRequest = serde_json::from_value(json!({
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .expect("request");

        let error = openai_chat_completions_to_upstream(
            &reqwest::Client::new(),
            request,
            "http://127.0.0.1:1",
            "embedding-model-ref",
            ServerCapability::Embedding,
        )
        .await
        .expect_err("non-chat capability rejected");

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "unsupported_provider_capability");
    }

    #[tokio::test]
    async fn openai_embeddings_maps_local_request_and_response() {
        async fn embeddings(
            AxumState(captured): AxumState<Arc<Mutex<Option<String>>>>,
            body: String,
        ) -> Json<Value> {
            *captured.lock().expect("lock") = Some(body);
            Json(json!({
                "task_ref": "task-1",
                "status": "completed",
                "model_ref": "local-embedding-ref",
                "data": [
                    {"index": 0, "embedding": [0.1, 0.2]},
                    {"index": 1, "embedding": [0.3, 0.4]}
                ]
            }))
        }

        let captured = Arc::new(Mutex::new(None));
        let (base_url, _task) = spawn_test_server(
            Router::new()
                .route(RUNTIME_EMBEDDINGS_PATH, post(embeddings))
                .with_state(captured.clone()),
        )
        .await;
        let request = LocalOpenAiEmbeddingRequest::from_value(json!({
            "model": "text-embedding-3-small",
            "input": ["first", "second"],
            "encoding_format": "float"
        }))
        .expect("request");

        let response = openai_embeddings_to_upstream(
            &reqwest::Client::new(),
            request,
            &base_url,
            "bound-local-ref",
            ServerCapability::Embedding,
        )
        .await
        .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), PROXY_BODY_LIMIT_BYTES)
            .await
            .expect("body");
        let value: Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(value["object"], "list");
        assert_eq!(value["model"], "local-embedding-ref");
        assert_eq!(value["usage"], Value::Null);
        assert_eq!(value["data"][0]["object"], "embedding");
        assert_eq!(value["data"][0]["index"], 0);
        assert_eq!(value["data"][1]["object"], "embedding");
        assert_eq!(value["data"][1]["index"], 1);
        assert_embedding_values(&value["data"][0]["embedding"], &[0.1, 0.2]);
        assert_embedding_values(&value["data"][1]["embedding"], &[0.3, 0.4]);

        let captured = captured.lock().expect("lock").clone().expect("captured");
        let captured: Value = serde_json::from_str(&captured).expect("native json");
        assert_eq!(captured["input"], json!(["first", "second"]));
        assert!(captured.get("model").is_none());
        assert!(captured.get("encoding_format").is_none());
    }

    #[tokio::test]
    async fn openai_embeddings_rejects_non_embedding_local_server() {
        let request = LocalOpenAiEmbeddingRequest::from_value(json!({
            "model": "text-embedding-3-small",
            "input": "hello"
        }))
        .expect("request");

        let error = openai_embeddings_to_upstream(
            &reqwest::Client::new(),
            request,
            "http://127.0.0.1:1",
            "chat-model-ref",
            ServerCapability::Chat,
        )
        .await
        .expect_err("non-embedding capability rejected");

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "unsupported_provider_capability");
    }

    #[tokio::test]
    async fn native_embeddings_still_proxy_through_local_route() {
        async fn embeddings(body: String) -> Json<Value> {
            Json(json!({
                "proxied_body": body,
                "model_ref": "local-embedding-ref",
                "data": [{"index": 0, "embedding": [0.1, 0.2]}]
            }))
        }

        let (base_url, _task) =
            spawn_test_server(Router::new().route(RUNTIME_EMBEDDINGS_PATH, post(embeddings))).await;
        let response = native_embedding_to_upstream(
            &reqwest::Client::new(),
            json!({
                "input": ["native text"],
                "task_ref": "task-1"
            }),
            &base_url,
        )
        .await
        .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), PROXY_BODY_LIMIT_BYTES)
            .await
            .expect("body");
        let value: Value = serde_json::from_slice(&body).expect("json");
        let proxied_body = value["proxied_body"].as_str().expect("proxied body");
        let proxied_body: Value = serde_json::from_str(proxied_body).expect("native json");
        assert_eq!(proxied_body["input"], json!(["native text"]));
        assert_eq!(proxied_body["task_ref"], "task-1");
        assert_eq!(value["model_ref"], "local-embedding-ref");
    }

    #[test]
    fn local_embedding_shape_detection_preserves_native_body() {
        assert!(!local_embedding_request_uses_openai_shape(&json!({
            "input": "native text"
        })));
        assert!(!local_embedding_request_uses_openai_shape(&json!({
            "input": "native text",
            "model": {"model_ref": "native-model-record"}
        })));
        assert!(local_embedding_request_uses_openai_shape(&json!({
            "model": "text-embedding-3-small",
            "input": "hello"
        })));
        assert!(local_embedding_request_uses_openai_shape(&json!({
            "input": "hello",
            "encoding_format": "float"
        })));
    }

    #[test]
    fn runtime_upstream_path_maps_known_native_routes_to_internal_runtime() {
        assert_eq!(
            runtime_upstream_path_and_query("/v1/chat?stream=false"),
            "/internal/v1/chat?stream=false"
        );
        assert_eq!(
            runtime_upstream_path_and_query("/v1/embeddings"),
            RUNTIME_EMBEDDINGS_PATH
        );
        assert_eq!(
            runtime_upstream_path_and_query("/v1/images/generations"),
            "/internal/v1/images/generations"
        );
        assert_eq!(
            runtime_upstream_path_and_query("/v1/not-runtime"),
            "/v1/not-runtime"
        );
    }

    #[test]
    fn openai_embeddings_rejects_unsupported_local_fields() {
        let error = LocalOpenAiEmbeddingRequest::from_value(json!({
            "model": "text-embedding-3-small",
            "input": "hello",
            "dimensions": 512
        }))
        .expect_err("dimensions rejected");

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "unsupported_provider_field");
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

    fn assert_embedding_values(value: &Value, expected: &[f64]) {
        let values = value.as_array().expect("embedding array");
        assert_eq!(values.len(), expected.len());
        for (value, expected) in values.iter().zip(expected) {
            let value = value.as_f64().expect("embedding float");
            assert!((value - expected).abs() < 0.00001);
        }
    }
}
