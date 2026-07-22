use super::*;

#[test]
fn gemini_operation_rejects_unsupported_suffix() {
    let error =
        gemini_operation_stream("gemini-2.0-flash:countTokens").expect_err("unsupported operation");

    let (code, _) = error.into_parts();
    assert_eq!(code, "unsupported_provider_operation");
}

#[test]
fn gemini_request_uses_bound_model_and_generation_config_for_direct_cloud() {
    let request: GeminiGenerateContentRequest = serde_json::from_value(json!({
        "systemInstruction": {
            "parts": [{"text": "Answer briefly."}]
        },
        "contents": [{
            "role": "user",
            "parts": [{"text": "hi"}]
        }],
        "generationConfig": {
            "maxOutputTokens": 12,
            "temperature": 0.2
        }
    }))
    .expect("request");

    let cloud_request = gemini_request_into_cloud(
        request,
        "caller-path-model:generateContent",
        Provider::Gemini,
        "bound-gemini-model".to_string(),
    )
    .expect("cloud request");

    assert_eq!(cloud_request.provider, Provider::Gemini);
    assert_eq!(cloud_request.model, "bound-gemini-model");
    assert_eq!(cloud_request.max_tokens, Some(12));
    assert_eq!(cloud_request.temperature, Some(0.2));
    assert!(!cloud_request.stream);
    assert_eq!(cloud_request.messages[0].role, "system");
    assert_eq!(
        cloud_request.messages[0].content,
        vec![CloudChatContentPart::Text("Answer briefly.".to_string())]
    );
    assert_eq!(cloud_request.messages[1].role, "user");
    assert_eq!(
        cloud_request.messages[1].content,
        vec![CloudChatContentPart::Text("hi".to_string())]
    );
}

#[test]
fn gemini_request_maps_inline_images_for_direct_cloud() {
    for (label, part) in [
        (
            "camel-case",
            json!({"inlineData": {"mimeType": "image/png", "data": "AA=="}}),
        ),
        (
            "snake-case",
            json!({"inline_data": {"mime_type": "image/jpeg", "data": "AQ=="}}),
        ),
    ] {
        let request: GeminiGenerateContentRequest = serde_json::from_value(json!({
            "contents": [{
                "role": "user",
                "parts": [
                    {"text": "Describe this image."},
                    part
                ]
            }]
        }))
        .expect(label);

        let cloud_request = gemini_request_into_cloud(
            request,
            "caller-path-model:generateContent",
            Provider::Gemini,
            "bound-gemini-model".to_string(),
        )
        .expect("cloud request");

        assert_eq!(cloud_request.provider, Provider::Gemini);
        assert_eq!(cloud_request.model, "bound-gemini-model");
        assert_eq!(cloud_request.messages[0].role, "user");
        assert_eq!(
            cloud_request.messages[0].content[0],
            CloudChatContentPart::Text("Describe this image.".to_string())
        );
        match &cloud_request.messages[0].content[1] {
            CloudChatContentPart::ImageBase64 { media_type, data } => {
                if label == "camel-case" {
                    assert_eq!(media_type, "image/png");
                    assert_eq!(data, "AA==");
                } else {
                    assert_eq!(media_type, "image/jpeg");
                    assert_eq!(data, "AQ==");
                }
            }
            other => panic!("expected image part, got {other:?}"),
        }
    }
}

