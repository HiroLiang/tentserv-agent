use super::{
    claude_messages::{
        claude_messages_response_value, claude_text_content, ClaudeMessage, ClaudeMessagesRequest,
    },
    embeddings::{embedding_response, EmbeddingRequest},
    error::CloudServerError,
    gemini_generate::{
        gemini_operation_stream, gemini_request_into_cloud, gemini_response_value,
        GeminiGenerateContentRequest,
    },
    images::ImageRequest,
    openai_chat::{openai_chat_response_value, OpenAiChatRequest, OpenAiMessage},
};
use crate::provider_compat::ensure_provider_capability;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    routing::post,
    Router,
};
use serde_json::{json, Value};
use tentgent_kernel::{
    features::{
        auth::domain::Provider,
        cloud::domain::{CloudChatContentPart, CloudEndpointCapability},
    },
    foundation::error::KernelError,
};
use tower::ServiceExt;

#[test]
fn openai_request_rejects_tools_with_provider_field_code() {
    let request: OpenAiChatRequest = serde_json::from_value(json!({
        "messages": [{"role": "user", "content": "hi"}],
        "tools": [{"type": "function", "function": {"name": "lookup"}}]
    }))
    .expect("request");

    let error = request
        .compat
        .reject_unsupported()
        .expect_err("tools unsupported");

    let (code, _) = error.into_parts();
    assert_eq!(code, "unsupported_provider_field");
}

#[test]
fn openai_request_accepts_current_text_only_chat_shape_for_direct_cloud() {
    let request: OpenAiChatRequest = serde_json::from_value(json!({
        "messages": [
            {"role": "developer", "content": [{"type": "text", "text": "Follow policy."}]},
            {"role": "user", "content": [{"type": "text", "text": "hi"}]}
        ],
        "max_completion_tokens": 12,
        "temperature": 0.2,
        "stream": true,
        "stream_options": {"include_usage": false, "include_obfuscation": false},
        "modalities": ["text"],
        "response_format": {"type": "text"},
        "tool_choice": "none",
        "function_call": "none",
        "parallel_tool_calls": false,
        "n": 1,
        "store": false
    }))
    .expect("request");

    request
        .compat
        .reject_unsupported()
        .expect("text-only shape supported");

    assert_eq!(
        request
            .max_tokens
            .or(request.compat.max_completion_tokens()),
        Some(12)
    );
    assert_eq!(request.messages.len(), 2);
}

#[test]
fn openai_message_accepts_image_url_parts_for_direct_cloud() {
    let message: OpenAiMessage = serde_json::from_value(json!({
        "role": "user",
        "content": [
            {"type": "text", "text": "Describe this image."},
            {"type": "image_url", "image_url": {"url": "https://example.com/cat.png", "detail": "low"}},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,AA==", "detail": "auto"}}
        ]
    }))
    .expect("message");

    let message = message.into_cloud().expect("cloud message");

    assert_eq!(message.role, "user");
    assert_eq!(
        message.content,
        vec![
            CloudChatContentPart::Text("Describe this image.".to_string()),
            CloudChatContentPart::ImageUrl {
                url: "https://example.com/cat.png".to_string()
            },
            CloudChatContentPart::ImageUrl {
                url: "data:image/png;base64,AA==".to_string()
            }
        ]
    );
}

#[test]
fn openai_message_rejects_malformed_image_url_parts_for_direct_cloud() {
    for (label, part) in [
        ("missing-image-url", json!({"type": "image_url"})),
        ("missing-url", json!({"type": "image_url", "image_url": {}})),
        (
            "empty-url",
            json!({"type": "image_url", "image_url": {"url": " "}}),
        ),
        (
            "invalid-detail",
            json!({"type": "image_url", "image_url": {"url": "https://example.com/cat.png", "detail": "full"}}),
        ),
    ] {
        let message: OpenAiMessage = serde_json::from_value(json!({
            "role": "user",
            "content": [part]
        }))
        .expect(label);

        let error = message.into_cloud().expect_err(label);

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "unsupported_provider_content");
    }
}

