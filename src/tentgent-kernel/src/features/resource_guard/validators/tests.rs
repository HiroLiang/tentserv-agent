use std::{fs, sync::Arc};

use crate::{
    features::{
        cluster::domain::{
            ClusterDefinition, ClusterRef, ClusterRouteKey, ClusterRouteTarget,
            ClusterRouteUpdatePolicy, ClusterStoreLayout,
        },
        model::domain::ModelCapability,
        resource_guard::{
            ResourceBlockerCode, ResourceGuardCode, ResourceGuardDependencies,
            ResourceGuardUseCase, ResourceMutationAuthorization, ResourceOperation,
            StdResourceGuard,
        },
        runtime_ownership::{FileRuntimeOwnershipStore, OwnershipProcessProbe},
        server::domain::CloudProvider,
        train::domain::{LoraTrainRun, LoraTrainRunStatus},
    },
    foundation::{error::KernelResult, layout::RuntimeLayout},
};

#[test]
fn guard_and_blocker_codes_keep_compatible_serialized_values() {
    assert_eq!(
        serde_json::to_string(&ResourceGuardCode::ServerInUse).unwrap(),
        "\"server_in_use\""
    );
    assert_eq!(
        serde_json::to_string(&ResourceBlockerCode::RuntimeResourceOwned).unwrap(),
        "\"runtime-resource-owned\""
    );
}

#[test]
fn resource_operation_codes_cover_the_structured_guard_surface() {
    let model_ref = "a".repeat(64);
    let cluster_ref = ClusterRef::parse("cluster-a").expect("cluster ref");
    let definition = ClusterDefinition {
        schema_version: 1,
        cluster_ref: cluster_ref.clone(),
        route_update_policy: Default::default(),
        routes: Default::default(),
    };
    let operations = vec![
        ResourceOperation::DeleteModel {
            model_ref: model_ref.clone(),
        },
        ResourceOperation::RemoveModelCapability {
            model_ref: model_ref.clone(),
            capability: ModelCapability::Chat,
        },
        ResourceOperation::ReplaceModelCapabilities {
            model_ref: model_ref.clone(),
            removed_capabilities: vec![ModelCapability::Chat],
        },
        ResourceOperation::DeleteAdapter {
            adapter_ref: "b".repeat(64),
            base_model_ref: Some(model_ref.clone()),
            capability: Some(ModelCapability::Chat),
        },
        ResourceOperation::RebindAdapter {
            adapter_ref: "b".repeat(64),
            old_base_model_ref: Some(model_ref.clone()),
            new_base_model_ref: Some("c".repeat(64)),
            capability: Some(ModelCapability::Chat),
        },
        ResourceOperation::DeleteDataset {
            dataset_ref: "d".repeat(64),
        },
        ResourceOperation::DeleteTrainPlan {
            plan_ref: "e".repeat(64),
        },
        ResourceOperation::DeleteCluster {
            cluster_ref: cluster_ref.to_string(),
        },
        ResourceOperation::ReplaceCluster {
            cluster_ref: cluster_ref.to_string(),
            definition,
        },
        ResourceOperation::DeleteServerSpec {
            server_ref: "f".repeat(64),
        },
    ];
    assert_eq!(
        operations
            .iter()
            .map(ResourceOperation::code)
            .collect::<Vec<_>>(),
        vec![
            "delete-model",
            "remove-model-capability",
            "replace-model-capabilities",
            "delete-adapter",
            "rebind-adapter",
            "delete-dataset",
            "delete-train-plan",
            "delete-cluster",
            "replace-cluster",
            "delete-server-spec",
        ]
    );
}