#[test]
fn gemini_request_maps_inline_audio_for_direct_cloud() {
    for (label, part) in [
        (
            "camel-case",
            json!({"inlineData": {"mimeType": "audio/mp3", "data": "AA=="}}),
        ),
        (
            "snake-case",
            json!({"inline_data": {"mime_type": "audio/wav", "data": "AQ=="}}),
        ),
    ] {
        let request: GeminiGenerateContentRequest = serde_json::from_value(json!({
            "contents": [{
                "role": "user",
                "parts": [
                    {"text": "Transcribe this audio."},
                    part
                ]
            }]
        }))
        .expect(label);

        let cloud_request = gemini_request_into_cloud(
            request,
            "caller-path-model:generateContent",
            Provider::Gemini,
            "bound-gemini-model".to_string(),
        )
        .expect("cloud request");

        assert_eq!(cloud_request.provider, Provider::Gemini);
        assert_eq!(cloud_request.model, "bound-gemini-model");
        assert_eq!(cloud_request.messages[0].role, "user");
        assert_eq!(
            cloud_request.messages[0].content[0],
            CloudChatContentPart::Text("Transcribe this audio.".to_string())
        );
        match &cloud_request.messages[0].content[1] {
            CloudChatContentPart::AudioBase64 { media_type, data } => {
                if label == "camel-case" {
                    assert_eq!(media_type, "audio/mp3");
                    assert_eq!(data, "AA==");
                } else {
                    assert_eq!(media_type, "audio/wav");
                    assert_eq!(data, "AQ==");
                }
            }
            other => panic!("expected audio part, got {other:?}"),
        }
    }
}

#[test]
fn gemini_request_rejects_malformed_inline_media_for_direct_cloud() {
    for (label, part) in [
        ("missing-inline-data", json!({"inlineData": {}})),
        ("missing-mime-type", json!({"inlineData": {"data": "AA=="}})),
        (
            "empty-mime-type",
            json!({"inlineData": {"mimeType": "", "data": "AA=="}}),
        ),
        (
            "unsupported-mime-type",
            json!({"inlineData": {"mimeType": "application/pdf", "data": "AA=="}}),
        ),
        (
            "missing-data",
            json!({"inlineData": {"mimeType": "image/png"}}),
        ),
        (
            "empty-data",
            json!({"inlineData": {"mimeType": "image/png", "data": ""}}),
        ),
        (
            "malformed-base64",
            json!({"inlineData": {"mimeType": "image/png", "data": "not base64!"}}),
        ),
        (
            "malformed-audio-base64",
            json!({"inlineData": {"mimeType": "audio/mp3", "data": "not base64!"}}),
        ),
        (
            "file-data",
            json!({"fileData": {"mimeType": "image/png", "fileUri": "gs://bucket/image.png"}}),
        ),
    ] {
        let request: GeminiGenerateContentRequest = serde_json::from_value(json!({
            "contents": [{
                "role": "user",
                "parts": [part]
            }]
        }))
        .expect(label);

        let error = gemini_request_into_cloud(
            request,
            "caller-path-model:generateContent",
            Provider::Gemini,
            "bound-gemini-model".to_string(),
        )
        .expect_err(label);

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "unsupported_provider_content");
    }
}

#[test]
fn gemini_request_marks_streaming_operation_for_direct_cloud() {
    let request: GeminiGenerateContentRequest = serde_json::from_value(json!({
        "contents": [{"parts": [{"text": "hi"}]}]
    }))
    .expect("request");

    let cloud_request = gemini_request_into_cloud(
        request,
        "caller-path-model:streamGenerateContent",
        Provider::Gemini,
        "bound-gemini-model".to_string(),
    )
    .expect("cloud request");

    assert!(cloud_request.stream);
    assert_eq!(cloud_request.model, "bound-gemini-model");
}

#[test]
fn gemini_response_value_uses_gemini_candidate_shape_for_direct_cloud() {
    let value = gemini_response_value(
        "gemini-2.5-flash",
        Some("hello".to_string()),
        Some("STOP".to_string()),
    );

    assert_eq!(value["modelVersion"], "gemini-2.5-flash");
    assert_eq!(value["usageMetadata"], Value::Null);
    assert_eq!(value["candidates"][0]["index"], 0);
    assert_eq!(value["candidates"][0]["content"]["role"], "model");
    assert_eq!(
        value["candidates"][0]["content"]["parts"][0]["text"],
        "hello"
    );
    assert_eq!(value["candidates"][0]["finishReason"], "STOP");
}

