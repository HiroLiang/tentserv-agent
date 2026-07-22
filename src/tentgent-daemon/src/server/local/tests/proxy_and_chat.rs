use super::*;

#[tokio::test]
async fn forward_to_runtime_preserves_path_query_body_and_headers() {
    async fn echo(OriginalUri(uri): OriginalUri, headers: HeaderMap, body: String) -> Json<Value> {
        Json(json!({
            "path_query": uri.path_and_query().map(|value| value.as_str()).unwrap_or(""),
            "content_type": headers.get(header::CONTENT_TYPE).and_then(|value| value.to_str().ok()),
            "body": body,
        }))
    }

    let (base_url, _task) = spawn_test_server(Router::new().route("/v1/chat", post(echo))).await;
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
async fn openai_chat_completions_rejects_vision_input_before_local_proxy() {
    let request: LocalOpenAiChatCompletionRequest = serde_json::from_value(json!({
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": "Describe this image."},
                {"type": "image_url", "image_url": {"url": "https://example.com/cat.png", "detail": "low"}}
            ]
        }]
    }))
    .expect("request");

    let error = openai_chat_completions_to_upstream(
        &reqwest::Client::new(),
        request,
        "http://127.0.0.1:1",
        "local-model-ref",
        ServerCapability::Chat,
    )
    .await
    .expect_err("vision input unsupported");

    assert_eq!(error.status, StatusCode::BAD_REQUEST);
    assert_eq!(error.code, "unsupported_provider_content");
}

#[tokio::test]
async fn openai_chat_completions_rejects_audio_input_before_local_proxy() {
    let request: LocalOpenAiChatCompletionRequest = serde_json::from_value(json!({
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": "Transcribe this."},
                {"type": "input_audio", "input_audio": {"data": "AA==", "format": "wav"}}
            ]
        }]
    }))
    .expect("request");

    let error = openai_chat_completions_to_upstream(
        &reqwest::Client::new(),
        request,
        "http://127.0.0.1:1",
        "local-model-ref",
        ServerCapability::Chat,
    )
    .await
    .expect_err("audio input unsupported");

    assert_eq!(error.status, StatusCode::BAD_REQUEST);
    assert_eq!(error.code, "unsupported_provider_content");
}

#[tokio::test]
async fn openai_chat_completions_rejects_audio_output_before_local_proxy() {
    let request: LocalOpenAiChatCompletionRequest = serde_json::from_value(json!({
        "messages": [{"role": "user", "content": "hi"}],
        "modalities": ["text", "audio"],
        "audio": {"voice": "alloy", "format": "wav"}
    }))
    .expect("request");

    let error = openai_chat_completions_to_upstream(
        &reqwest::Client::new(),
        request,
        "http://127.0.0.1:1",
        "local-model-ref",
        ServerCapability::Chat,
    )
    .await
    .expect_err("audio output unsupported");

    assert_eq!(error.status, StatusCode::BAD_REQUEST);
    assert_eq!(error.code, "unsupported_provider_field");
}

#[tokio::test]
async fn claude_messages_maps_local_request_and_response() {
    async fn chat(
        AxumState(captured): AxumState<Arc<Mutex<Option<String>>>>,
        body: String,
    ) -> Json<Value> {
        *captured.lock().expect("lock") = Some(body);
        Json(json!({
            "task_ref": "task-1",
            "status": "completed",
            "text": "hello from local claude"
        }))
    }

    let captured = Arc::new(Mutex::new(None));
    let (base_url, _task) = spawn_test_server(
        Router::new()
            .route(RUNTIME_CHAT_PATH, post(chat))
            .with_state(captured.clone()),
    )
    .await;
    let request: LocalClaudeMessagesRequest = serde_json::from_value(json!({
        "model": "claude-sonnet-4-5",
        "system": [{"type": "text", "text": "Answer briefly."}],
        "max_tokens": 16,
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "hi"}]},
            {"role": "assistant", "content": "hello"},
            {"role": "user", "content": "again"}
        ],
        "temperature": 0.2
    }))
    .expect("request");

    let response = claude_messages_to_upstream(
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
    assert_eq!(value["type"], "message");
    assert_eq!(value["role"], "assistant");
    assert_eq!(value["model"], "local-model-ref");
    assert_eq!(value["content"][0]["type"], "text");
    assert_eq!(value["content"][0]["text"], "hello from local claude");
    assert_eq!(value["stop_reason"], "end_turn");
    assert_eq!(value["stop_sequence"], Value::Null);
    assert_eq!(value["usage"], Value::Null);

    let captured = captured.lock().expect("lock").clone().expect("captured");
    let captured: Value = serde_json::from_str(&captured).expect("native json");
    assert_eq!(captured["messages"][0]["role"], "system");
    assert_eq!(captured["messages"][0]["content"], "Answer briefly.");
    assert_eq!(captured["messages"][1]["role"], "user");
    assert_eq!(captured["messages"][1]["content"], "hi");
    assert_eq!(captured["messages"][2]["role"], "assistant");
    assert_eq!(captured["messages"][2]["content"], "hello");
    assert_eq!(captured["messages"][3]["role"], "user");
    assert_eq!(captured["messages"][3]["content"], "again");
    assert_eq!(captured["max_tokens"], 16);
    assert_eq!(captured["temperature"], 0.2);
    assert!(captured.get("model").is_none());
}

