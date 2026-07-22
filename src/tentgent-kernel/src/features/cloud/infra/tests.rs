use super::*;

fn client() -> ReqwestCloudModelClient {
    ReqwestCloudModelClient::with_client_and_endpoints(
        Client::new(),
        CloudProviderEndpoints {
            openai_base_url: Url::parse("https://openai.test").unwrap(),
            anthropic_base_url: Url::parse("https://anthropic.test").unwrap(),
            gemini_base_url: Url::parse("https://gemini.test").unwrap(),
        },
    )
}

#[test]
fn openai_chat_template_keeps_image_url_parts() {
    let request = CloudChatRequest {
        provider: Provider::OpenAI,
        model: "gpt-test".to_string(),
        messages: vec![CloudChatMessage {
            role: "user".to_string(),
            content: vec![
                CloudChatContentPart::Text("what is here?".to_string()),
                CloudChatContentPart::ImageUrl {
                    url: "data:image/png;base64,AA==".to_string(),
                },
            ],
        }],
        max_tokens: Some(12),
        temperature: Some(0.0),
        stream: false,
        response_modalities: None,
        audio: None,
    };
    let http = client().chat_request(&request, "sk-test", false).unwrap();
    let body = http.body().and_then(|body| body.as_bytes()).unwrap();
    let value: Value = serde_json::from_slice(body).unwrap();

    assert_eq!(value["model"], "gpt-test");
    assert_eq!(value["messages"][0]["content"][0]["type"], "text");
    assert_eq!(value["messages"][0]["content"][1]["type"], "image_url");
}

#[test]
fn openai_chat_template_keeps_audio_input_and_output_options() {
    let request = CloudChatRequest {
        provider: Provider::OpenAI,
        model: "gpt-audio".to_string(),
        messages: vec![CloudChatMessage {
            role: "user".to_string(),
            content: vec![
                CloudChatContentPart::Text("what is in this recording?".to_string()),
                CloudChatContentPart::InputAudio {
                    data: "AA==".to_string(),
                    format: "wav".to_string(),
                },
            ],
        }],
        max_tokens: Some(12),
        temperature: Some(0.0),
        stream: false,
        response_modalities: Some(vec!["text".to_string(), "audio".to_string()]),
        audio: Some(json!({"voice": "alloy", "format": "wav"})),
    };
    let http = client().chat_request(&request, "sk-test", false).unwrap();
    let body = http.body().and_then(|body| body.as_bytes()).unwrap();
    let value: Value = serde_json::from_slice(body).unwrap();

    assert_eq!(value["model"], "gpt-audio");
    assert_eq!(value["modalities"], json!(["text", "audio"]));
    assert_eq!(value["audio"], json!({"voice": "alloy", "format": "wav"}));
    assert_eq!(value["messages"][0]["content"][0]["type"], "text");
    assert_eq!(value["messages"][0]["content"][1]["type"], "input_audio");
    assert_eq!(
        value["messages"][0]["content"][1]["input_audio"],
        json!({"data": "AA==", "format": "wav"})
    );
}

#[test]
fn openai_chat_response_preserves_audio_output() {
    let response = decode_chat_response(
        Provider::OpenAI,
        json!({
            "choices": [{
                "message": {
                    "content": "hello",
                    "audio": {
                        "id": "audio_123",
                        "data": "AA==",
                        "transcript": "hello"
                    }
                },
                "finish_reason": "stop"
            }]
        }),
    )
    .expect("response");

    assert_eq!(response.text, "hello");
    assert_eq!(response.finish_reason, "stop");
    assert_eq!(
        response.audio,
        Some(json!({
            "id": "audio_123",
            "data": "AA==",
            "transcript": "hello"
        }))
    );
}