#[test]
fn gemini_parts_accept_text_and_inline_data_for_direct_cloud() {
    let request: GeminiGenerateContentRequest = serde_json::from_value(json!({
        "contents": [{
            "role": "user",
            "parts": [
                {"text": "Describe these inputs."},
                {"inlineData": {"mimeType": "image/png", "data": "AA=="}},
                {"inlineData": {"mimeType": "audio/mp3", "data": "AQ=="}}
            ]
        }]
    }))
    .expect("request");

    let cloud_request = gemini_request_into_cloud(
        request,
        "gemini-2.0-flash:generateContent",
        Provider::Gemini,
        "bound-gemini-model".to_string(),
    )
    .expect("cloud request");

    assert_eq!(
        cloud_request.messages[0].content,
        vec![
            CloudChatContentPart::Text("Describe these inputs.".to_string()),
            CloudChatContentPart::ImageBase64 {
                media_type: "image/png".to_string(),
                data: "AA==".to_string()
            },
            CloudChatContentPart::AudioBase64 {
                media_type: "audio/mp3".to_string(),
                data: "AQ==".to_string()
            }
        ]
    );
}

#[test]
fn gemini_request_rejects_tool_fields_for_direct_cloud() {
    for (label, field) in [
        (
            "tools",
            json!({"tools": [{"functionDeclarations": [{"name": "lookup"}]}]}),
        ),
        (
            "tool-config",
            json!({"toolConfig": {"functionCallingConfig": {"mode": "AUTO"}}}),
        ),
    ] {
        let mut body = json!({
            "contents": [{"parts": [{"text": "hi"}]}]
        });
        body.as_object_mut()
            .expect("object")
            .extend(field.as_object().expect("field").clone());
        let request: GeminiGenerateContentRequest = serde_json::from_value(body).expect(label);

        let error = gemini_request_into_cloud(
            request,
            "gemini-2.0-flash:generateContent",
            Provider::Gemini,
            "bound-gemini-model".to_string(),
        )
        .expect_err("tools unsupported");

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "unsupported_provider_field");
    }
}

#[test]
fn embedding_request_rejects_dimensions_override() {
    let request: EmbeddingRequest = serde_json::from_value(json!({
        "input": "hello",
        "dimensions": 384
    }))
    .expect("request");

    let error = request
        .reject_unsupported()
        .expect_err("dimensions unsupported");

    let (code, _) = error.into_parts();
    assert_eq!(code, "unsupported_provider_field");
}

#[test]
fn embedding_request_rejects_base64_encoding() {
    let request: EmbeddingRequest = serde_json::from_value(json!({
        "input": "hello",
        "encoding_format": "base64"
    }))
    .expect("request");

    let error = request
        .reject_unsupported()
        .expect_err("base64 unsupported");

    let (code, _) = error.into_parts();
    assert_eq!(code, "unsupported_provider_field");
}