#[test]
fn openai_message_accepts_audio_input_for_direct_cloud() {
    let message: OpenAiMessage = serde_json::from_value(json!({
        "role": "user",
        "content": [
            {"type": "text", "text": "Transcribe this."},
            {"type": "input_audio", "input_audio": {"data": "AA==", "format": "wav"}},
            {"type": "input_audio", "input_audio": {"data": "AQ==", "format": "mp3"}}
        ]
    }))
    .expect("message");

    let message = message.into_cloud().expect("cloud message");

    assert_eq!(message.role, "user");
    assert_eq!(
        message.content,
        vec![
            CloudChatContentPart::Text("Transcribe this.".to_string()),
            CloudChatContentPart::InputAudio {
                data: "AA==".to_string(),
                format: "wav".to_string()
            },
            CloudChatContentPart::InputAudio {
                data: "AQ==".to_string(),
                format: "mp3".to_string()
            }
        ]
    );
}

#[test]
fn openai_message_rejects_malformed_audio_input_for_direct_cloud() {
    for (label, part) in [
        ("missing-payload", json!({"type": "input_audio"})),
        (
            "missing-data",
            json!({"type": "input_audio", "input_audio": {"format": "wav"}}),
        ),
        (
            "empty-data",
            json!({"type": "input_audio", "input_audio": {"data": " ", "format": "wav"}}),
        ),
        (
            "invalid-format",
            json!({"type": "input_audio", "input_audio": {"data": "AA==", "format": "flac"}}),
        ),
    ] {
        let message: OpenAiMessage =
            serde_json::from_value(json!({"role": "user", "content": [part]})).expect(label);

        let error = message.into_cloud().expect_err(label);

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "unsupported_provider_content");
    }
}

#[test]
fn openai_request_accepts_audio_output_options_for_direct_cloud() {
    let request: OpenAiChatRequest = serde_json::from_value(json!({
        "messages": [{"role": "user", "content": "hi"}],
        "modalities": ["text", "audio"],
        "audio": {"voice": "alloy", "format": "wav"}
    }))
    .expect("request");

    request
        .compat
        .reject_unsupported_for_direct_cloud_openai(false)
        .expect("direct cloud audio output supported");
    assert_eq!(
        request.compat.response_modalities(),
        Some(vec!["text".to_string(), "audio".to_string()])
    );
    assert_eq!(
        request.compat.audio(),
        Some(json!({"voice": "alloy", "format": "wav"}))
    );
}

#[test]
fn openai_request_rejects_unknown_direct_cloud_modalities() {
    let request: OpenAiChatRequest = serde_json::from_value(json!({
        "messages": [{"role": "user", "content": "hi"}],
        "modalities": ["text", "video"]
    }))
    .expect("request");

    let error = request
        .compat
        .reject_unsupported_for_direct_cloud_openai(false)
        .expect_err("unknown modality unsupported");

    let (code, _) = error.into_parts();
    assert_eq!(code, "unsupported_provider_field");
}

#[test]
fn openai_request_rejects_direct_cloud_audio_output_streaming() {
    let request: OpenAiChatRequest = serde_json::from_value(json!({
        "messages": [{"role": "user", "content": "hi"}],
        "stream": true,
        "modalities": ["text", "audio"],
        "audio": {"voice": "alloy", "format": "wav"}
    }))
    .expect("request");

    let error = request
        .compat
        .reject_unsupported_for_direct_cloud_openai(true)
        .expect_err("audio output streaming unsupported");

    let (code, _) = error.into_parts();
    assert_eq!(code, "unsupported_provider_field");
}

#[test]
fn openai_message_rejects_file_parts_for_direct_cloud() {
    let message: OpenAiMessage = serde_json::from_value(json!({
        "role": "user",
        "content": [{"type": "file", "file": {"file_id": "file_123"}}]
    }))
    .expect("message");

    let error = message.into_cloud().expect_err("file unsupported");

    assert_eq!(error.status, StatusCode::BAD_REQUEST);
    assert_eq!(error.code, "unsupported_provider_content");
}

