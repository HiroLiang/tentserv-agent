use super::*;

#[tokio::test]
async fn gemini_generate_content_maps_local_request_and_response() {
    async fn chat(
        AxumState(captured): AxumState<Arc<Mutex<Option<String>>>>,
        body: String,
    ) -> Json<Value> {
        *captured.lock().expect("lock") = Some(body);
        Json(json!({
            "task_ref": "task-1",
            "status": "completed",
            "text": "hello from local gemini"
        }))
    }

    let captured = Arc::new(Mutex::new(None));
    let (base_url, _task) = spawn_test_server(
        Router::new()
            .route(RUNTIME_CHAT_PATH, post(chat))
            .with_state(captured.clone()),
    )
    .await;
    let request: LocalGeminiGenerateContentRequest = serde_json::from_value(json!({
        "systemInstruction": {"parts": [{"text": "Answer briefly."}]},
        "contents": [
            {"role": "user", "parts": [{"text": "hi"}]},
            {"role": "model", "parts": [{"text": "hello"}]},
            {"parts": [{"text": "again"}]}
        ],
        "generationConfig": {
            "maxOutputTokens": 16,
            "temperature": 0.2
        }
    }))
    .expect("request");

    let response = gemini_generate_content_to_upstream(
        &reqwest::Client::new(),
        request,
        "caller-path-model:generateContent",
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
    assert_eq!(value["modelVersion"], "local-model-ref");
    assert_eq!(value["candidates"][0]["content"]["role"], "model");
    assert_eq!(
        value["candidates"][0]["content"]["parts"][0]["text"],
        "hello from local gemini"
    );
    assert_eq!(value["candidates"][0]["finishReason"], "STOP");
    assert_eq!(value["usageMetadata"], Value::Null);

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
async fn gemini_generate_content_maps_local_stream_response() {
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
    let request: LocalGeminiGenerateContentRequest = serde_json::from_value(json!({
        "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
        "generationConfig": {"maxOutputTokens": 8}
    }))
    .expect("request");

    let response = gemini_generate_content_to_upstream(
        &reqwest::Client::new(),
        request,
        "caller-path-model:streamGenerateContent",
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
    assert!(body.contains(r#""modelVersion":"local-model-ref""#));
    assert!(body.contains(r#""text":"one""#));
    assert!(body.contains(r#""text":" two""#));
    assert!(body.contains(r#""finishReason":"STOP""#));
    assert!(!body.contains("event:"));
    assert!(!body.contains("data: [DONE]"));

    let captured = captured.lock().expect("lock").clone().expect("captured");
    let captured: Value = serde_json::from_str(&captured).expect("native json");
    assert_eq!(captured["messages"][0]["role"], "user");
    assert_eq!(captured["messages"][0]["content"], "hi");
    assert_eq!(captured["max_tokens"], 8);
}

#[tokio::test]
async fn gemini_generate_content_rejects_non_chat_local_server() {
    let request: LocalGeminiGenerateContentRequest = serde_json::from_value(json!({
        "contents": [{"parts": [{"text": "hi"}]}]
    }))
    .expect("request");

    let error = gemini_generate_content_to_upstream(
        &reqwest::Client::new(),
        request,
        "caller-path-model:generateContent",
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
fn gemini_generate_content_rejects_unsupported_local_tools_and_parts() {
    let tools: LocalGeminiGenerateContentRequest = serde_json::from_value(json!({
        "contents": [{"parts": [{"text": "hi"}]}],
        "tools": [{"functionDeclarations": [{"name": "lookup"}]}]
    }))
    .expect("request");

    let error = tools
        .into_native_chat_request()
        .expect_err("tools unsupported");
    assert_eq!(error.status, StatusCode::BAD_REQUEST);
    assert_eq!(error.code, "unsupported_provider_field");

    for (label, part) in [
        (
            "inline-data-camel-case",
            json!({"inlineData": {"mimeType": "image/png", "data": "AA=="}}),
        ),
        (
            "inline-data-snake-case",
            json!({"inline_data": {"mime_type": "image/jpeg", "data": "AA=="}}),
        ),
        (
            "audio-inline-data-camel-case",
            json!({"inlineData": {"mimeType": "audio/mp3", "data": "AA=="}}),
        ),
        (
            "audio-inline-data-snake-case",
            json!({"inline_data": {"mime_type": "audio/wav", "data": "AA=="}}),
        ),
        (
            "file-data",
            json!({"fileData": {"mimeType": "image/png", "fileUri": "gs://bucket/image.png"}}),
        ),
    ] {
        let request: LocalGeminiGenerateContentRequest = serde_json::from_value(json!({
            "contents": [{
                "role": "user",
                "parts": [
                    {"text": "Describe this image."},
                    part
                ]
            }]
        }))
        .expect(label);

        let error = request
            .into_native_chat_request()
            .expect_err("inline media unsupported locally");
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "unsupported_provider_content");
    }
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
async fn openai_image_generation_maps_local_request_and_response() {
    async fn images(
        AxumState(captured): AxumState<Arc<Mutex<Option<String>>>>,
        body: String,
    ) -> Json<Value> {
        *captured.lock().expect("lock") = Some(body.clone());
        let body: Value = serde_json::from_str(&body).expect("native json");
        let output_path = body["output_path"].as_str().expect("output path");
        std::fs::create_dir_all(
            std::path::Path::new(output_path)
                .parent()
                .expect("output parent"),
        )
        .expect("create output parent");
        std::fs::write(output_path, b"image-bytes").expect("write image");
        Json(json!({
            "task_ref": "task-1",
            "status": "completed",
            "model_ref": "local-image-ref",
            "output_format": "png",
            "media_type": "image/png",
            "output_path": output_path,
            "total_bytes": 11,
            "width": 512,
            "height": 512,
            "seed": null
        }))
    }

    let captured = Arc::new(Mutex::new(None));
    let (base_url, _task) = spawn_test_server(
        Router::new()
            .route(RUNTIME_IMAGE_GENERATIONS_PATH, post(images))
            .with_state(captured.clone()),
    )
    .await;
    let layout = test_runtime_layout("local-openai-image-generation");
    let request = LocalOpenAiImageGenerationRequest::from_value(json!({
        "model": "gpt-image-1",
        "prompt": "A small red cube",
        "size": "512x512"
    }))
    .expect("request");

    let response = openai_image_generation_to_upstream(
        &reqwest::Client::new(),
        request,
        &base_url,
        "bound-local-image-ref",
        &layout,
        "server/local:image",
        ServerCapability::ImageGeneration,
    )
    .await
    .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), PROXY_BODY_LIMIT_BYTES)
        .await
        .expect("body");
    let value: Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(value["data"][0]["b64_json"], "aW1hZ2UtYnl0ZXM=");
    assert_eq!(value["model"], "local-image-ref");

    let captured = captured.lock().expect("lock").clone().expect("captured");
    let captured: Value = serde_json::from_str(&captured).expect("native json");
    assert_eq!(captured["prompt"], "A small red cube");
    assert_eq!(captured["output_format"], "png");
    assert_eq!(captured["width"], 512);
    assert_eq!(captured["height"], 512);
    assert!(captured["output_path"]
        .as_str()
        .expect("output path")
        .contains("local-openai-images/server-local-image/"));
    assert!(captured.get("model").is_none());
    assert!(captured.get("provider").is_none());
    assert!(captured.get("size").is_none());
}

#[tokio::test]
async fn openai_image_generation_rejects_non_image_generation_local_server() {
    let request = LocalOpenAiImageGenerationRequest::from_value(json!({
        "model": "gpt-image-1",
        "prompt": "A small red cube"
    }))
    .expect("request");
    let layout = test_runtime_layout("local-openai-image-non-image");

    let error = openai_image_generation_to_upstream(
        &reqwest::Client::new(),
        request,
        "http://127.0.0.1:1",
        "chat-model-ref",
        &layout,
        "server-ref",
        ServerCapability::Chat,
    )
    .await
    .expect_err("non-image capability rejected");

    assert_eq!(error.status, StatusCode::BAD_REQUEST);
    assert_eq!(error.code, "unsupported_provider_capability");
}

#[test]
fn openai_image_generation_rejects_unsupported_local_fields() {
    let error = LocalOpenAiImageGenerationRequest::from_value(json!({
        "model": "gpt-image-1",
        "prompt": "A small red cube",
        "response_format": "b64_json"
    }))
    .expect_err("response_format rejected");

    assert_eq!(error.status, StatusCode::BAD_REQUEST);
    assert_eq!(error.code, "unsupported_provider_field");
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
fn local_image_generation_shape_detection_preserves_native_body() {
    assert!(!local_image_generation_request_uses_openai_shape(&json!({
        "prompt": "native text",
        "output_path": "/tmp/out.png"
    })));
    assert!(local_image_generation_request_uses_openai_shape(&json!({
        "model": "gpt-image-1",
        "prompt": "A small red cube"
    })));
    assert!(local_image_generation_request_uses_openai_shape(&json!({
        "prompt": "A small red cube",
        "size": "1024x1024"
    })));
}

#[test]
fn runtime_upstream_path_maps_known_native_routes_to_runtime_routes() {
    assert_eq!(
        runtime_upstream_path_and_query("/v1/chat?stream=false"),
        "/v1/chat?stream=false"
    );
    assert_eq!(
        runtime_upstream_path_and_query("/v1/embeddings"),
        RUNTIME_EMBEDDINGS_PATH
    );
    assert_eq!(
        runtime_upstream_path_and_query("/v1/images/generations"),
        RUNTIME_IMAGE_GENERATIONS_PATH
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