#[tokio::test]
async fn claude_messages_maps_local_stream_response() {
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
    let request: LocalClaudeMessagesRequest = serde_json::from_value(json!({
        "model": "claude-sonnet-4-5",
        "max_tokens": 8,
        "messages": [{"role": "user", "content": "hi"}],
        "stream": true
    }))
    .expect("request");

    let response = claude_messages_to_upstream(
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
    assert!(body.contains("event: message_start"));
    assert!(body.contains("event: content_block_start"));
    assert!(body.contains("event: content_block_delta"));
    assert!(body.contains(r#""type":"text_delta""#));
    assert!(body.contains(r#""text":"one""#));
    assert!(body.contains(r#""text":" two""#));
    assert!(body.contains("event: content_block_stop"));
    assert!(body.contains("event: message_delta"));
    assert!(body.contains(r#""stop_reason":"end_turn""#));
    assert!(body.contains("event: message_stop"));
    assert!(!body.contains("data: [DONE]"));

    let captured = captured.lock().expect("lock").clone().expect("captured");
    let captured: Value = serde_json::from_str(&captured).expect("native json");
    assert_eq!(captured["messages"][0]["role"], "user");
    assert_eq!(captured["messages"][0]["content"], "hi");
    assert_eq!(captured["max_tokens"], 8);
}

#[tokio::test]
async fn claude_messages_rejects_non_chat_local_server() {
    let request: LocalClaudeMessagesRequest = serde_json::from_value(json!({
        "model": "claude-sonnet-4-5",
        "max_tokens": 8,
        "messages": [{"role": "user", "content": "hi"}]
    }))
    .expect("request");

    let error = claude_messages_to_upstream(
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

#[test]
fn claude_messages_rejects_unsupported_local_tools_and_blocks() {
    let tools: LocalClaudeMessagesRequest = serde_json::from_value(json!({
        "model": "claude-sonnet-4-5",
        "max_tokens": 8,
        "messages": [{"role": "user", "content": "hi"}],
        "tools": [{"name": "lookup", "input_schema": {"type": "object"}}]
    }))
    .expect("request");

    let error = tools
        .into_native_chat_request()
        .expect_err("tools unsupported");
    assert_eq!(error.status, StatusCode::BAD_REQUEST);
    assert_eq!(error.code, "unsupported_provider_field");

    for (label, field) in [
        (
            "audio",
            json!({"audio": {"voice": "alloy", "format": "wav"}}),
        ),
        ("modalities", json!({"modalities": ["text", "audio"]})),
        (
            "input-audio",
            json!({"input_audio": {"data": "AA==", "format": "wav"}}),
        ),
    ] {
        let mut body = json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 8,
            "messages": [{"role": "user", "content": "hi"}]
        });
        body.as_object_mut()
            .expect("object")
            .extend(field.as_object().expect("field").clone());
        let request: LocalClaudeMessagesRequest = serde_json::from_value(body).expect(label);

        let error = request
            .into_native_chat_request()
            .expect_err("audio field unsupported");
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "unsupported_provider_field");
    }

    for (label, field) in [
        ("audio", json!({"audio": {"id": "audio_1", "data": "AA=="}})),
        (
            "input-audio",
            json!({"input_audio": {"data": "AA==", "format": "wav"}}),
        ),
    ] {
        let mut message = json!({"role": "user", "content": "hi"});
        message
            .as_object_mut()
            .expect("message")
            .extend(field.as_object().expect("field").clone());
        let request: LocalClaudeMessagesRequest = serde_json::from_value(json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 8,
            "messages": [message]
        }))
        .expect(label);

        let error = request
            .into_native_chat_request()
            .expect_err("message audio field unsupported");
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "unsupported_provider_field");
    }

    for (label, content) in [
        (
            "audio",
            json!([{"type": "audio", "source": {"type": "base64", "media_type": "audio/wav", "data": "AA=="}}]),
        ),
        (
            "input-audio",
            json!([{"type": "input_audio", "input_audio": {"data": "AA==", "format": "wav"}}]),
        ),
        (
            "image",
            json!([{"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AA=="}}]),
        ),
        (
            "tool-use",
            json!([{"type": "tool_use", "id": "toolu_1", "name": "lookup", "input": {}}]),
        ),
        (
            "tool-result",
            json!([{"type": "tool_result", "tool_use_id": "toolu_1", "content": "ok"}]),
        ),
    ] {
        let block: LocalClaudeMessagesRequest = serde_json::from_value(json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 8,
            "messages": [{
                "role": "user",
                "content": content
            }]
        }))
        .expect(label);

        let error = block
            .into_native_chat_request()
            .expect_err("block unsupported");
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "unsupported_provider_content");
    }
}