#[test]
fn openai_response_keeps_chat_completion_shape_for_direct_cloud() {
    let body = openai_chat_response_value(
        "gpt-4o-mini",
        "A cat sitting on a chair.".to_string(),
        "stop".to_string(),
        None,
    );

    assert_eq!(body["object"], "chat.completion");
    assert_eq!(body["model"], "gpt-4o-mini");
    assert_eq!(body["choices"][0]["message"]["role"], "assistant");
    assert_eq!(
        body["choices"][0]["message"]["content"],
        "A cat sitting on a chair."
    );
    assert_eq!(body["choices"][0]["finish_reason"], "stop");
    assert!(body["usage"].is_null());
}

#[test]
fn openai_response_preserves_audio_output_for_direct_cloud() {
    let body = openai_chat_response_value(
        "gpt-audio",
        "hello".to_string(),
        "stop".to_string(),
        Some(json!({
            "id": "audio_123",
            "data": "AA==",
            "transcript": "hello"
        })),
    );

    assert_eq!(body["choices"][0]["message"]["content"], "hello");
    assert_eq!(body["choices"][0]["message"]["audio"]["id"], "audio_123");
    assert_eq!(body["choices"][0]["message"]["audio"]["data"], "AA==");
    assert_eq!(
        body["choices"][0]["message"]["audio"]["transcript"],
        "hello"
    );
}

#[test]
fn claude_request_rejects_stream_true_with_provider_field_code() {
    let request: ClaudeMessagesRequest = serde_json::from_value(json!({
        "max_tokens": 16,
        "messages": [{"role": "user", "content": "hi"}],
        "stream": true
    }))
    .expect("request");

    let error = request
        .reject_unsupported()
        .expect_err("stream unsupported");

    let (code, _) = error.into_parts();
    assert_eq!(code, "unsupported_provider_field");
}

#[test]
fn claude_request_accepts_text_blocks_and_system_blocks_for_direct_cloud() {
    let request: ClaudeMessagesRequest = serde_json::from_value(json!({
        "system": [{"type": "text", "text": "Answer briefly."}],
        "max_tokens": 16,
        "messages": [{
            "role": "user",
            "content": [{"type": "text", "text": "hi"}]
        }],
        "temperature": 0.2
    }))
    .expect("request");

    request.reject_unsupported().expect("text shape supported");
    assert_eq!(request.max_tokens, 16);
    assert_eq!(
        claude_text_content(request.system.expect("system")).expect("system text"),
        "Answer briefly."
    );
    let message = request
        .messages
        .into_iter()
        .next()
        .expect("message")
        .into_cloud()
        .expect("cloud message");

    assert_eq!(message.role, "user");
    assert_eq!(
        message.content,
        vec![CloudChatContentPart::Text("hi".to_string())]
    );
}

#[test]
fn claude_message_accepts_base64_image_blocks_for_direct_cloud() {
    for (media_type, data) in [
        ("image/jpeg", "/9j/"),
        ("image/png", "AA=="),
        ("image/gif", "R0lGODlh"),
        ("image/webp", "UklGRg=="),
    ] {
        let message: ClaudeMessage = serde_json::from_value(json!({
            "role": "user",
            "content": [
                {"type": "image", "source": {"type": "base64", "media_type": media_type, "data": data}},
                {"type": "text", "text": "Describe this image."}
            ]
        }))
        .expect(media_type);

        let message = message.into_cloud().expect("cloud message");

        assert_eq!(message.role, "user");
        assert_eq!(
            message.content,
            vec![
                CloudChatContentPart::ImageBase64 {
                    media_type: media_type.to_string(),
                    data: data.to_string()
                },
                CloudChatContentPart::Text("Describe this image.".to_string())
            ]
        );
    }
}

#[test]
fn claude_request_rejects_tool_fields_for_direct_cloud() {
    for (label, field) in [
        (
            "tools",
            json!({"tools": [{"name": "lookup", "input_schema": {"type": "object"}}]}),
        ),
        ("tool_choice", json!({"tool_choice": {"type": "auto"}})),
    ] {
        let mut body = json!({
            "max_tokens": 16,
            "messages": [{"role": "user", "content": "hi"}]
        });
        body.as_object_mut()
            .expect("object")
            .extend(field.as_object().expect("field").clone());
        let request: ClaudeMessagesRequest = serde_json::from_value(body).expect(label);

        let error = request.reject_unsupported().expect_err("tools unsupported");

        let (code, _) = error.into_parts();
        assert_eq!(code, "unsupported_provider_field");
    }
}

