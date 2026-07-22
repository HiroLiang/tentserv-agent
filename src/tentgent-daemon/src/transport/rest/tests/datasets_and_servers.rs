use super::*;

#[tokio::test]
async fn dataset_sync_import_stores_local_dataset() {
    let requested_home = unique_home("dataset-sync-import");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let source_dir = home.join("fixtures/importable-dataset");
    fs::create_dir_all(&source_dir).expect("source dataset");
    fs::write(source_dir.join("train.jsonl"), sample_dataset_record()).expect("train jsonl");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/datasets/import")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"path":"{}"}}"#,
                    path_string(&source_dir)
                )))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["mutation"]["kind"], "import");
    assert_eq!(body["dataset"]["tuning_ready"], true);
    let dataset_ref = body["dataset"]["dataset_ref"]
        .as_str()
        .expect("dataset ref");
    assert!(home
        .join("datasets/store")
        .join(dataset_ref)
        .join("source/train.jsonl")
        .is_file());

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn dataset_inspect_returns_not_found_for_missing_reference() {
    let requested_home = unique_home("datasets-not-found");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let dataset_ref = "4".repeat(64);
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/datasets/{}", &dataset_ref[..12]))
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
async fn dataset_inspect_returns_conflict_for_ambiguous_prefix() {
    let requested_home = unique_home("datasets-ambiguous");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let first_ref = format!("{}0", "5".repeat(63));
    let second_ref = format!("{}1", "5".repeat(63));
    write_dataset_fixture(&home, &first_ref);
    write_dataset_fixture(&home, &second_ref);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/datasets/{}", &first_ref[..12]))
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
async fn dataset_inspect_rejects_invalid_reference() {
    let state = rest_state("datasets-invalid-ref");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/datasets/not-hex")
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
async fn dataset_remove_deletes_kernel_catalog_entry() {
    let requested_home = unique_home("datasets-remove");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let dataset_ref = "8".repeat(64);
    write_dataset_fixture(&home, &dataset_ref);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/datasets/{}", &dataset_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(
        body["dataset"]["dataset_ref"].as_str(),
        Some(dataset_ref.as_str())
    );
    assert!(!home.join("datasets/store").join(&dataset_ref).exists());

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/datasets")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["datasets"].as_array().expect("datasets").len(), 0);

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn servers_returns_empty_catalog_for_isolated_home() {
    let requested_home = unique_home("servers-empty");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/servers")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["servers"].as_array().expect("servers").len(), 0);

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_list_and_inspect_read_kernel_catalog() {
    let requested_home = unique_home("servers-catalog");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let server_ref = "6".repeat(64);
    let model_ref = "7".repeat(64);
    write_server_fixture(&home, &server_ref, &model_ref);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/servers")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let servers = body["servers"].as_array().expect("servers");
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0]["server_ref"].as_str(), Some(server_ref.as_str()));
    assert_eq!(servers[0]["short_ref"].as_str(), Some(&server_ref[..12]));
    assert_eq!(servers[0]["runtime_kind"], "local");
    assert_eq!(servers[0]["capability"], "chat");
    assert_eq!(servers[0]["model_ref"].as_str(), Some(model_ref.as_str()));
    assert_eq!(servers[0]["host"], "127.0.0.1");
    assert_eq!(servers[0]["port"], 8999);
    assert_eq!(servers[0]["lazy_load"], false);
    assert_eq!(servers[0]["idle_seconds"], 60);
    assert_eq!(servers[0]["running"], false);
    assert!(servers[0]["process"].is_null());

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/servers/{}", &server_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let server = &body["server"];
    assert_eq!(server["server_ref"].as_str(), Some(server_ref.as_str()));
    assert_eq!(server["capability"], "chat");
    let expected_server_dir = path_string(home.join("servers").join(&server_ref));
    assert_eq!(
        server["server_dir"].as_str(),
        Some(expected_server_dir.as_str())
    );
    let expected_spec_path =
        path_string(home.join("servers").join(&server_ref).join("server.toml"));
    assert_eq!(
        server["spec_path"].as_str(),
        Some(expected_spec_path.as_str())
    );
    let expected_process_path =
        path_string(home.join("servers").join(&server_ref).join("process.toml"));
    assert_eq!(
        server["process_path"].as_str(),
        Some(expected_process_path.as_str())
    );
    assert_eq!(server["ownership"]["scope"]["kind"], "server");
    assert_eq!(
        server["ownership"]["scope"]["reference"].as_str(),
        Some(server_ref.as_str())
    );
    assert_eq!(server["ownership"]["claims"], serde_json::json!([]));
    assert_eq!(server["ownership"]["generations"], serde_json::json!([]));
    assert_eq!(server["ownership"]["issues"], serde_json::json!([]));
    let serialized_ownership = serde_json::to_string(&server["ownership"]).unwrap();
    for private_field in [
        "pid",
        "process_token",
        "owner_id",
        "runtime_key",
        "generation_id",
        "record",
    ] {
        assert!(!serialized_ownership.contains(&format!("\"{private_field}\":")));
    }

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_create_infers_capability_from_model_metadata() {
    let requested_home = unique_home("servers-create");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "b".repeat(64);
    write_safetensors_model_fixture_with_capabilities(&home, &model_ref, &["chat", "embedding"]);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/servers")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"runtime_ref":"{model_ref}","host":"127.0.0.1","port":8998,"lazy_load":true,"idle_seconds":30,"allow_unverified":true}}"#
                )))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert_eq!(body["created"], true);
    assert_eq!(body["server"]["runtime_kind"], "local");
    assert_eq!(body["server"]["capability"], "embedding");
    assert_eq!(
        body["server"]["model_ref"].as_str(),
        Some(model_ref.as_str())
    );
    assert_eq!(body["server"]["port"], 8998);
    assert_eq!(body["server"]["lazy_load"], true);
    assert_eq!(body["server"]["idle_seconds"], 30);
    let server_ref = body["server"]["server_ref"].as_str().expect("server ref");
    assert!(home
        .join("servers")
        .join(server_ref)
        .join("server.toml")
        .exists());

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_create_accepts_structured_cluster_target_and_blocks_cluster_removal() {
    let requested_home = unique_home("servers-create-cluster");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "9".repeat(64);
    write_safetensors_model_fixture_with_capabilities(&home, &model_ref, &["chat"]);
    write_cluster_fixture(&home, "local-assistant", &model_ref);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/servers")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"runtime_kind":"cluster","cluster_ref":"local-assistant","host":"127.0.0.1","port":8997,"allow_unverified":true}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert_eq!(body["server"]["runtime_kind"], "cluster");
    assert_eq!(body["server"]["cluster_ref"], "local-assistant");
    assert_eq!(body["server"]["capability"], Value::Null);
    assert_eq!(body["server"]["model_ref"], Value::Null);
    assert_eq!(body["server"]["target"]["kind"], "cluster");
    assert_eq!(body["server"]["target"]["cluster_ref"], "local-assistant");
    let server_ref = body["server"]["server_ref"]
        .as_str()
        .expect("server ref")
        .to_string();

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/servers")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let servers = body["servers"].as_array().expect("servers");
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0]["cluster_ref"], "local-assistant");
    assert_eq!(servers[0]["capability"], Value::Null);
    assert_eq!(servers[0]["target"]["kind"], "cluster");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/v1/servers/{}", &server_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["server"]["runtime_kind"], "cluster");
    assert_eq!(body["server"]["cluster_ref"], "local-assistant");
    assert_eq!(body["server"]["target"]["cluster_ref"], "local-assistant");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v1/clusters/local-assistant")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"], "cluster_in_use");
    assert!(body["message"]
        .as_str()
        .is_some_and(|message| message.contains("server-spec")));
    assert_eq!(body["blockers"][0]["kind"], "server-spec");
    assert_eq!(body["blockers"][0]["code"], "cluster-in-use");
    assert_eq!(body["blockers"][0]["resource_ref"], "local-assistant");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_create_rejects_mixed_cluster_and_runtime_ref_targets() {
    let requested_home = unique_home("servers-create-cluster-mixed");
    let state = rest_state_for_home(requested_home);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/servers")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"runtime_kind":"cluster","cluster_ref":"local-assistant","runtime_ref":"abc123"}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
    assert!(body["message"]
        .as_str()
        .is_some_and(|message| message.contains("must not include")));

    let _ = fs::remove_dir_all(state.app().layout().home_dir.clone());
}

