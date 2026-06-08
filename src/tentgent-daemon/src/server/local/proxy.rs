use axum::{
    body::{to_bytes, Body},
    extract::{Request as AxumRequest, State},
    http::{header, Request, StatusCode},
    response::Response,
};

use super::{
    capability::ensure_model_endpoint, error::LocalServerError, LocalServerState,
    PROXY_BODY_LIMIT_BYTES, RUNTIME_CHAT_PATH, RUNTIME_CHAT_STREAM_PATH, RUNTIME_EMBEDDINGS_PATH,
    RUNTIME_IMAGE_GENERATIONS_PATH,
};

pub(super) async fn proxy_request(
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

pub(super) async fn forward_to_runtime(
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

pub(super) fn runtime_upstream_path_and_query(path_and_query: &str) -> String {
    let (path, query) = path_and_query
        .split_once('?')
        .map_or((path_and_query, None), |(path, query)| (path, Some(query)));
    let path = match path {
        "/v1/audio/transcriptions" => "/internal/v1/audio/transcriptions",
        "/v1/audio/speech" => "/internal/v1/audio/speech",
        "/v1/chat" => RUNTIME_CHAT_PATH,
        "/v1/chat/stream" => RUNTIME_CHAT_STREAM_PATH,
        "/v1/embeddings" => RUNTIME_EMBEDDINGS_PATH,
        "/v1/images/generations" => RUNTIME_IMAGE_GENERATIONS_PATH,
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

pub(super) fn response_from_upstream(
    upstream: reqwest::Response,
) -> Result<Response, LocalServerError> {
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