#[test]
fn claude_request_rejects_audio_fields_for_direct_cloud() {
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
            "max_tokens": 16,
            "messages": [{"role": "user", "content": "hi"}]
        });
        body.as_object_mut()
            .expect("object")
            .extend(field.as_object().expect("field").clone());
        let request: ClaudeMessagesRequest = serde_json::from_value(body).expect(label);

        let error = request.reject_unsupported().expect_err("audio unsupported");

        let (code, _) = error.into_parts();
        assert_eq!(code, "unsupported_provider_field");
    }
}

#[test]
fn claude_message_rejects_audio_fields_for_direct_cloud() {
    for (label, field) in [
        ("audio", json!({"audio": {"id": "audio_1", "data": "AA=="}})),
        (
            "input-audio",
            json!({"input_audio": {"data": "AA==", "format": "wav"}}),
        ),
    ] {
        let mut body = json!({
            "role": "user",
            "content": "hi"
        });
        body.as_object_mut()
            .expect("object")
            .extend(field.as_object().expect("field").clone());
        let message: ClaudeMessage = serde_json::from_value(body).expect(label);

        let error = message.into_cloud().expect_err("audio unsupported");

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "unsupported_provider_field");
    }
}

#[test]
fn claude_message_rejects_unsupported_content_for_direct_cloud() {
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
            "url-image",
            json!([{"type": "image", "source": {"type": "url", "url": "https://example.com/image.png"}}]),
        ),
        (
            "file-image",
            json!([{"type": "image", "source": {"type": "file", "file_id": "file_123"}}]),
        ),
        ("missing-source", json!([{"type": "image"}])),
        (
            "missing-media-type",
            json!([{"type": "image", "source": {"type": "base64", "data": "AA=="}}]),
        ),
        (
            "empty-media-type",
            json!([{"type": "image", "source": {"type": "base64", "media_type": "", "data": "AA=="}}]),
        ),
        (
            "unsupported-media-type",
            json!([{"type": "image", "source": {"type": "base64", "media_type": "image/bmp", "data": "AA=="}}]),
        ),
        (
            "missing-data",
            json!([{"type": "image", "source": {"type": "base64", "media_type": "image/png"}}]),
        ),
        (
            "empty-data",
            json!([{"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": ""}}]),
        ),
        (
            "malformed-base64",
            json!([{"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "not base64!"}}]),
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
        let message: ClaudeMessage = serde_json::from_value(json!({
            "role": "user",
            "content": content
        }))
        .expect(label);

        let error = message.into_cloud().expect_err(label);

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "unsupported_provider_content");
    }
}

#[test]
fn claude_response_keeps_message_shape_for_direct_cloud() {
    let body = claude_messages_response_value(
        "claude-sonnet-4-5",
        "A small chart is shown.".to_string(),
        "end_turn".to_string(),
    );

    assert_eq!(body["type"], "message");
    assert_eq!(body["role"], "assistant");
    assert_eq!(body["model"], "claude-sonnet-4-5");
    assert_eq!(body["content"][0]["type"], "text");
    assert_eq!(body["content"][0]["text"], "A small chart is shown.");
    assert_eq!(body["stop_reason"], "end_turn");
    assert!(body["stop_sequence"].is_null());
    assert!(body["usage"].is_null());
}

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
fn gemini_request_rejects_malformed_inline_images_for_direct_cloud() {
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
                {"text": "Describe this image."},
                {"inlineData": {"mimeType": "image/png", "data": "AA=="}}
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
            CloudChatContentPart::Text("Describe this image.".to_string()),
            CloudChatContentPart::ImageBase64 {
                media_type: "image/png".to_string(),
                data: "AA==".to_string()
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
