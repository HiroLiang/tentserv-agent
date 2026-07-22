use super::*;

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
