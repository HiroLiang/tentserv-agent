use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Barrier, Mutex,
    },
    thread,
    time::Duration,
};

use crate::{
    features::{
        cluster::domain::{ClusterRef, ClusterRouteKey},
        resource_coordination::{
            infra::FileResourceCoordinator, ResourceCoordinator, ResourceKey, ResourceKind,
            ResourceLockMode, ResourceLockRequest,
        },
        runtime::infra::ModelRuntimeCapability,
        runtime_ownership::{
            new_process_token, FileRuntimeOwnershipStore, OwnershipOperationRecord,
            ProcessInstanceIdentity, RouteGenerationClaim, RuntimeExecutionIdentity,
            RuntimeGenerationEndpoint, RuntimeGenerationLaunchTarget, RuntimeGenerationRecord,
            RuntimeGenerationState, RuntimeLaunchPolicyRecord, RuntimeOwnershipLayout,
            RuntimeOwnershipScope, RuntimeOwnershipStatus, RuntimeOwnershipStore,
            RuntimeReconcileRequest,
        },
    },
    foundation::layout::RuntimeLayout,
};

use super::super::usecases::{
    RouteClaimAcquireRequest, RouteClaimTransition, StdRuntimeOwnershipUseCase,
};

#[test]
fn focused_inspection_does_not_create_missing_runtime_home() {
    let root = temp_root("read-only-inspect");
    let layout = runtime_layout(&root);

    let inspection = StdRuntimeOwnershipUseCase::default()
        .inspect_runtime_ownership(&layout)
        .unwrap();

    assert_eq!(inspection.summary.route_claim_count, 0);
    assert!(!root.exists());
}

