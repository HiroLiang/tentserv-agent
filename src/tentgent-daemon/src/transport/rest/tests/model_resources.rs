use super::*;

#[tokio::test]
async fn model_capabilities_post_rejects_empty_final_capability_set() {
    let requested_home = unique_home("models-capability-post-empty");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "7".repeat(64);
    write_model_fixture_with_capabilities(&home, &model_ref, &["chat"]);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/models/{}/capabilities", &model_ref[..12]))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"remove":["chat"]}"#))
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
        .contains("must not be empty"));

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_capability_verify_records_and_lists_proofs() {
    let requested_home = unique_home("models-capability-verify");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "5".repeat(64);
    write_model_fixture_with_capabilities(&home, &model_ref, &["vision-chat"]);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/v1/models/{}/capabilities/verify",
                    &model_ref[..12]
                ))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"capability":"vision-chat"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["proof"]["capability"], "vision-chat");
    assert_eq!(body["proof"]["status"], "verified");
    assert_eq!(body["proof"]["source"], "manual-probe");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/models/{}/capabilities/proofs",
                    &model_ref[..12]
                ))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["proofs"].as_array().expect("proofs").len(), 1);
    assert_eq!(body["proofs"][0]["capability"], "vision-chat");
    assert_eq!(body["proofs"][0]["status"], "verified");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!(
                    "/v1/models/{}/capabilities/proofs/vision-chat",
                    &model_ref[..12]
                ))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["proof_clear"]["capability"], "vision-chat");
    assert_eq!(body["proof_clear"]["removed_proof_count"], 1);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/models/{}/capabilities/proofs",
                    &model_ref[..12]
                ))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert!(body["proofs"].as_array().expect("proofs").is_empty());

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_capability_verify_records_failed_proof_for_undeclared_capability() {
    let requested_home = unique_home("models-capability-verify-failed");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "3".repeat(64);
    write_model_fixture_with_capabilities(&home, &model_ref, &["chat"]);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/v1/models/{}/capabilities/verify",
                    &model_ref[..12]
                ))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"capability":"embedding"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["proof"]["capability"], "embedding");
    assert_eq!(body["proof"]["status"], "failed");
    assert!(body["proof"]["error"]
        .as_str()
        .expect("error")
        .contains("does not advertise capability"));

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_capability_proof_clear_rejects_invalid_capability() {
    let requested_home = unique_home("models-capability-clear-invalid");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "7".repeat(64);
    write_model_fixture_with_capabilities(&home, &model_ref, &["chat"]);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!(
                    "/v1/models/{}/capabilities/proofs/not-a-capability",
                    &model_ref[..12]
                ))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_patch_capability_remains_compatibility_alias() {
    let requested_home = unique_home("models-capability-patch");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "6".repeat(64);
    write_model_fixture(&home, &model_ref);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/models/{}", &model_ref[..12]))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"capability":"vision-chat"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(
        body["model"]["model_capabilities"],
        serde_json::json!(["vision-chat"])
    );

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_list_and_inspect_read_kernel_catalog() {
    let requested_home = unique_home("models-catalog");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "a".repeat(64);
    write_model_fixture(&home, &model_ref);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let models = body["models"].as_array().expect("models");
    assert_eq!(models.len(), 1);
    assert_eq!(models[0]["model_ref"].as_str(), Some(model_ref.as_str()));
    assert_eq!(models[0]["short_ref"].as_str(), Some(&model_ref[..12]));
    assert_eq!(models[0]["format"], "mlx");
    assert_eq!(
        models[0]["model_capabilities"],
        serde_json::json!(["chat", "embedding"])
    );
    assert_eq!(models[0]["model_capability_source"], "explicit-user");
    assert!(models[0].get("manifest_path").is_none());
    assert!(models[0].get("variant_source_path").is_none());

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/models/{}", &model_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let model = &body["model"];
    assert_eq!(model["model_ref"].as_str(), Some(model_ref.as_str()));
    let expected_manifest_path = path_string(
        home.join("models/store")
            .join(&model_ref)
            .join("manifest.json"),
    );
    assert_eq!(
        model["manifest_path"].as_str(),
        Some(expected_manifest_path.as_str())
    );
    let expected_variant_source_path = path_string(
        home.join("models/store")
            .join(&model_ref)
            .join("variants/mlx/source"),
    );
    assert_eq!(
        model["variant_source_path"].as_str(),
        Some(expected_variant_source_path.as_str())
    );

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_inspect_returns_not_found_for_missing_reference() {
    let requested_home = unique_home("models-not-found");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "b".repeat(64);
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/models/{}", &model_ref[..12]))
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
async fn model_inspect_returns_conflict_for_ambiguous_prefix() {
    let requested_home = unique_home("models-ambiguous");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let first_ref = format!("{}0", "c".repeat(63));
    let second_ref = format!("{}1", "c".repeat(63));
    write_model_fixture(&home, &first_ref);
    write_model_fixture(&home, &second_ref);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/models/{}", &first_ref[..12]))
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
async fn model_inspect_rejects_invalid_reference() {
    let state = rest_state("models-invalid-ref");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/models/not-hex")
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
async fn model_remove_deletes_kernel_catalog_entry() {
    let requested_home = unique_home("models-remove");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "d".repeat(64);
    write_safetensors_model_fixture_with_capabilities(&home, &model_ref, &["chat", "embedding"]);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/models/{}", &model_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(
        body["model"]["model_ref"].as_str(),
        Some(model_ref.as_str())
    );
    assert!(!home.join("models/store").join(&model_ref).exists());

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["models"].as_array().expect("models").len(), 0);

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn adapters_returns_empty_catalog_for_isolated_home() {
    let requested_home = unique_home("adapters-empty");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/adapters")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["adapters"].as_array().expect("adapters").len(), 0);

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn adapter_list_and_inspect_read_kernel_catalog() {
    let requested_home = unique_home("adapters-catalog");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let adapter_ref = "d".repeat(64);
    let base_model_ref = "e".repeat(64);
    write_adapter_fixture(&home, &adapter_ref, &base_model_ref);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/adapters")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let adapters = body["adapters"].as_array().expect("adapters");
    assert_eq!(adapters.len(), 1);
    assert_eq!(
        adapters[0]["adapter_ref"].as_str(),
        Some(adapter_ref.as_str())
    );
    assert_eq!(adapters[0]["short_ref"].as_str(), Some(&adapter_ref[..12]));
    assert_eq!(adapters[0]["format"], "mlx");
    assert_eq!(adapters[0]["type"], "lora");
    assert_eq!(
        adapters[0]["base_model_ref"].as_str(),
        Some(base_model_ref.as_str())
    );
    assert_eq!(adapters[0]["backend_support"], serde_json::json!(["mlx"]));
    assert!(adapters[0].get("manifest_path").is_none());
    assert!(adapters[0].get("managed_source_path").is_none());

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/adapters/{}", &adapter_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let adapter = &body["adapter"];
    assert_eq!(adapter["adapter_ref"].as_str(), Some(adapter_ref.as_str()));
    let expected_manifest_path = path_string(
        home.join("adapters/store")
            .join(&adapter_ref)
            .join("manifest.json"),
    );
    assert_eq!(
        adapter["manifest_path"].as_str(),
        Some(expected_manifest_path.as_str())
    );
    let expected_managed_source_path = path_string(
        home.join("adapters/store")
            .join(&adapter_ref)
            .join("source"),
    );
    assert_eq!(
        adapter["managed_source_path"].as_str(),
        Some(expected_managed_source_path.as_str())
    );

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn adapter_inspect_returns_not_found_for_missing_reference() {
    let requested_home = unique_home("adapters-not-found");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let adapter_ref = "f".repeat(64);
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/adapters/{}", &adapter_ref[..12]))
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
async fn adapter_inspect_returns_conflict_for_ambiguous_prefix() {
    let requested_home = unique_home("adapters-ambiguous");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let first_ref = format!("{}0", "1".repeat(63));
    let second_ref = format!("{}1", "1".repeat(63));
    let base_model_ref = "2".repeat(64);
    write_adapter_fixture(&home, &first_ref, &base_model_ref);
    write_adapter_fixture(&home, &second_ref, &base_model_ref);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/adapters/{}", &first_ref[..12]))
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
async fn adapter_inspect_rejects_invalid_reference() {
    let state = rest_state("adapters-invalid-ref");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/adapters/not-hex")
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
async fn adapter_remove_deletes_kernel_catalog_entry() {
    let requested_home = unique_home("adapters-remove");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let adapter_ref = "e".repeat(64);
    let base_model_ref = "f".repeat(64);
    write_adapter_fixture(&home, &adapter_ref, &base_model_ref);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/adapters/{}", &adapter_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(
        body["adapter"]["adapter_ref"].as_str(),
        Some(adapter_ref.as_str())
    );
    assert!(!home.join("adapters/store").join(&adapter_ref).exists());

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/adapters")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["adapters"].as_array().expect("adapters").len(), 0);

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn datasets_returns_empty_catalog_for_isolated_home() {
    let requested_home = unique_home("datasets-empty");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
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
async fn train_plans_returns_empty_catalog_for_isolated_home() {
    let requested_home = unique_home("train-plans-empty");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/train/lora/plans")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["plans"].as_array().expect("plans").len(), 0);

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn train_plan_preview_rejects_path_like_refs_before_kernel_lookup() {
    let state = rest_state("train-plan-preview-invalid-ref");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/train/lora/plans/preview")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model_ref":"models/local","dataset_ref":"aaaaaaaaaaaa","backend":"auto"}"#,
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
async fn dataset_list_and_inspect_read_kernel_catalog() {
    let requested_home = unique_home("datasets-catalog");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let dataset_ref = "3".repeat(64);
    write_dataset_fixture(&home, &dataset_ref);

    let response = build_router(state.clone())
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
    let datasets = body["datasets"].as_array().expect("datasets");
    assert_eq!(datasets.len(), 1);
    assert_eq!(
        datasets[0]["dataset_ref"].as_str(),
        Some(dataset_ref.as_str())
    );
    assert_eq!(datasets[0]["short_ref"].as_str(), Some(&dataset_ref[..12]));
    assert_eq!(datasets[0]["format"], "directory");
    assert_eq!(datasets[0]["source_kind"], "local");
    assert_eq!(datasets[0]["tuning_ready"], true);
    assert_eq!(datasets[0]["splits"]["train"], "train.jsonl");
    assert_eq!(datasets[0]["splits"]["validation"], "valid.jsonl");
    assert_eq!(
        datasets[0]["warnings"],
        serde_json::json!(["small training set"])
    );
    assert!(datasets[0].get("manifest_path").is_none());
    assert!(datasets[0].get("managed_source_path").is_none());

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/datasets/{}", &dataset_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let dataset = &body["dataset"];
    assert_eq!(dataset["dataset_ref"].as_str(), Some(dataset_ref.as_str()));
    let expected_manifest_path = path_string(
        home.join("datasets/store")
            .join(&dataset_ref)
            .join("manifest.json"),
    );
    assert_eq!(
        dataset["manifest_path"].as_str(),
        Some(expected_manifest_path.as_str())
    );
    let expected_managed_source_path = path_string(
        home.join("datasets/store")
            .join(&dataset_ref)
            .join("source"),
    );
    assert_eq!(
        dataset["managed_source_path"].as_str(),
        Some(expected_managed_source_path.as_str())
    );

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn dataset_deterministic_routes_validate_template_export_and_diff() {
    let requested_home = unique_home("datasets-deterministic-tools");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let source_dir = home.join("fixtures/source-dataset");
    fs::create_dir_all(&source_dir).expect("source dataset");
    fs::write(source_dir.join("train.jsonl"), sample_dataset_record()).expect("train jsonl");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/datasets/validate")
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
    assert_eq!(body["valid"], true);
    assert_eq!(body["source"]["kind"], "path");
    assert_eq!(body["records"], 1);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/datasets/template")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"task":"chat","language":"en"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["task"], "chat");
    assert!(body["content"]
        .as_str()
        .expect("content")
        .contains("train.jsonl"));

    let first_ref = "9".repeat(64);
    let second_ref = "a".repeat(64);
    write_dataset_fixture(&home, &first_ref);
    write_dataset_fixture(&home, &second_ref);
    let manifest =
        r#"{"files":[{"relative_path":"train.jsonl","size_bytes":111,"sha256":"same"}]}"#;
    fs::write(
        home.join("datasets/store")
            .join(&first_ref)
            .join("manifest.json"),
        manifest,
    )
    .expect("first manifest");
    fs::write(
        home.join("datasets/store")
            .join(&second_ref)
            .join("manifest.json"),
        manifest,
    )
    .expect("second manifest");
    fs::write(
        home.join("datasets/store")
            .join(&first_ref)
            .join("source/train.jsonl"),
        sample_dataset_record(),
    )
    .expect("first source");
    fs::write(
        home.join("datasets/store")
            .join(&second_ref)
            .join("source/train.jsonl"),
        sample_dataset_record(),
    )
    .expect("second source");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/datasets/{}/diff", &first_ref[..12]))
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"right_dataset_ref":"{}"}}"#,
                    &second_ref[..12]
                )))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["left"]["short_ref"], &first_ref[..12]);
    assert_eq!(body["right"]["short_ref"], &second_ref[..12]);
    assert_eq!(body["diff"]["summary"]["unchanged"], 1);

    let export_dir = home.join("exports/dataset");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/datasets/{}/export", &first_ref[..12]))
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"output_path":"{}"}}"#,
                    path_string(&export_dir)
                )))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(
        body["dataset"]["dataset_ref"].as_str(),
        Some(first_ref.as_str())
    );
    assert!(export_dir.join("train.jsonl").is_file());

    let _ = fs::remove_dir_all(home);
}