#[test]
fn anthropic_chat_template_uses_messages_url_and_bound_model() {
    let request = CloudChatRequest {
        provider: Provider::Anthropic,
        model: "claude-test".to_string(),
        messages: vec![
            CloudChatMessage {
                role: "system".to_string(),
                content: vec![CloudChatContentPart::Text("Answer briefly.".to_string())],
            },
            CloudChatMessage {
                role: "user".to_string(),
                content: vec![
                    CloudChatContentPart::ImageBase64 {
                        media_type: "image/png".to_string(),
                        data: "AA==".to_string(),
                    },
                    CloudChatContentPart::Text("Describe this image.".to_string()),
                ],
            },
        ],
        max_tokens: Some(12),
        temperature: Some(0.2),
        stream: false,
        response_modalities: None,
        audio: None,
    };
    let http = client().chat_request(&request, "sk-ant", false).unwrap();
    let body = http.body().and_then(|body| body.as_bytes()).unwrap();
    let value: Value = serde_json::from_slice(body).unwrap();

    assert_eq!(http.url().as_str(), "https://anthropic.test/v1/messages");
    assert_eq!(value["model"], "claude-test");
    assert_eq!(value["system"], "Answer briefly.");
    assert_eq!(value["max_tokens"], 12);
    let temperature = value["temperature"].as_f64().expect("temperature");
    assert!((temperature - 0.2).abs() < 0.00001);
    assert_eq!(value["stream"], false);
    assert_eq!(value["messages"][0]["role"], "user");
    assert_eq!(value["messages"][0]["content"][0]["type"], "image");
    assert_eq!(
        value["messages"][0]["content"][0]["source"]["media_type"],
        "image/png"
    );
    assert_eq!(value["messages"][0]["content"][1]["type"], "text");
}

#[test]
fn gemini_chat_template_uses_generate_content_url_and_inline_data() {
    let request = CloudChatRequest {
        provider: Provider::Gemini,
        model: "gemini-test".to_string(),
        messages: vec![CloudChatMessage {
            role: "user".to_string(),
            content: vec![CloudChatContentPart::ImageBase64 {
                media_type: "image/png".to_string(),
                data: "AA==".to_string(),
            }],
        }],
        max_tokens: Some(12),
        temperature: None,
        stream: false,
        response_modalities: None,
        audio: None,
    };
    let http = client()
        .chat_request(&request, "gemini-key", false)
        .unwrap();
    assert_eq!(
        http.url().as_str(),
        "https://gemini.test/v1beta/models/gemini-test:generateContent?key=gemini-key"
    );
    let body = http.body().and_then(|body| body.as_bytes()).unwrap();
    let value: Value = serde_json::from_slice(body).unwrap();

    assert_eq!(
        value["contents"][0]["parts"][0]["inlineData"]["mimeType"],
        "image/png"
    );
    assert_eq!(value["generationConfig"]["maxOutputTokens"], 12);
}

#[test]
fn gemini_chat_template_keeps_audio_inline_data() {
    let request = CloudChatRequest {
        provider: Provider::Gemini,
        model: "gemini-test".to_string(),
        messages: vec![CloudChatMessage {
            role: "user".to_string(),
            content: vec![
                CloudChatContentPart::Text("Transcribe this audio.".to_string()),
                CloudChatContentPart::AudioBase64 {
                    media_type: "audio/mp3".to_string(),
                    data: "AA==".to_string(),
                },
            ],
        }],
        max_tokens: None,
        temperature: None,
        stream: false,
        response_modalities: None,
        audio: None,
    };
    let http = client()
        .chat_request(&request, "gemini-key", false)
        .unwrap();
    let body = http.body().and_then(|body| body.as_bytes()).unwrap();
    let value: Value = serde_json::from_slice(body).unwrap();

    assert_eq!(
        value["contents"][0]["parts"][0]["text"],
        "Transcribe this audio."
    );
    assert_eq!(
        value["contents"][0]["parts"][1]["inlineData"]["mimeType"],
        "audio/mp3"
    );
    assert_eq!(
        value["contents"][0]["parts"][1]["inlineData"]["data"],
        "AA=="
    );
}

#[test]
fn openai_gpt_image_template_omits_response_format() {
    let http = client()
        .image_generation_request(
            &CloudImageGenerationRequest {
                provider: Provider::OpenAI,
                model: "gpt-image-1".to_string(),
                prompt: "red square".to_string(),
                size: Some("1024x1024".to_string()),
            },
            "sk-test",
        )
        .unwrap();
    let body = http.body().and_then(|body| body.as_bytes()).unwrap();
    let value: Value = serde_json::from_slice(body).unwrap();

    assert_eq!(value["model"], "gpt-image-1");
    assert_eq!(value["size"], "1024x1024");
    assert!(value.get("response_format").is_none());
}

