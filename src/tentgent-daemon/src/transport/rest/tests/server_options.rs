use super::*;

async fn create_server(state: RestState, body: Value) -> axum::response::Response {
    build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/servers")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn cloud_server_rejects_lifecycle_field_presence_with_inferred_or_explicit_target() {
    let state = rest_state_for_home(unique_home("cloud-option-presence"));
    for explicit_kind in [false, true] {
        for (field, values) in [
            (
                "lazy_load",
                vec![Value::Null, Value::Bool(false), Value::Bool(true)],
            ),
            (
                "runtime_idle_seconds",
                vec![Value::Null, 0.into(), 300.into()],
            ),
            ("idle_seconds", vec![Value::Null, 0.into(), 300.into()]),
            ("model_idle_seconds", vec![Value::Null, 0.into(), 10.into()]),
        ] {
            for value in values {
                let mut body = serde_json::json!({"runtime_ref":"openai:gpt-4.1-mini"});
                if explicit_kind {
                    body["runtime_kind"] = "cloud".into();
                }
                body[field] = value;
                let response = create_server(state.clone(), body).await;
                assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{field}");
                let body = json_body(response).await;
                assert_eq!(body["error"], "unsupported_target");
                assert!(body["message"].as_str().unwrap().contains(field), "{body}");
            }
        }
    }
    let response = create_server(
        state.clone(),
        serde_json::json!({"runtime_ref":"openai:gpt-4.1-mini"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert_eq!(body["server"]["lazy_load"], false);
    assert!(body["server"]["runtime_idle_seconds"].is_null());
    assert_eq!(
        body["server"]["lifecycle_options_applicability"],
        "not_applicable_legacy_ignored"
    );
    let response = create_server(
        state,
        serde_json::json!({"runtime_ref":"openai:gpt-4.1-mini"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await["server"]["server_ref"],
        body["server"]["server_ref"]
    );
}

#[tokio::test]
async fn local_and_cluster_null_options_preserve_defaults_and_alias_behavior() {
    let state = rest_state_for_home(unique_home("local-option-nulls"));
    let home = &state.app().layout().home_dir;
    let model_ref = "a".repeat(64);
    write_safetensors_model_fixture_with_capabilities(home, &model_ref, &["chat"]);
    write_cluster_fixture(home, "option-test", &model_ref);
    for mut body in [
        serde_json::json!({"runtime_ref":model_ref,"allow_unverified":true}),
        serde_json::json!({"runtime_kind":"cluster","cluster_ref":"option-test","allow_unverified":true}),
    ] {
        let response = create_server(state.clone(), body.clone()).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let first = json_body(response).await;
        for field in [
            "lazy_load",
            "runtime_idle_seconds",
            "model_idle_seconds",
            "idle_seconds",
        ] {
            body[field] = Value::Null;
        }
        let response = create_server(state.clone(), body.clone()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let reused = json_body(response).await;
        assert_eq!(
            reused["server"]["server_ref"],
            first["server"]["server_ref"]
        );
        assert_eq!(reused["server"]["effective_runtime_idle_seconds"], 300);
        assert_eq!(reused["server"]["effective_model_idle_seconds"], 0);
        assert!(reused["server"]
            .get("lifecycle_options_applicability")
            .is_none());
        body["idle_seconds"] = 30.into();
        let response = create_server(state.clone(), body.clone()).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let alias = json_body(response).await;
        assert_eq!(alias["server"]["runtime_idle_seconds"], 30);
        body["runtime_idle_seconds"] = 30.into();
        assert_eq!(
            create_server(state.clone(), body.clone()).await.status(),
            StatusCode::OK
        );
        body["runtime_idle_seconds"] = 31.into();
        let response = create_server(state.clone(), body.clone()).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(json_body(response).await["message"]
            .as_str()
            .unwrap()
            .contains("must match"));
    }
}

#[tokio::test]
async fn image_server_eager_create_and_legacy_start_fail_without_spawning() {
    let state = rest_state_for_home(unique_home("image-option-lazy"));
    let home = state.app().layout().home_dir.clone();
    let model_ref = "b".repeat(64);
    write_model_fixture_with_capabilities(&home, &model_ref, &["image-generation"]);
    let mut input = serde_json::json!({"runtime_ref":model_ref,"allow_unverified":true});
    for value in [None, Some(Value::Null), Some(false.into())] {
        if let Some(value) = value {
            input["lazy_load"] = value;
        }
        let response = create_server(state.clone(), input.clone()).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(json_body(response).await["message"]
            .as_str()
            .unwrap()
            .contains("requires --lazy-load"));
    }
    input["lazy_load"] = true.into();
    let response = create_server(state.clone(), input).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    let reference = body["server"]["server_ref"].as_str().unwrap();
    let spec_path = home.join("servers").join(reference).join("server.toml");
    let legacy = fs::read_to_string(&spec_path)
        .unwrap()
        .replace("lazy_load = true", "lazy_load = false");
    fs::write(&spec_path, &legacy).unwrap();
    for allow_unverified in [false, true] {
        let response = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/servers/{reference}/start"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"allow_unverified":allow_unverified}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(json_body(response).await["message"]
            .as_str()
            .unwrap()
            .contains("requires --lazy-load"));
    }
    assert_eq!(fs::read_to_string(&spec_path).unwrap(), legacy);
    assert!(!spec_path.with_file_name("process.toml").exists());
    for method in ["GET", "DELETE"] {
        let response = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(format!("/v1/servers/{reference}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}

#[tokio::test]
async fn cloud_legacy_inspect_preserves_raw_fields_and_cloud_images_need_no_lazy() {
    let state = rest_state_for_home(unique_home("cloud-legacy-inspect"));
    let input =
        serde_json::json!({"runtime_ref":"openai:gpt-image-1","capability":"image-generation"});
    let response = create_server(state.clone(), input.clone()).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    let reference = body["server"]["server_ref"].as_str().unwrap();
    let path = state
        .app()
        .layout()
        .home_dir
        .join("servers")
        .join(reference)
        .join("server.toml");
    let mut legacy = fs::read_to_string(&path)
        .unwrap()
        .replace("lazy_load = false", "lazy_load = true");
    legacy.push_str("\nidle_seconds = 42\n");
    fs::write(&path, &legacy).unwrap();
    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/v1/servers/{reference}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["server"]["server_ref"], reference);
    assert_eq!(body["server"]["lazy_load"], true);
    assert_eq!(body["server"]["idle_seconds"], 42);
    assert_eq!(body["server"]["runtime_idle_seconds"], 42);
    assert_eq!(
        body["server"]["lifecycle_options_applicability"],
        "not_applicable_legacy_ignored"
    );
    assert_eq!(fs::read_to_string(path).unwrap(), legacy);
    let mut input = input;
    input["lazy_load"] = true.into();
    assert_eq!(
        create_server(state, input).await.status(),
        StatusCode::BAD_REQUEST
    );
}
