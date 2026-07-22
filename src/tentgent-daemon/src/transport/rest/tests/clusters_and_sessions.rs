use super::*;

#[tokio::test]
async fn cluster_apply_inspect_and_remove_roundtrip() {
    let requested_home = unique_home("clusters-roundtrip");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "4".repeat(64);
    write_safetensors_model_fixture_with_capabilities(&home, &model_ref, &["chat", "embedding"]);
    write_model_capability_proof(
        &home,
        &model_ref,
        ModelCapability::Chat,
        ModelCapabilityProofStatus::Verified,
        "safetensors",
        Some(("local-chat-transformers-peft", 1)),
        None,
    );

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/v1/clusters/local-assistant")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(format!(
                    r#"{{
                        "schema_version": 1,
                        "cluster_ref": "local-assistant",
                        "routes": {{
                            "chat": {{
                                "kind": "local-model",
                                "model_ref": "{model_ref}"
                            }},
                            "embedding": {{
                                "kind": "local-model",
                                "model_ref": "{model_ref}"
                            }}
                        }}
                    }}"#
                )))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["cluster"]["cluster_ref"], "local-assistant");
    assert_eq!(body["cluster"]["schema_version"], 1);
    assert_eq!(body["cluster"]["route_update_policy"], "drain");
    let routes = body["cluster"]["routes"].as_array().expect("routes");
    assert_eq!(routes.len(), 2);
    assert_eq!(routes[0]["route"], "chat");
    assert_eq!(routes[0]["kind"], "local-model");
    assert_eq!(routes[0]["model_ref"].as_str(), Some(model_ref.as_str()));
    assert!(body["cluster"]["readiness"].is_null());
    assert!(routes[0]["readiness"].is_null());
    assert_eq!(routes[1]["route"], "embedding");
    assert!(home
        .join("clusters")
        .join("local-assistant")
        .join("cluster.toml")
        .exists());

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/clusters")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let clusters = body["clusters"].as_array().expect("clusters");
    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0]["cluster_ref"], "local-assistant");
    assert_eq!(
        clusters[0]["routes"],
        serde_json::json!(["chat", "embedding"])
    );

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/clusters/local-assistant")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["cluster"]["cluster_ref"], "local-assistant");
    assert_eq!(body["cluster"]["route_update_policy"], "drain");
    assert_eq!(body["cluster"]["ownership"]["status"], "healthy");
    assert_eq!(body["cluster"]["ownership"]["route_claim_count"], 0);
    assert_eq!(body["cluster"]["ownership"]["scope"]["kind"], "cluster");
    assert_eq!(
        body["cluster"]["ownership"]["scope"]["reference"],
        "local-assistant"
    );
    assert_eq!(
        body["cluster"]["ownership"]["claims"],
        serde_json::json!([])
    );
    assert_eq!(
        body["cluster"]["ownership"]["generations"],
        serde_json::json!([])
    );
    assert_eq!(
        body["cluster"]["ownership"]["issues"],
        serde_json::json!([])
    );
    assert_eq!(body["cluster"]["readiness"]["status"], "partial");
    assert_eq!(body["cluster"]["readiness"]["ready_route_count"], 1);
    assert_eq!(body["cluster"]["readiness"]["attention_route_count"], 1);
    assert_eq!(
        body["cluster"]["definition_path"].as_str(),
        Some(
            path_string(
                home.join("clusters")
                    .join("local-assistant")
                    .join("cluster.toml")
            )
            .as_str()
        )
    );
    let routes = body["cluster"]["routes"].as_array().expect("routes");
    assert_eq!(routes[0]["readiness"]["status"], "verified");
    assert_eq!(routes[0]["runtime_profile_readiness"]["source"], "inferred");
    assert_eq!(
        routes[0]["runtime_profile_readiness"]["effective"]["profile_id"],
        "local-chat-transformers-peft"
    );
    assert_eq!(routes[1]["readiness"]["status"], "unknown");
    assert!(routes[1]["next_actions"]
        .as_array()
        .expect("next actions")
        .iter()
        .any(|action| action["code"] == "verify-model-capability"));

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/v1/clusters/local-assistant")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(format!(
                    r#"{{
                        "schema_version": 1,
                        "cluster_ref": "local-assistant",
                        "route_update_policy": "block",
                        "routes": {{
                            "chat": {{"kind": "local-model", "model_ref": "{model_ref}"}},
                            "embedding": {{"kind": "local-model", "model_ref": "{model_ref}"}}
                        }}
                    }}"#
                )))
                .expect("policy-only request"),
        )
        .await
        .expect("policy-only response");
    assert_eq!(response.status(), StatusCode::OK);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/v1/clusters/local-assistant")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(format!(
                    r#"{{
                        "schema_version": 1,
                        "cluster_ref": "local-assistant",
                        "route_update_policy": "drain",
                        "routes": {{
                            "chat": {{"kind": "local-model", "model_ref": "{model_ref}"}}
                        }}
                    }}"#
                )))
                .expect("blocked route update request"),
        )
        .await
        .expect("blocked route update response");
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"], "cluster_in_use");
    assert_eq!(body["blockers"][0]["code"], "cluster-route-update-blocked");
    assert_eq!(body["blockers"][0]["field"], "route_update_policy");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/v1/clusters/local-assistant")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["removed"]["kind"], "cluster");
    assert_eq!(body["removed"]["cluster_ref"], "local-assistant");
    assert!(!home.join("clusters").join("local-assistant").exists());

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/clusters/local-assistant")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json_body(response).await;
    assert_eq!(body["error"], "not_found");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn cluster_apply_rejects_invalid_cluster_ref() {
    let state = rest_state("clusters-invalid-ref");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/v1/clusters/BadRef")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"schema_version":1,"cluster_ref":"secret","routes":{}}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn cluster_apply_rejects_route_missing_model_capability() {
    let requested_home = unique_home("clusters-incompatible-route");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "5".repeat(64);
    write_model_fixture_with_capabilities(&home, &model_ref, &["embedding"]);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/v1/clusters/local-assistant")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(format!(
                    r#"{{
                        "schema_version": 1,
                        "cluster_ref": "local-assistant",
                        "routes": {{
                            "chat": {{
                                "kind": "local-model",
                                "model_ref": "{model_ref}"
                            }}
                        }}
                    }}"#
                )))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
    assert!(body["message"]
        .as_str()
        .expect("message")
        .contains("requires model capability `chat`"));

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn sessions_returns_empty_catalog_for_isolated_home() {
    let requested_home = unique_home("sessions-empty");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/sessions")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["sessions"].as_array().expect("sessions").len(), 0);

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn session_list_inspect_and_messages_read_kernel_store() {
    let requested_home = unique_home("sessions-catalog");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let older_ref = "111111111111000000000000";
    let newer_ref = "222222222222000000000000";
    write_session_fixture(
        &home,
        older_ref,
        "Older",
        "2026-05-01T00:00:00Z",
        "2026-05-01T00:10:00Z",
        0,
        None,
    );
    write_session_fixture(
        &home,
        newer_ref,
        "Newer",
        "2026-05-01T00:00:00Z",
        "2026-05-01T00:20:00Z",
        3,
        Some(&[
            session_message("user", "one"),
            session_message("assistant", "two"),
            session_message("user", "three"),
        ]),
    );

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/sessions")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let sessions = body["sessions"].as_array().expect("sessions");
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0]["session_ref"], newer_ref);
    assert_eq!(sessions[0]["short_ref"], "222222222222");
    assert_eq!(sessions[0]["title"], "Newer");
    assert_eq!(
        sessions[0]["store_path"].as_str(),
        Some(path_string(home.join("sessions").join(newer_ref)).as_str())
    );

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/sessions/222222")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let session = &body["session"];
    assert_eq!(session["session_ref"], newer_ref);
    let expected_messages_path =
        path_string(home.join("sessions").join(newer_ref).join("messages.jsonl"));
    assert_eq!(
        session["messages_path"].as_str(),
        Some(expected_messages_path.as_str())
    );
    assert!(session["warnings"].as_array().expect("warnings").is_empty());

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/sessions/222222/messages?tail=2&ignored=true")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let messages = body["messages"].as_array().expect("messages");
    assert_eq!(body["session"]["session_ref"], newer_ref);
    assert_eq!(body["tail"], 2);
    assert_eq!(body["total_messages"], 3);
    assert_eq!(body["truncated"], true);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["index"], 1);
    assert_eq!(messages[0]["role"], "assistant");
    assert_eq!(messages[0]["metadata"], serde_json::json!({}));

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn session_messages_reject_invalid_tail() {
    let state = rest_state("sessions-invalid-tail");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/sessions/222222222222/messages?tail=0")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn session_inspect_returns_not_found_for_missing_reference() {
    let requested_home = unique_home("sessions-not-found");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/sessions/333333333333")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json_body(response).await;
    assert_eq!(body["error"], "not_found");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn session_inspect_returns_conflict_for_ambiguous_prefix() {
    let requested_home = unique_home("sessions-ambiguous");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    write_session_fixture(
        &home,
        "444444444444000000000000",
        "First",
        "2026-05-01T00:00:00Z",
        "2026-05-01T00:10:00Z",
        0,
        None,
    );
    write_session_fixture(
        &home,
        "444444444444111111111111",
        "Second",
        "2026-05-01T00:00:00Z",
        "2026-05-01T00:20:00Z",
        0,
        None,
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/sessions/444444")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"], "ambiguous_ref");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn session_inspect_rejects_invalid_reference() {
    let state = rest_state("sessions-invalid-ref");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/sessions/not-hex")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn session_write_routes_create_update_append_compact_and_remove() {
    let requested_home = unique_home("sessions-write");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sessions")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"title":"Draft","tags":["smoke"],"messages":[{"role":"user","content":"hi"},{"role":"assistant","content":"hello"}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert_eq!(body["created"], true);
    assert_eq!(body["session"]["title"], "Draft");
    assert_eq!(body["session"]["message_count"], 2);
    let session_ref = body["session"]["session_ref"]
        .as_str()
        .expect("session ref")
        .to_string();

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/sessions/{}", &session_ref[..12]))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"Renamed","tags":["updated"]}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["session"]["title"], "Renamed");
    assert_eq!(body["session"]["tags"], serde_json::json!(["updated"]));

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{}/messages", &session_ref[..12]))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"messages":[{"role":"user","content":"third","metadata":{"topic":"rest"}}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["session"]["message_count"], 3);
    assert_eq!(body["appended"][0]["index"], 2);
    assert_eq!(body["appended"][0]["role"], "user");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{}/compact", &session_ref[..12]))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"keep_recent_messages":3}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["compacted"]["compacted"], false);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/sessions/{}", &session_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["removed"]["kind"], "session");
    assert!(!home.join("sessions").join(&session_ref).exists());

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/sessions/{}", &session_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let _ = fs::remove_dir_all(home);
}