#[test]
fn route_claim_is_durable_and_reused() {
    let root = temp_root("claim");
    let layout = runtime_layout(&root);
    let target =
        RuntimeExecutionIdentity::model_bound("model-a", ModelRuntimeCapability::Chat, None);
    let claim = RouteGenerationClaim::new(
        "server-a",
        ClusterRef::parse("cluster-a").unwrap(),
        ClusterRouteKey::Chat,
        "definition-a",
        target,
        ProcessInstanceIdentity::current(new_process_token()),
    );
    let usecase = StdRuntimeOwnershipUseCase::default();
    assert!(matches!(
        usecase
            .acquire_route_claim(
                &layout,
                RouteClaimAcquireRequest {
                    claim: claim.clone()
                }
            )
            .unwrap(),
        RouteClaimTransition::Acquired(_)
    ));
    assert!(matches!(
        usecase
            .acquire_route_claim(&layout, RouteClaimAcquireRequest { claim })
            .unwrap(),
        RouteClaimTransition::Existing(_)
    ));
    let inspection = usecase.summarize_runtime_ownership(&layout).unwrap();
    assert_eq!(inspection.summary.route_claim_count, 1);
    assert_eq!(inspection.summary.status, RuntimeOwnershipStatus::Healthy);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn route_claim_release_does_not_deadlock_with_server_stop_lock() {
    let root = temp_root("claim-release-during-stop");
    let layout = runtime_layout(&root);
    let claim = RouteGenerationClaim::new(
        "server-a",
        ClusterRef::parse("cluster-a").unwrap(),
        ClusterRouteKey::Chat,
        "definition-a",
        RuntimeExecutionIdentity::model_bound("model-a", ModelRuntimeCapability::Chat, None),
        ProcessInstanceIdentity::current(new_process_token()),
    );
    let owner_id = claim.owner_id.clone();
    let usecase = StdRuntimeOwnershipUseCase::default();
    assert!(matches!(
        usecase
            .acquire_route_claim(&layout, RouteClaimAcquireRequest { claim })
            .unwrap(),
        RouteClaimTransition::Acquired(_)
    ));
    let stop_permit = FileResourceCoordinator
        .acquire(
            &layout,
            ResourceLockRequest::new(
                "stop-server-test",
                vec![
                    (ResourceKey::maintenance(), ResourceLockMode::Shared),
                    (
                        ResourceKey::new(ResourceKind::Server, "server-a"),
                        ResourceLockMode::Exclusive,
                    ),
                ],
            ),
        )
        .unwrap()
        .expect("stop permit");

    let release = usecase.release_route_claim(&layout, &owner_id).unwrap();
    assert!(release.is_none(), "claim release was busy: {release:?}");
    assert!(FileRuntimeOwnershipStore
        .read_claim(
            &RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir),
            &owner_id,
        )
        .unwrap()
        .is_none());
    drop(stop_permit);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn reconcile_dry_run_then_apply_removes_proven_stale_claim() {
    let root = temp_root("reconcile-stale");
    let layout = runtime_layout(&root);
    let claim = RouteGenerationClaim::new(
        "server-a",
        ClusterRef::parse("cluster-a").unwrap(),
        ClusterRouteKey::Chat,
        "definition-a",
        RuntimeExecutionIdentity::model_bound("model-a", ModelRuntimeCapability::Chat, None),
        ProcessInstanceIdentity {
            pid: u32::MAX,
            token: "stale-token".to_string(),
        },
    );
    let usecase = StdRuntimeOwnershipUseCase::default();
    assert!(matches!(
        usecase
            .acquire_route_claim(&layout, RouteClaimAcquireRequest { claim })
            .unwrap(),
        RouteClaimTransition::Acquired(_)
    ));
    let dry_run = usecase
        .reconcile_runtime_ownership(
            &layout,
            RuntimeReconcileRequest {
                apply: false,
                purge_quarantine: false,
            },
        )
        .unwrap();
    assert_eq!(dry_run.actions.len(), 1);
    assert!(!dry_run.actions[0].applied);
    assert_eq!(dry_run.after.route_claim_count, 1);

    let applied = usecase
        .reconcile_runtime_ownership(
            &layout,
            RuntimeReconcileRequest {
                apply: true,
                purge_quarantine: false,
            },
        )
        .unwrap();
    assert!(applied.actions[0].applied);
    assert_eq!(applied.after.route_claim_count, 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn malformed_record_requires_process_exclusion_before_quarantine_and_later_purge() {
    let root = temp_root("quarantine");
    let layout = runtime_layout(&root);
    let ownership = RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir);
    fs::create_dir_all(&ownership.claims).expect("claims dir");
    let unverifiable = ownership.claims.join("unverifiable.toml");
    fs::write(&unverifiable, "not = [valid").expect("malformed syntax");
    let dead = ownership.claims.join("dead.toml");
    fs::write(&dead, format!("pid = {}\n", u32::MAX)).expect("schema-invalid record");

    let usecase = StdRuntimeOwnershipUseCase::default();
    let applied = usecase
        .reconcile_runtime_ownership(
            &layout,
            RuntimeReconcileRequest {
                apply: true,
                purge_quarantine: false,
            },
        )
        .unwrap();
    assert!(unverifiable.exists());
    assert!(!dead.exists());
    assert_eq!(fs::read_dir(&ownership.quarantine).unwrap().count(), 1);
    assert!(applied
        .actions
        .iter()
        .any(|action| { action.action == "quarantine-malformed-record" && action.applied }));
    assert!(applied
        .actions
        .iter()
        .any(|action| { action.action == "quarantine-malformed-record" && !action.applied }));

    let purged = usecase
        .reconcile_runtime_ownership(
            &layout,
            RuntimeReconcileRequest {
                apply: true,
                purge_quarantine: true,
            },
        )
        .unwrap();
    assert!(purged
        .actions
        .iter()
        .any(|action| action.action == "purge-quarantine" && action.applied));
    assert_eq!(fs::read_dir(&ownership.quarantine).unwrap().count(), 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_generation_admission_reuses_starting_record() {
    let root = temp_root("generation");
    let layout = runtime_layout(&root);
    let identity = RuntimeExecutionIdentity::unbound(ModelRuntimeCapability::LoraTuning, None);
    let policy = RuntimeLaunchPolicyRecord {
        runtime_idle_seconds: 300,
        model_idle_seconds: 0,
        legacy_unbounded_model: false,
    };
    let usecase = StdRuntimeOwnershipUseCase::default();
    assert!(matches!(
        usecase
            .admit_runtime_generation(&layout, identity.clone(), policy.clone())
            .unwrap(),
        super::super::usecases::RuntimeGenerationAdmission::Start(_)
    ));
    assert!(matches!(
        usecase
            .admit_runtime_generation(&layout, identity, policy)
            .unwrap(),
        super::super::usecases::RuntimeGenerationAdmission::Starting(_)
    ));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn concurrent_runtime_callers_share_one_generation_and_first_policy() {
    let root = temp_root("concurrent-generation-callers");
    let layout = runtime_layout(&root);
    let identity =
        RuntimeExecutionIdentity::model_bound("model-a", ModelRuntimeCapability::Chat, None);
    let callers = [
        ("one-shot", 101),
        ("daemon", 202),
        ("direct-server", 303),
        ("cluster", 404),
    ];
    let barrier = Arc::new(Barrier::new(callers.len()));
    let outcomes = Arc::new(Mutex::new(Vec::new()));
    let handles = callers
        .into_iter()
        .map(|(caller, idle)| {
            let barrier = Arc::clone(&barrier);
            let outcomes = Arc::clone(&outcomes);
            let layout = layout.clone();
            let identity = identity.clone();
            thread::spawn(move || {
                barrier.wait();
                let policy = RuntimeLaunchPolicyRecord {
                    runtime_idle_seconds: idle,
                    model_idle_seconds: 0,
                    legacy_unbounded_model: false,
                };
                let admission = StdRuntimeOwnershipUseCase::default()
                    .admit_runtime_generation(&layout, identity, policy)
                    .expect("runtime admission");
                outcomes
                    .lock()
                    .expect("outcomes lock")
                    .push((caller, admission));
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().expect("caller thread");
    }

    let outcomes = outcomes.lock().expect("outcomes lock");
    let starts = outcomes
        .iter()
        .filter_map(|(caller, admission)| match admission {
            super::super::usecases::RuntimeGenerationAdmission::Start(record) => {
                Some((*caller, record.policy.clone()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(starts.len(), 1, "exactly one caller may start a generation");
    assert_eq!(
        outcomes
            .iter()
            .filter(|(_, admission)| matches!(
                admission,
                super::super::usecases::RuntimeGenerationAdmission::Starting(_)
            ))
            .count(),
        callers.len() - 1
    );

    let inspection = StdRuntimeOwnershipUseCase::default()
        .summarize_runtime_ownership(&layout)
        .expect("ownership inspection");
    assert_eq!(inspection.generations.len(), 1);
    assert_eq!(inspection.generations[0].policy, starts[0].1);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_generation_transitions_preserve_launch_recovery_evidence() {
    let root = temp_root("generation-transitions");
    let layout = runtime_layout(&root);
    let identity =
        RuntimeExecutionIdentity::model_bound("model-a", ModelRuntimeCapability::Chat, None);
    let policy = runtime_policy();
    let usecase = StdRuntimeOwnershipUseCase::default();
    let record = match usecase
        .admit_runtime_generation(&layout, identity.clone(), policy)
        .unwrap()
    {
        super::super::usecases::RuntimeGenerationAdmission::Start(record) => record,
        other => panic!("unexpected admission: {other:?}"),
    };
    let target = RuntimeGenerationLaunchTarget {
        host: "127.0.0.1".to_string(),
        port: 9876,
    };
    usecase
        .prepare_runtime_launch(&layout, &identity, &record.generation_id, target.clone())
        .unwrap();
    let endpoint = RuntimeGenerationEndpoint {
        host: target.host.clone(),
        port: target.port,
        pid: std::process::id(),
        process_token: record.process_token.clone(),
    };
    usecase
        .mark_runtime_spawned(&layout, &identity, &record.generation_id, endpoint.clone())
        .unwrap();
    usecase
        .mark_runtime_ready(&layout, &identity, &record.generation_id, endpoint)
        .unwrap();

    let inspection = usecase.summarize_runtime_ownership(&layout).unwrap();
    let stored = &inspection.generations[0];
    assert_eq!(stored.state, RuntimeGenerationState::Ready);
    assert_eq!(stored.launch_target, Some(target));
    assert!(stored.endpoint.is_some());
    assert_eq!(stored.diagnostic, None);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn legacy_runtime_generation_without_recovery_fields_deserializes() {
    let record = RuntimeGenerationRecord::starting(
        RuntimeExecutionIdentity::unbound(ModelRuntimeCapability::LoraTuning, None),
        runtime_policy(),
    );
    let mut value = toml::Value::try_from(&record).expect("serialize generation");
    let table = value.as_table_mut().expect("generation table");
    table.remove("launch_target");
    table.remove("diagnostic");

    let decoded: RuntimeGenerationRecord = value.try_into().expect("read legacy generation");
    assert_eq!(decoded.launch_target, None);
    assert_eq!(decoded.diagnostic, None);
}

#[test]
fn reconcile_adopts_matching_worker_from_prepared_launch_target() {
    let root = temp_root("generation-adoption");
    let layout = runtime_layout(&root);
    let identity =
        RuntimeExecutionIdentity::model_bound("model-a", ModelRuntimeCapability::Chat, None);
    let usecase = StdRuntimeOwnershipUseCase::default();
    let record = match usecase
        .admit_runtime_generation(&layout, identity.clone(), runtime_policy())
        .unwrap()
    {
        super::super::usecases::RuntimeGenerationAdmission::Start(record) => record,
        other => panic!("unexpected admission: {other:?}"),
    };
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind health listener");
    listener
        .set_nonblocking(true)
        .expect("nonblocking listener");
    let port = listener.local_addr().unwrap().port();
    usecase
        .prepare_runtime_launch(
            &layout,
            &identity,
            &record.generation_id,
            RuntimeGenerationLaunchTarget {
                host: "127.0.0.1".to_string(),
                port,
            },
        )
        .unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_server = Arc::clone(&stop);
    let token = record.process_token.clone();
    let server = thread::spawn(move || {
        while !stop_server.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut request = [0_u8; 1024];
                    let _ = stream.read(&mut request);
                    let body = format!(
                        "{{\"status\":\"ready\",\"pid\":{},\"process_token\":\"{}\",\"runtime\":{{\"capability\":\"chat\",\"model_ref\":\"model-a\"}}}}",
                        std::process::id(), token
                    );
                    let _ = stream.write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .as_bytes(),
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });

    let reconciled = usecase
        .reconcile_runtime_ownership(
            &layout,
            RuntimeReconcileRequest {
                apply: true,
                purge_quarantine: false,
            },
        )
        .unwrap();
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();

    assert!(reconciled
        .actions
        .iter()
        .any(|action| { action.action == "adopt-matching-generation" && action.applied }));
    let inspection = usecase.summarize_runtime_ownership(&layout).unwrap();
    assert_eq!(
        inspection.generations[0].state,
        RuntimeGenerationState::Ready
    );
    assert_eq!(
        inspection.generations[0].endpoint.as_ref().unwrap().port,
        port
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn reconcile_rejects_worker_when_prepared_process_token_does_not_match() {
    let root = temp_root("generation-adoption-token-mismatch");
    let layout = runtime_layout(&root);
    let identity =
        RuntimeExecutionIdentity::model_bound("model-a", ModelRuntimeCapability::Chat, None);
    let usecase = StdRuntimeOwnershipUseCase::default();
    let record = match usecase
        .admit_runtime_generation(&layout, identity.clone(), runtime_policy())
        .unwrap()
    {
        super::super::usecases::RuntimeGenerationAdmission::Start(record) => record,
        other => panic!("unexpected admission: {other:?}"),
    };
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind health listener");
    listener
        .set_nonblocking(true)
        .expect("nonblocking listener");
    let port = listener.local_addr().unwrap().port();
    usecase
        .prepare_runtime_launch(
            &layout,
            &identity,
            &record.generation_id,
            RuntimeGenerationLaunchTarget {
                host: "127.0.0.1".to_string(),
                port,
            },
        )
        .unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_server = Arc::clone(&stop);
    let server = thread::spawn(move || {
        while !stop_server.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut request = [0_u8; 1024];
                    let _ = stream.read(&mut request);
                    let body = format!(
                        "{{\"status\":\"ready\",\"pid\":{},\"process_token\":\"wrong-token\",\"runtime\":{{\"capability\":\"chat\",\"model_ref\":\"model-a\"}}}}",
                        std::process::id()
                    );
                    let _ = stream.write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .as_bytes(),
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });

    let reconciled = usecase
        .reconcile_runtime_ownership(
            &layout,
            RuntimeReconcileRequest {
                apply: true,
                purge_quarantine: false,
            },
        )
        .unwrap();
    assert!(!reconciled
        .actions
        .iter()
        .any(|action| action.action == "adopt-matching-generation"));
    let inspection = usecase.inspect_runtime_ownership(&layout).unwrap();
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();

    assert_eq!(
        inspection.generations[0].state,
        RuntimeGenerationState::Starting
    );
    assert!(
        inspection.issues.iter().any(|issue| {
            !issue.recoverable
                && issue.kind == "runtime-generation"
                && issue
                    .description
                    .contains("process token does not match ownership record")
        }),
        "unexpected ownership issues: {:?}",
        inspection.issues
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn scoped_ownership_filters_cluster_server_and_unrelated_operations() {
    let root = temp_root("scoped-inspection");
    let layout = runtime_layout(&root);
    let usecase = StdRuntimeOwnershipUseCase::default();
    let identity_a =
        RuntimeExecutionIdentity::model_bound("model-a", ModelRuntimeCapability::Chat, None);
    let identity_b =
        RuntimeExecutionIdentity::model_bound("model-b", ModelRuntimeCapability::Chat, None);
    for (server_ref, cluster_ref, identity) in [
        ("server-a", "cluster-a", identity_a.clone()),
        ("server-b", "cluster-b", identity_b.clone()),
    ] {
        usecase
            .acquire_route_claim(
                &layout,
                RouteClaimAcquireRequest {
                    claim: RouteGenerationClaim::new(
                        server_ref,
                        ClusterRef::parse(cluster_ref).unwrap(),
                        ClusterRouteKey::Chat,
                        format!("definition-{cluster_ref}"),
                        identity.clone(),
                        ProcessInstanceIdentity::current(new_process_token()),
                    ),
                },
            )
            .unwrap();
        usecase
            .admit_runtime_generation(&layout, identity, runtime_policy())
            .unwrap();
    }
    let ownership = RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir);
    FileRuntimeOwnershipStore
        .write_operation(
            &ownership,
            &OwnershipOperationRecord {
                schema_version: super::super::RUNTIME_OWNERSHIP_SCHEMA_VERSION,
                operation_id: "cluster-a-operation".to_string(),
                operation: "test".to_string(),
                pid: std::process::id(),
                resource_keys: vec![crate::features::resource_coordination::ResourceKey::new(
                    crate::features::resource_coordination::ResourceKind::Cluster,
                    "cluster-a",
                )],
                started_at: super::super::now_text(),
            },
        )
        .unwrap();
    FileRuntimeOwnershipStore
        .write_operation(
            &ownership,
            &OwnershipOperationRecord {
                schema_version: super::super::RUNTIME_OWNERSHIP_SCHEMA_VERSION,
                operation_id: "cluster-b-operation".to_string(),
                operation: "test".to_string(),
                pid: std::process::id(),
                resource_keys: vec![crate::features::resource_coordination::ResourceKey::new(
                    crate::features::resource_coordination::ResourceKind::Cluster,
                    "cluster-b",
                )],
                started_at: super::super::now_text(),
            },
        )
        .unwrap();

    let cluster = usecase
        .inspect_runtime_ownership_scope(
            &layout,
            RuntimeOwnershipScope::Cluster {
                cluster_ref: ClusterRef::parse("cluster-a").unwrap(),
            },
        )
        .unwrap();
    assert_eq!(cluster.claims.len(), 1);
    assert_eq!(cluster.generations.len(), 1);
    assert_eq!(cluster.summary.active_operation_count, 1);
    assert_eq!(cluster.claims[0].server_ref, "server-a");

    let direct_server = usecase
        .inspect_runtime_ownership_scope(
            &layout,
            RuntimeOwnershipScope::Server {
                server_ref: "direct-server".to_string(),
                runtime_identities: vec![identity_a],
            },
        )
        .unwrap();
    assert!(direct_server.claims.is_empty());
    assert_eq!(direct_server.generations.len(), 1);

    let cloud_server = usecase
        .inspect_runtime_ownership_scope(
            &layout,
            RuntimeOwnershipScope::Server {
                server_ref: "cloud-server".to_string(),
                runtime_identities: Vec::new(),
            },
        )
        .unwrap();
    assert!(cloud_server.claims.is_empty());
    assert!(cloud_server.generations.is_empty());
    assert_eq!(cloud_server.summary.active_operation_count, 0);

    let serialized = serde_json::to_string(&cluster).unwrap();
    for private_field in [
        "pid",
        "process_token",
        "owner_id",
        "runtime_key",
        "generation_id",
        "record",
    ] {
        assert!(
            !serialized.contains(&format!("\"{private_field}\":")),
            "leaked {private_field}"
        );
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn legacy_ownership_operation_without_resource_keys_deserializes() {
    let operation: OwnershipOperationRecord = toml::from_str(
        "schema_version = 1\noperation_id = 'legacy'\noperation = 'test'\npid = 1\nstarted_at = 'unknown'\n",
    )
    .unwrap();
    assert!(operation.resource_keys.is_empty());
}

#[test]
fn runtime_policy_accepts_legacy_names_and_text_but_rejects_negative_values() {
    let legacy: RuntimeLaunchPolicyRecord =
        toml::from_str("idle_keep_alive_seconds = '300'\nmodel_idle_timeout_seconds = '0'\n")
            .expect("legacy runtime policy");
    assert_eq!(legacy, runtime_policy());

    let body = toml::to_string(&legacy).expect("canonical runtime policy");
    assert!(body.contains("runtime_idle_seconds = 300"));
    assert!(body.contains("model_idle_seconds = 0"));
    assert!(!body.contains("idle_keep_alive_seconds"));
    assert!(!body.contains("model_idle_timeout_seconds"));

    let migrated = toml::from_str::<RuntimeLaunchPolicyRecord>(
        "idle_keep_alive_seconds = '300'\nmodel_idle_timeout_seconds = '-1'\n",
    )
    .expect("former legacy sentinel must remain recoverable");
    assert_eq!(migrated.runtime_idle_seconds, 300);
    assert_eq!(migrated.model_idle_seconds, 0);
    assert!(migrated.legacy_unbounded_model);
    let migrated_body = toml::to_string(&migrated).expect("migrated runtime policy");
    assert!(migrated_body.contains("model_idle_seconds = 0"));
    assert!(migrated_body.contains("legacy_unbounded_model = true"));

    let error = toml::from_str::<RuntimeLaunchPolicyRecord>(
        "runtime_idle_seconds = '300'\nmodel_idle_seconds = '-1'\n",
    )
    .expect_err("negative policy must be rejected");
    assert!(error.to_string().contains("non-negative"));

    let error = toml::from_str::<RuntimeLaunchPolicyRecord>(
        "runtime_idle_seconds = 30\nmodel_idle_seconds = 31\n",
    )
    .expect_err("invalid clock ordering must be rejected");
    assert!(error.to_string().contains("less than or equal"));
}

fn runtime_policy() -> RuntimeLaunchPolicyRecord {
    RuntimeLaunchPolicyRecord {
        runtime_idle_seconds: 300,
        model_idle_seconds: 0,
        legacy_unbounded_model: false,
    }
}

fn temp_root(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "tentgent-runtime-ownership-{label}-{}",
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