#[test]
fn embedding_request_rejects_empty_input_before_cloud_dispatch() {
    let request: EmbeddingRequest = serde_json::from_value(json!({
        "input": []
    }))
    .expect("request");

    let error = request.validate().expect_err("empty input rejected");

    assert_eq!(error.status, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(error.code, "bad_request");
    assert!(error.message.contains("at least one string"));
}

#[test]
fn image_request_accepts_prompt_and_size() {
    let request: ImageRequest = serde_json::from_value(json!({
        "prompt": "A small red cube",
        "size": "1024x1024"
    }))
    .expect("request");

    request
        .reject_unsupported()
        .expect("image request supported");

    assert_eq!(request.prompt, "A small red cube");
    assert_eq!(request.size.as_deref(), Some("1024x1024"));
}

#[test]
fn image_request_rejects_response_format() {
    let request: ImageRequest = serde_json::from_value(json!({
        "prompt": "A small red cube",
        "response_format": "b64_json"
    }))
    .expect("request");

    let error = request
        .reject_unsupported()
        .expect_err("response_format unsupported");

    let (code, _) = error.into_parts();
    assert_eq!(code, "unsupported_provider_field");
}

#[test]
fn image_request_rejects_n() {
    let request: ImageRequest = serde_json::from_value(json!({
        "prompt": "A small red cube",
        "n": 2
    }))
    .expect("request");

    let error = request.reject_unsupported().expect_err("n unsupported");

    let (code, _) = error.into_parts();
    assert_eq!(code, "unsupported_provider_field");
}

#[test]
fn image_request_ignores_caller_model_and_provider() {
    let request: ImageRequest = serde_json::from_value(json!({
        "model": "gpt-image-1",
        "provider": "openai",
        "prompt": "A small red cube",
        "size": "1024x1024"
    }))
    .expect("request");

    request
        .reject_unsupported()
        .expect("direct cloud server ignores route selector fields");

    assert_eq!(request.prompt, "A small red cube");
    assert_eq!(request.size.as_deref(), Some("1024x1024"));
}

#[test]
fn image_request_accepts_gemini_image_generation_body() {
    let request: ImageRequest = serde_json::from_value(json!({
        "model": "gemini-2.5-flash-image",
        "provider": "gemini",
        "prompt": "A small red cube",
        "size": "1024x1024"
    }))
    .expect("request");

    request
        .reject_unsupported()
        .expect("direct cloud server uses the bound Gemini image model");

    assert_eq!(request.prompt, "A small red cube");
    assert_eq!(request.size.as_deref(), Some("1024x1024"));
}

#[test]
fn openai_embedding_response_uses_openai_list_shape() {
    let response = embedding_response(
        Provider::OpenAI,
        "text-embedding-3-small".to_string(),
        vec![vec![0.1, 0.2], vec![0.3, 0.4]],
    );
    let value = serde_json::to_value(response).expect("json");

    assert_eq!(value["object"], "list");
    assert_eq!(value["model"], "text-embedding-3-small");
    assert_eq!(value["usage"], Value::Null);
    assert_eq!(value["data"][0]["object"], "embedding");
    assert_eq!(value["data"][0]["index"], 0);
    assert_eq!(value["data"][0]["embedding"], json!([0.1f32, 0.2f32]));
    assert_eq!(value["data"][1]["object"], "embedding");
    assert_eq!(value["data"][1]["index"], 1);
    assert_eq!(value["data"][1]["embedding"], json!([0.3f32, 0.4f32]));
}

#[test]
fn gemini_embedding_response_keeps_native_shape() {
    let response = embedding_response(
        Provider::Gemini,
        "gemini-embedding-001".to_string(),
        vec![vec![0.1, 0.2]],
    );
    let value = serde_json::to_value(response).expect("json");

    assert_eq!(value["model_ref"], "gemini-embedding-001");
    assert_eq!(value["data"][0]["index"], 0);
    assert_eq!(value["data"][0]["embedding"], json!([0.1f32, 0.2f32]));
    assert!(value.get("object").is_none());
}

#[test]
fn anthropic_embedding_capability_is_rejected_for_direct_cloud() {
    let error = ensure_provider_capability(Provider::Anthropic, CloudEndpointCapability::Embedding)
        .expect_err("Anthropic embedding unsupported");

    let cloud_error = CloudServerError::from(error);

    assert_eq!(cloud_error.status, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(cloud_error.code, "unsupported_provider_capability");
    assert!(cloud_error.message.contains("Anthropic"));
    assert!(cloud_error.message.contains("embedding"));
}

#[tokio::test]
async fn anthropic_bound_cloud_embeddings_route_rejects_capability_before_upstream() {
    let router = Router::new()
        .route("/v1/embeddings", post(super::embeddings::embeddings))
        .with_state(super::CloudServerState {
            config: super::CloudServerRuntimeConfig {
                server_ref: "test-server".to_string(),
                provider: Provider::Anthropic,
                provider_model: "claude-3-5-sonnet-latest".to_string(),
                host: "127.0.0.1".to_string(),
                port: 0,
                runtime_home: None,
            },
            secret: "sk-ant".to_string(),
        });

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/embeddings")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"input":"hello"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body: Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(body["error"], "unsupported_provider_capability");
    assert!(body["message"]
        .as_str()
        .unwrap_or_default()
        .contains("Anthropic"));
    assert!(body["message"]
        .as_str()
        .unwrap_or_default()
        .contains("embedding"));
}

#[test]
fn unsupported_kernel_target_maps_to_provider_capability_code() {
    let error = CloudServerError::from(KernelError::UnsupportedTarget(
        "Anthropic does not support cloud embedding through Tentgent yet".to_string(),
    ));

    assert_eq!(error.status, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(error.code, "unsupported_provider_capability");
}