#[tokio::test]
async fn server_create_rejects_invalid_cluster_ref_as_bad_request() {
    let requested_home = unique_home("servers-create-cluster-invalid-ref");
    let state = rest_state_for_home(requested_home);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/servers")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"runtime_kind":"cluster","cluster_ref":"../not-safe"}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
    assert!(body["message"]
        .as_str()
        .is_some_and(|message| message.contains("invalid cluster_ref")));

    let _ = fs::remove_dir_all(state.app().layout().home_dir.clone());
}

#[tokio::test]
async fn server_create_rejects_non_chat_model_for_chat_server() {
    let requested_home = unique_home("servers-create-non-chat");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "3".repeat(64);
    write_model_fixture_with_capabilities(&home, &model_ref, &["embedding"]);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/servers")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"runtime_ref":"{model_ref}","capability":"chat","host":"127.0.0.1","port":8998}}"#
                )))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "unsupported_target");
    assert!(body["message"]
        .as_str()
        .expect("message")
        .contains("requires model capability `chat`"));

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_create_prepares_embedding_server_spec() {
    let requested_home = unique_home("servers-create-embedding");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "5".repeat(64);
    write_safetensors_model_fixture_with_capabilities(&home, &model_ref, &["embedding"]);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/servers")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"runtime_ref":"{model_ref}","capability":"embedding","host":"127.0.0.1","port":8998,"allow_unverified":true}}"#
                )))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert_eq!(body["server"]["capability"], "embedding");
    assert_eq!(
        body["server"]["model_ref"].as_str(),
        Some(model_ref.as_str())
    );

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_create_prepares_rerank_server_spec() {
    let requested_home = unique_home("servers-create-rerank");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "6".repeat(64);
    write_safetensors_model_fixture_with_capabilities(&home, &model_ref, &["rerank"]);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/servers")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"runtime_ref":"{model_ref}","capability":"rerank","host":"127.0.0.1","port":8999,"allow_unverified":true}}"#
                )))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert_eq!(body["server"]["capability"], "rerank");
    assert_eq!(
        body["server"]["model_ref"].as_str(),
        Some(model_ref.as_str())
    );

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_remove_deletes_stopped_spec() {
    let requested_home = unique_home("servers-remove");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let server_ref = "c".repeat(64);
    let model_ref = "d".repeat(64);
    write_server_fixture(&home, &server_ref, &model_ref);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/servers/{}", &server_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["removed"]["kind"], "server");
    assert_eq!(
        body["removed"]["server_ref"].as_str(),
        Some(server_ref.as_str())
    );
    assert!(!home.join("servers").join(&server_ref).exists());

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_start_returns_conflict_for_running_server() {
    let requested_home = unique_home("servers-start-running");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let server_ref = "d".repeat(64);
    let model_ref = "e".repeat(64);
    write_model_fixture(&home, &model_ref);
    write_server_fixture(&home, &server_ref, &model_ref);
    write_server_process_fixture(&home, &server_ref, std::process::id());

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/servers/{}/start", &server_ref[..12]))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"allow_unverified":true}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"], "already_running");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_start_rejects_incompatible_stored_chat_spec() {
    let requested_home = unique_home("servers-start-non-chat-model");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let server_ref = "a".repeat(64);
    let model_ref = "4".repeat(64);
    write_model_fixture_with_capabilities(&home, &model_ref, &["embedding"]);
    write_server_fixture_with_capability(&home, &server_ref, &model_ref, Some("chat"));

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/servers/{}/start", &server_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "unsupported_target");
    assert!(body["message"]
        .as_str()
        .expect("message")
        .contains("requires model capability `chat`"));

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_stop_returns_conflict_for_stopped_server() {
    let requested_home = unique_home("servers-stop-stopped");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let server_ref = "e".repeat(64);
    let model_ref = "f".repeat(64);
    write_server_fixture(&home, &server_ref, &model_ref);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/servers/{}/stop", &server_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"], "not_running");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_health_reports_stopped_server_without_probe() {
    let requested_home = unique_home("servers-health");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let server_ref = "1".repeat(64);
    let model_ref = "2".repeat(64);
    write_server_fixture(&home, &server_ref, &model_ref);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/servers/{}/health", &server_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(
        body["server"]["server_ref"].as_str(),
        Some(server_ref.as_str())
    );
    assert_eq!(body["running"], false);
    assert_eq!(body["reachable"], false);
    assert_eq!(body["target_url"], "http://127.0.0.1:8999/healthz");
    assert!(body["target_status"].is_null());

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_logs_return_metadata_and_tail_content() {
    let requested_home = unique_home("servers-logs");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let server_ref = "2".repeat(64);
    let model_ref = "3".repeat(64);
    write_server_fixture(&home, &server_ref, &model_ref);
    let server_dir = home.join("servers").join(&server_ref);
    fs::write(server_dir.join("stdout.log"), "hello").expect("stdout log");
    fs::write(server_dir.join("stderr.log"), "alpha-beta").expect("stderr log");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/v1/servers/{}/logs", &server_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["logs"]["stdout"]["exists"], true);
    assert_eq!(body["logs"]["stderr"]["total_bytes"], 10);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/servers/{}/logs/stderr?tail_bytes=4",
                    &server_ref[..12]
                ))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["log"]["kind"], "stderr");
    assert_eq!(body["log"]["tail_bytes"], 4);
    assert_eq!(body["log"]["truncated"], true);
    assert_eq!(body["log"]["content"], "beta");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_inspect_returns_not_found_for_missing_reference() {
    let requested_home = unique_home("servers-not-found");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let server_ref = "8".repeat(64);
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/servers/{}", &server_ref[..12]))
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
async fn server_inspect_returns_conflict_for_ambiguous_prefix() {
    let requested_home = unique_home("servers-ambiguous");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let first_ref = format!("{}0", "9".repeat(63));
    let second_ref = format!("{}1", "9".repeat(63));
    let model_ref = "a".repeat(64);
    write_server_fixture(&home, &first_ref, &model_ref);
    write_server_fixture(&home, &second_ref, &model_ref);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/servers/{}", &first_ref[..12]))
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
async fn server_inspect_rejects_invalid_reference() {
    let state = rest_state("servers-invalid-ref");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/servers/not-hex")
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
async fn clusters_returns_empty_catalog_for_isolated_home() {
    let requested_home = unique_home("clusters-empty");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let response = build_router(state)
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
    assert_eq!(body["clusters"].as_array().expect("clusters").len(), 0);

    let _ = fs::remove_dir_all(home);
}