#[test]
fn guard_rejections_are_sorted_and_structured() {
    let root = temp_root("sorted");
    let layout = runtime_layout(&root);
    let model_ref = "a".repeat(64);
    write_server_ref(&layout, "server-z", "z-server", &model_ref);
    write_server_ref(&layout, "server-a", "a-server", &model_ref);

    let result = StdResourceGuard::default()
        .authorize(
            &layout,
            ResourceOperation::DeleteModel {
                model_ref: model_ref.clone(),
            },
        )
        .expect("guard result");
    let ResourceMutationAuthorization::Rejected(rejection) = result else {
        panic!("model deletion must be rejected");
    };
    assert_eq!(rejection.operation, "delete-model");
    assert_eq!(rejection.resource, model_ref);
    assert_eq!(rejection.code, ResourceGuardCode::ModelInUse);
    assert_eq!(
        rejection
            .blockers
            .iter()
            .map(|blocker| blocker.reference.as_str())
            .collect::<Vec<_>>(),
        vec!["a-server", "z-server"]
    );
    assert!(rejection
        .blockers
        .iter()
        .all(|blocker| blocker.code == ResourceBlockerCode::ModelInUse));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn unreadable_reference_state_fails_closed() {
    let root = temp_root("malformed");
    let layout = runtime_layout(&root);
    let server_dir = layout.servers_dir.join("malformed");
    fs::create_dir_all(&server_dir).expect("server dir");
    fs::write(server_dir.join("server.toml"), "not = [valid").expect("malformed spec");
    let error = StdResourceGuard::default()
        .authorize(
            &layout,
            ResourceOperation::DeleteModel {
                model_ref: "a".repeat(64),
            },
        )
        .expect_err("malformed blocker state must fail closed");
    assert!(error
        .to_string()
        .contains("resource guard server probe failed"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn stored_block_policy_requires_a_policy_only_transition() {
    let root = temp_root("cluster-block-policy");
    let layout = runtime_layout(&root);
    let cluster_ref = ClusterRef::parse("cluster-a").expect("cluster ref");
    let current_routes: std::collections::BTreeMap<_, _> = [(
        ClusterRouteKey::Chat,
        ClusterRouteTarget::Provider {
            provider: CloudProvider::OpenAI,
            provider_model: "gpt-current".to_string(),
        },
    )]
    .into_iter()
    .collect();
    let store = ClusterStoreLayout::from_home_dir(layout.home_dir.clone());
    let path = store.cluster_definition_path(cluster_ref.as_str());
    fs::create_dir_all(path.parent().expect("cluster parent")).expect("cluster parent");
    fs::write(
        &path,
        toml::to_string_pretty(&ClusterDefinition {
            schema_version: 1,
            cluster_ref: cluster_ref.clone(),
            route_update_policy: ClusterRouteUpdatePolicy::Block,
            routes: current_routes.clone(),
        })
        .expect("cluster toml"),
    )
    .expect("stored cluster");

    let changed = ClusterDefinition {
        schema_version: 1,
        cluster_ref: cluster_ref.clone(),
        route_update_policy: ClusterRouteUpdatePolicy::Drain,
        routes: [(
            ClusterRouteKey::Chat,
            ClusterRouteTarget::Provider {
                provider: CloudProvider::OpenAI,
                provider_model: "gpt-next".to_string(),
            },
        )]
        .into_iter()
        .collect(),
    };
    let result = StdResourceGuard::default()
        .authorize(
            &layout,
            ResourceOperation::ReplaceCluster {
                cluster_ref: cluster_ref.to_string(),
                definition: changed,
            },
        )
        .expect("guard result");
    let ResourceMutationAuthorization::Rejected(rejection) = result else {
        panic!("route change must be rejected by stored block policy");
    };
    assert_eq!(rejection.blockers.len(), 1);
    assert_eq!(
        rejection.blockers[0].code,
        ResourceBlockerCode::ClusterRouteUpdateBlocked
    );
    assert_eq!(
        rejection.blockers[0].field.as_deref(),
        Some("route_update_policy")
    );

    let policy_only = StdResourceGuard::default()
        .authorize(
            &layout,
            ResourceOperation::ReplaceCluster {
                cluster_ref: cluster_ref.to_string(),
                definition: ClusterDefinition {
                    schema_version: 1,
                    cluster_ref,
                    route_update_policy: ClusterRouteUpdatePolicy::Drain,
                    routes: current_routes,
                },
            },
        )
        .expect("policy-only guard result");
    assert!(matches!(
        policy_only,
        ResourceMutationAuthorization::Permitted(_)
    ));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn every_executable_operation_is_permitted_without_references() {
    let root = temp_root("all-permitted");
    let layout = runtime_layout(&root);
    let model_ref = "a".repeat(64);
    let cluster_ref = ClusterRef::parse("cluster-a").expect("cluster ref");
    let definition = ClusterDefinition {
        schema_version: 1,
        cluster_ref: cluster_ref.clone(),
        route_update_policy: ClusterRouteUpdatePolicy::Drain,
        routes: Default::default(),
    };
    let operations = vec![
        ResourceOperation::DeleteModel {
            model_ref: model_ref.clone(),
        },
        ResourceOperation::RemoveModelCapability {
            model_ref: model_ref.clone(),
            capability: ModelCapability::Chat,
        },
        ResourceOperation::ReplaceModelCapabilities {
            model_ref: model_ref.clone(),
            removed_capabilities: vec![ModelCapability::Chat],
        },
        ResourceOperation::DeleteAdapter {
            adapter_ref: "b".repeat(64),
            base_model_ref: Some(model_ref.clone()),
            capability: Some(ModelCapability::Chat),
        },
        ResourceOperation::RebindAdapter {
            adapter_ref: "b".repeat(64),
            old_base_model_ref: Some(model_ref),
            new_base_model_ref: Some("c".repeat(64)),
            capability: Some(ModelCapability::Chat),
        },
        ResourceOperation::DeleteDataset {
            dataset_ref: "d".repeat(64),
        },
        ResourceOperation::DeleteTrainPlan {
            plan_ref: "e".repeat(64),
        },
        ResourceOperation::DeleteCluster {
            cluster_ref: cluster_ref.to_string(),
        },
        ResourceOperation::ReplaceCluster {
            cluster_ref: cluster_ref.to_string(),
            definition,
        },
        ResourceOperation::DeleteServerSpec {
            server_ref: "f".repeat(64),
        },
    ];

    for operation in operations {
        let code = operation.code();
        let result = StdResourceGuard::default()
            .authorize(&layout, operation)
            .expect("guard result");
        assert!(
            matches!(result, ResourceMutationAuthorization::Permitted(_)),
            "{code} should be permitted without references"
        );
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn model_capability_and_adapter_operations_use_server_spec_blockers() {
    let root = temp_root("server-operation-blockers");
    let layout = runtime_layout(&root);
    let model_ref = "a".repeat(64);
    let adapter_ref = "b".repeat(64);
    write_server_spec(
        &layout,
        "server-a",
        &format!(
            "short_ref = \"server-a\"\nmodel_ref = \"{model_ref}\"\ncapability = \"chat\"\nadapter_ref = \"{adapter_ref}\"\n"
        ),
    );
    let operations = [
        (
            ResourceOperation::DeleteModel {
                model_ref: model_ref.clone(),
            },
            ResourceBlockerCode::ModelInUse,
        ),
        (
            ResourceOperation::RemoveModelCapability {
                model_ref: model_ref.clone(),
                capability: ModelCapability::Chat,
            },
            ResourceBlockerCode::CapabilityInUse,
        ),
        (
            ResourceOperation::ReplaceModelCapabilities {
                model_ref: model_ref.clone(),
                removed_capabilities: vec![ModelCapability::Chat],
            },
            ResourceBlockerCode::CapabilityInUse,
        ),
        (
            ResourceOperation::DeleteAdapter {
                adapter_ref: adapter_ref.clone(),
                base_model_ref: Some(model_ref.clone()),
                capability: Some(ModelCapability::Chat),
            },
            ResourceBlockerCode::AdapterInUse,
        ),
        (
            ResourceOperation::RebindAdapter {
                adapter_ref,
                old_base_model_ref: Some(model_ref),
                new_base_model_ref: Some("c".repeat(64)),
                capability: Some(ModelCapability::Chat),
            },
            ResourceBlockerCode::AdapterRebindInUse,
        ),
    ];

    for (operation, expected) in operations {
        assert_rejected_with(&layout, operation, expected);
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn dataset_and_train_plan_operations_use_train_run_blockers() {
    let root = temp_root("train-run-blockers");
    let layout = runtime_layout(&root);
    let plan_ref = "e".repeat(64);
    let dataset_ref = "d".repeat(64);
    write_train_run(&layout, &plan_ref, &dataset_ref, std::process::id());

    assert_rejected_with(
        &layout,
        ResourceOperation::DeleteDataset {
            dataset_ref: dataset_ref.clone(),
        },
        ResourceBlockerCode::DatasetInUse,
    );
    assert_rejected_with(
        &layout,
        ResourceOperation::DeleteTrainPlan { plan_ref },
        ResourceBlockerCode::TrainRunActive,
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn cluster_and_running_server_deletion_are_blocked() {
    let root = temp_root("cluster-server-blockers");
    let layout = runtime_layout(&root);
    let server_ref = "f".repeat(64);
    write_server_spec(
        &layout,
        "server-a",
        &format!(
            "server_ref = \"{server_ref}\"\nshort_ref = \"server-a\"\ncluster_ref = \"cluster-a\"\n"
        ),
    );
    fs::write(
        layout.servers_dir.join("server-a/process.toml"),
        format!("pid = {}\n", std::process::id()),
    )
    .expect("process record");

    assert_rejected_with(
        &layout,
        ResourceOperation::DeleteCluster {
            cluster_ref: "cluster-a".to_string(),
        },
        ResourceBlockerCode::ClusterInUse,
    );
    assert_rejected_with(
        &layout,
        ResourceOperation::DeleteServerSpec { server_ref },
        ResourceBlockerCode::ServerRunning,
    );
    let _ = fs::remove_dir_all(root);
}

fn assert_rejected_with(
    layout: &RuntimeLayout,
    operation: ResourceOperation,
    expected: ResourceBlockerCode,
) {
    let result = injected_guard(true)
        .authorize(layout, operation)
        .expect("guard result");
    let ResourceMutationAuthorization::Rejected(rejection) = result else {
        panic!("operation must be rejected");
    };
    assert!(
        rejection
            .blockers
            .iter()
            .any(|blocker| blocker.code == expected),
        "expected blocker {expected:?}, got {:?}",
        rejection.blockers
    );
}

struct StaticProcessProbe {
    running: bool,
}

impl OwnershipProcessProbe for StaticProcessProbe {
    fn is_process_running(&self, _pid: u32) -> KernelResult<bool> {
        Ok(self.running)
    }
}

fn injected_guard(running: bool) -> StdResourceGuard {
    StdResourceGuard::new_with_dependencies(ResourceGuardDependencies {
        coordinator: Arc::new(
            crate::features::resource_coordination::infra::FileResourceCoordinator,
        ),
        ownership_store: Arc::new(FileRuntimeOwnershipStore),
        process_probe: Arc::new(StaticProcessProbe { running }),
    })
}

fn write_server_spec(layout: &RuntimeLayout, dir: &str, body: &str) {
    let server_dir = layout.servers_dir.join(dir);
    fs::create_dir_all(&server_dir).expect("server dir");
    fs::write(server_dir.join("server.toml"), body).expect("server spec");
}

fn write_train_run(layout: &RuntimeLayout, plan_ref: &str, dataset_ref: &str, pid: u32) {
    let run_ref = "c".repeat(64);
    let run_dir = layout.train_dir.join("lora/runs").join(&run_ref);
    fs::create_dir_all(&run_dir).expect("run dir");
    let run = LoraTrainRun {
        schema_version: 1,
        run_ref: run_ref.clone(),
        short_ref: run_ref[..12].to_string(),
        status: LoraTrainRunStatus::Running,
        phase: Some("training".to_string()),
        error: None,
        created_at: "2026-07-17T00:00:00Z".to_string(),
        started_at: Some("2026-07-17T00:00:00Z".to_string()),
        ended_at: None,
        plan_ref: plan_ref.to_string(),
        plan_short_ref: plan_ref[..12].to_string(),
        model_ref: "a".repeat(64),
        dataset_ref: dataset_ref.to_string(),
        backend: None,
        recipe_hash: "recipe".to_string(),
        pid: Some(pid),
        exit_code: None,
        exit_signal: None,
        adapter_ref: None,
        adapter_path: None,
        adapter_output_path: None,
        adapter_store_path: None,
        run_dir: run_dir.display().to_string(),
        run_path: run_dir.join("run.toml").display().to_string(),
        metrics_path: run_dir.join("metrics.jsonl").display().to_string(),
        raw_log_path: run_dir.join("raw.log").display().to_string(),
    };
    fs::write(
        run_dir.join("run.toml"),
        toml::to_string_pretty(&run).expect("run toml"),
    )
    .expect("run record");
}

fn write_server_ref(layout: &RuntimeLayout, dir: &str, short_ref: &str, model_ref: &str) {
    write_server_spec(
        layout,
        dir,
        &format!("short_ref = \"{short_ref}\"\nmodel_ref = \"{model_ref}\"\n"),
    );
}

fn temp_root(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "tentgent-resource-guard-{label}-{}",
        crate::features::resource_coordination::new_operation_id()
    ))
}

fn runtime_layout(root: &std::path::Path) -> RuntimeLayout {
    RuntimeLayout {
        home_dir: root.to_path_buf(),
        data_root_dir: root.join("data"),
        models_dir: root.join("models"),
        servers_dir: root.join("servers"),
        adapters_dir: root.join("adapters"),
        datasets_dir: root.join("datasets"),
        sessions_dir: root.join("sessions"),
        train_dir: root.join("train"),
        cache_dir: root.join("cache"),
        runtime_dir: root.join("runtime"),
        logs_dir: root.join("logs"),
        locks_dir: root.join("locks"),
        python_env_dir: root.join("runtime/python"),
        bootstrap_dir: root.join("runtime/bootstrap"),
        bootstrap_uv_dir: root.join("runtime/bootstrap/uv"),
        bootstrap_uv_cache_dir: root.join("runtime/bootstrap/uv-cache"),
        capabilities_path: root.join("runtime/capabilities.toml"),
        auth_metadata_path: root.join("runtime/auth.toml"),
        config_path: root.join("config.toml"),
    }
}