#[test]
fn openai_legacy_image_template_requests_b64_json() {
    let http = client()
        .image_generation_request(
            &CloudImageGenerationRequest {
                provider: Provider::OpenAI,
                model: "dall-e-3".to_string(),
                prompt: "red square".to_string(),
                size: None,
            },
            "sk-test",
        )
        .unwrap();
    let body = http.body().and_then(|body| body.as_bytes()).unwrap();
    let value: Value = serde_json::from_slice(body).unwrap();

    assert_eq!(value["model"], "dall-e-3");
    assert_eq!(value["response_format"], "b64_json");
}

#[test]
fn gemini_image_template_uses_generate_content_for_image_models() {
    let http = client()
        .image_generation_request(
            &CloudImageGenerationRequest {
                provider: Provider::Gemini,
                model: "gemini-2.5-flash-image".to_string(),
                prompt: "red square".to_string(),
                size: Some("1024x1024".to_string()),
            },
            "gemini-key",
        )
        .unwrap();
    assert_eq!(
        http.url().as_str(),
        "https://gemini.test/v1/models/gemini-2.5-flash-image:generateContent?key=gemini-key"
    );
    let body = http.body().and_then(|body| body.as_bytes()).unwrap();
    let value: Value = serde_json::from_slice(body).unwrap();

    assert_eq!(value["contents"][0]["parts"][0]["text"], "red square");
    assert!(value.get("generationConfig").is_none());
}

#[test]
fn gemini_image_template_preserves_imagen_predict_fallback() {
    let http = client()
        .image_generation_request(
            &CloudImageGenerationRequest {
                provider: Provider::Gemini,
                model: "imagen-4.0-generate-001".to_string(),
                prompt: "red square".to_string(),
                size: Some("1K".to_string()),
            },
            "gemini-key",
        )
        .unwrap();
    assert_eq!(
        http.url().as_str(),
        "https://gemini.test/v1beta/models/imagen-4.0-generate-001:predict?key=gemini-key"
    );
    let body = http.body().and_then(|body| body.as_bytes()).unwrap();
    let value: Value = serde_json::from_slice(body).unwrap();

    assert_eq!(value["instances"][0]["prompt"], "red square");
    assert_eq!(value["parameters"]["sampleCount"], 1);
    assert_eq!(value["parameters"]["sampleImageSize"], "1K");
}

#[test]
fn gemini_image_template_rejects_non_image_models() {
    let err = client()
        .image_generation_request(
            &CloudImageGenerationRequest {
                provider: Provider::Gemini,
                model: "gemini-2.5-flash".to_string(),
                prompt: "red square".to_string(),
                size: None,
            },
            "gemini-key",
        )
        .expect_err("non-image Gemini model rejected");

    let KernelError::UnsupportedTarget(message) = err else {
        panic!("expected UnsupportedTarget");
    };
    assert!(message.contains("Gemini image generation requires"));
    assert!(message.contains("gemini-2.5-flash"));
}

#[test]
fn gemini_image_response_decodes_generate_content_inline_data() {
    let response = decode_image_generation_response(
        Provider::Gemini,
        json!({
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "Here is an image."},
                        {"inlineData": {"mimeType": "image/png", "data": "AA=="}}
                    ]
                }
            }]
        }),
    )
    .expect("response");

    assert_eq!(response.b64_json, "AA==");
    assert_eq!(response.media_type, "image/png");
}

#[test]
fn gemini_image_response_decodes_snake_case_generate_content_inline_data() {
    let response = decode_image_generation_response(
        Provider::Gemini,
        json!({
            "candidates": [{
                "content": {
                    "parts": [
                        {"inline_data": {"mime_type": "image/png", "data": "AQ=="}}
                    ]
                }
            }]
        }),
    )
    .expect("response");

    assert_eq!(response.b64_json, "AQ==");
}

#[test]
fn anthropic_embedding_is_unsupported() {
    let err = cloud_embedding_request(
        &Client::new(),
        &CloudProviderEndpoints::default(),
        &CloudEmbeddingRequest {
            provider: Provider::Anthropic,
            model: "claude".to_string(),
            input: vec!["hello".to_string()],
        },
        "sk-ant",
    )
    .expect_err("unsupported");

    let KernelError::UnsupportedTarget(message) = err else {
        panic!("expected UnsupportedTarget");
    };
    assert!(message.contains("Anthropic"));
    assert!(message.contains("embedding"));
}
