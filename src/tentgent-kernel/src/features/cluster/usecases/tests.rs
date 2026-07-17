use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::features::auth::domain::{AuthKeyStatus, KeychainPresence, Provider};
use crate::features::auth::usecases::{AuthStatusReport, AuthStatusRequest, AuthStatusUseCase};
use crate::features::cluster::domain::{
    ClusterDefinition, ClusterReadinessStatus, ClusterRef, ClusterRouteExecutionBlockerCode,
    ClusterRouteExecutionDecision, ClusterRouteKey, ClusterRouteReadinessStatus,
    ClusterRouteTarget, CLUSTER_SCHEMA_VERSION,
};
use crate::features::cluster::infra::{FileClusterCatalogStore, StdClusterStoreLayoutInitializer};
use crate::features::cluster::ports::ClusterCatalogStore;
use crate::features::cluster::usecases::{
    ClusterApplyFileRequest, ClusterInspectRequest, ClusterReadinessInspectRequest,
    ClusterReadinessUseCase, ClusterRemoveRequest, ClusterRouteExecutionUseCase,
    ClusterRouteResolveRequest, ClusterSpecUseCase, StdClusterReadinessUseCase,
    StdClusterRouteExecutionUseCase, StdClusterUseCase,
};
use crate::features::model::domain::{
    default_model_capability_source, ModelCapability, ModelCapabilityProof,
    ModelCapabilityProofSource, ModelCapabilityProofStatus, ModelFormat, ModelImportMethod,
    ModelMetadata, ModelRef, ModelSourceKind, ModelStoreLayout, ModelVariantMetadata,
    ModelVariantStatus, SOURCE_DIRNAME,
};
use crate::features::model::infra::{FileModelCapabilityProofStore, FileModelCatalogStore};
use crate::features::model::ports::{ModelCapabilityProofStore, ModelCatalogStore};
use crate::features::server::domain::{CloudProvider, ServerRuntimeProfileSelection};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
};

#[test]
fn cluster_ref_rejects_path_like_values() {
    assert!(ClusterRef::parse("../secret").is_err());
    assert!(ClusterRef::parse(".hidden").is_err());
    assert!(ClusterRef::parse("Local").is_err());
    assert!(ClusterRef::parse(" local-assistant").is_err());
    assert!(ClusterRef::parse("local-assistant ").is_err());
    assert!(ClusterRef::parse("local assistant").is_err());
    assert!(ClusterRef::parse("local-assistant").is_ok());
}

#[test]
fn route_key_limits_first_cluster_mvp_routes() {
    assert_eq!(
        ClusterRouteKey::parse("audio-transcription").expect("route"),
        ClusterRouteKey::AudioTranscription
    );
    assert!(ClusterRouteKey::parse(" chat").is_err());
    assert!(ClusterRouteKey::parse("chat ").is_err());
    assert!(ClusterRouteKey::parse("image-generation").is_err());
}

#[test]
fn apply_cluster_file_reads_toml_validates_and_stores_definition() {
    let root = unique_path("cluster-apply-file");
    let source_dir = root.join("source");
    let home = root.join("home");
    fs::create_dir_all(&source_dir).expect("source dir");
    let source_path = source_dir.join("cluster.toml");
    fs::write(
        &source_path,
        r#"
schema_version = 1
cluster_ref = "local-assistant"

[routes.chat]
kind = "provider"
provider = "openai"
provider_model = "gpt-test"
"#,
    )
    .expect("cluster source");

    let layout_resolver = StdRuntimeLayoutResolver;
    let layout_initializer = StdClusterStoreLayoutInitializer;
    let catalog = FileClusterCatalogStore;
    let model_catalog = FileModelCatalogStore;
    let usecase = StdClusterUseCase::new(
        &layout_resolver,
        &layout_initializer,
        &catalog,
        &model_catalog,
    );

    let applied = usecase
        .apply_cluster_file(ClusterApplyFileRequest {
            layout: layout_input(&home, LayoutResolveMode::Create),
            source_path,
            force_unsafe_source: false,
        })
        .expect("apply cluster file");

    assert_eq!(
        applied.inspection.definition.cluster_ref.to_string(),
        "local-assistant"
    );
    assert_eq!(applied.inspection.definition.routes.len(), 1);
    assert!(applied.inspection.definition_path.exists());

    let stored =
        fs::read_to_string(&applied.inspection.definition_path).expect("stored cluster definition");
    assert!(stored.contains(r#"cluster_ref = "local-assistant""#));
    assert!(stored.contains(r#"provider_model = "gpt-test""#));

    let inspected = usecase
        .inspect_cluster(ClusterInspectRequest {
            layout: layout_input(&home, LayoutResolveMode::ReadOnly),
            cluster_ref: ClusterRef::parse("local-assistant").expect("cluster ref"),
        })
        .expect("inspect stored cluster");
    assert_eq!(
        inspected.inspection.definition,
        applied.inspection.definition
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn remove_cluster_is_blocked_by_stored_cluster_server_spec() {
    let root = unique_path("cluster-remove-server-blocker");
    let home = root.join("home");
    let layout_resolver = StdRuntimeLayoutResolver;
    let layout = layout_resolver
        .resolve(layout_input(&home, LayoutResolveMode::Create))
        .expect("layout");
    let cluster_ref = ClusterRef::parse("local-assistant").expect("cluster ref");
    let cluster_store =
        crate::features::cluster::domain::ClusterStoreLayout::from_home_dir(home.clone());
    let cluster_catalog = FileClusterCatalogStore;
    cluster_catalog
        .save_cluster(
            &cluster_store,
            &ClusterDefinition {
                schema_version: CLUSTER_SCHEMA_VERSION,
                cluster_ref: cluster_ref.clone(),
                route_update_policy: Default::default(),
                routes: [(
                    ClusterRouteKey::Chat,
                    ClusterRouteTarget::Provider {
                        provider: CloudProvider::OpenAI,
                        provider_model: "gpt-test".to_string(),
                    },
                )]
                .into_iter()
                .collect(),
            },
        )
        .expect("save cluster");
    let server_dir = layout.servers_dir.join("server-ref");
    fs::create_dir_all(&server_dir).expect("server dir");
    fs::write(
        server_dir.join("server.toml"),
        "short_ref = \"abc123\"\ncluster_ref = \"local-assistant\"\n",
    )
    .expect("server spec");
    let usecase = StdClusterUseCase::new(
        &layout_resolver,
        &StdClusterStoreLayoutInitializer,
        &cluster_catalog,
        &FileModelCatalogStore,
    );

    let error = usecase
        .remove_cluster(ClusterRemoveRequest {
            layout: layout_input(&home, LayoutResolveMode::Create),
            cluster_ref,
        })
        .expect_err("stored server spec must block cluster removal");

    assert!(error.to_string().contains("delete-cluster"));
    assert!(error.to_string().contains("server-spec abc123"));
    assert!(cluster_store
        .cluster_definition_path("local-assistant")
        .exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn readiness_reports_verified_local_route_with_inferred_profile() {
    let root = unique_path("cluster-readiness-verified");
    let model_ref = ModelRef::parse("a".repeat(64)).expect("model ref");
    let profile = ServerRuntimeProfileSelection::new("local-chat-llama-cpp", 1);
    let readiness = readiness_for(
        &root,
        ClusterRouteTarget::LocalModel {
            model_ref: model_ref.clone(),
            runtime_profile: None,
        },
        Some(metadata_fixture(
            model_ref.clone(),
            vec![ModelCapability::Chat],
        )),
        vec![proof_fixture(
            model_ref,
            ModelCapability::Chat,
            ModelCapabilityProofStatus::Verified,
            "gguf",
            Some(profile),
            None,
        )],
        FakeAuthStatus::default(),
    );

    assert_eq!(readiness.status, ClusterReadinessStatus::Ready);
    let route = readiness.routes.first().expect("route readiness");
    assert_eq!(route.status, ClusterRouteReadinessStatus::Verified);
    assert_eq!(route.runtime_profile.source.as_str(), "inferred");
    assert!(route
        .flags
        .contains(&"runtime-profile-inferred".to_string()));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn readiness_reports_unknown_local_route_without_proof() {
    let root = unique_path("cluster-readiness-unknown");
    let model_ref = ModelRef::parse("b".repeat(64)).expect("model ref");
    let readiness = readiness_for(
        &root,
        ClusterRouteTarget::LocalModel {
            model_ref: model_ref.clone(),
            runtime_profile: Some(ServerRuntimeProfileSelection::new(
                "local-chat-llama-cpp",
                1,
            )),
        },
        Some(metadata_fixture(model_ref, vec![ModelCapability::Chat])),
        Vec::new(),
        FakeAuthStatus::default(),
    );

    assert_eq!(readiness.status, ClusterReadinessStatus::Blocked);
    let route = readiness.routes.first().expect("route readiness");
    assert_eq!(route.status, ClusterRouteReadinessStatus::Unknown);
    assert!(route
        .next_actions
        .iter()
        .any(|action| action.code.as_str() == "verify-model-capability"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn readiness_reports_stale_local_proof() {
    let root = unique_path("cluster-readiness-stale");
    let model_ref = ModelRef::parse("c".repeat(64)).expect("model ref");
    let readiness = readiness_for(
        &root,
        ClusterRouteTarget::LocalModel {
            model_ref: model_ref.clone(),
            runtime_profile: None,
        },
        Some(metadata_fixture(
            model_ref.clone(),
            vec![ModelCapability::Chat],
        )),
        vec![proof_fixture(
            model_ref,
            ModelCapability::Chat,
            ModelCapabilityProofStatus::Verified,
            "old-backend",
            None,
            None,
        )],
        FakeAuthStatus::default(),
    );

    let route = readiness.routes.first().expect("route readiness");
    assert_eq!(route.status, ClusterRouteReadinessStatus::Stale);
    assert!(route
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("backend changed")));
    assert!(route.flags.contains(&"stale-proof".to_string()));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn readiness_reports_failed_local_proof_with_recovery_actions() {
    let root = unique_path("cluster-readiness-failed");
    let model_ref = ModelRef::parse("d".repeat(64)).expect("model ref");
    let readiness = readiness_for(
        &root,
        ClusterRouteTarget::LocalModel {
            model_ref: model_ref.clone(),
            runtime_profile: Some(ServerRuntimeProfileSelection::new(
                "local-chat-llama-cpp",
                1,
            )),
        },
        Some(metadata_fixture(
            model_ref.clone(),
            vec![ModelCapability::Chat],
        )),
        vec![proof_fixture(
            model_ref,
            ModelCapability::Chat,
            ModelCapabilityProofStatus::Failed,
            "gguf",
            Some(ServerRuntimeProfileSelection::new(
                "local-chat-llama-cpp",
                1,
            )),
            Some("runtime failed to load model".to_string()),
        )],
        FakeAuthStatus::default(),
    );

    let route = readiness.routes.first().expect("route readiness");
    assert_eq!(route.status, ClusterRouteReadinessStatus::Failed);
    assert!(route
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("runtime failed")));
    assert!(route
        .next_actions
        .iter()
        .any(|action| action.code.as_str() == "clear-model-proof"));
    assert!(route
        .next_actions
        .iter()
        .any(|action| action.code.as_str() == "verify-model-capability"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn readiness_reports_unsupported_when_model_lacks_route_capability() {
    let root = unique_path("cluster-readiness-unsupported");
    let model_ref = ModelRef::parse("e".repeat(64)).expect("model ref");
    let readiness = readiness_for(
        &root,
        ClusterRouteTarget::LocalModel {
            model_ref: model_ref.clone(),
            runtime_profile: None,
        },
        Some(metadata_fixture(
            model_ref,
            vec![ModelCapability::Embedding],
        )),
        Vec::new(),
        FakeAuthStatus::default(),
    );

    let route = readiness.routes.first().expect("route readiness");
    assert_eq!(route.status, ClusterRouteReadinessStatus::Unsupported);
    assert!(route
        .next_actions
        .iter()
        .any(|action| action.code.as_str() == "inspect-model"));
    assert!(route
        .next_actions
        .iter()
        .any(|action| action.code.as_str() == "choose-supported-route-target"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn readiness_reports_provider_auth_missing_without_secret_resolution() {
    let root = unique_path("cluster-readiness-provider-auth");
    let readiness = readiness_for(
        &root,
        ClusterRouteTarget::Provider {
            provider: CloudProvider::OpenAI,
            provider_model: "gpt-test".to_string(),
        },
        None,
        Vec::new(),
        FakeAuthStatus {
            statuses: vec![AuthKeyStatus::local(
                Provider::OpenAI,
                false,
                KeychainPresence::Unknown,
            )],
        },
    );

    let route = readiness.routes.first().expect("route readiness");
    assert_eq!(route.status, ClusterRouteReadinessStatus::AuthMissing);
    assert!(route
        .next_actions
        .iter()
        .any(|action| action.code.as_str() == "set-provider-auth"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn route_execution_allows_unknown_only_with_explicit_override() {
    let root = unique_path("cluster-route-execution-unknown");
    let home = root.join("home");
    let layout_resolver = StdRuntimeLayoutResolver;
    let runtime_layout = layout_resolver
        .resolve(layout_input(&home, LayoutResolveMode::Create))
        .expect("runtime layout");
    let model_ref = ModelRef::parse("f".repeat(64)).expect("model ref");
    let model_store = ModelStoreLayout::from_models_dir(runtime_layout.models_dir.clone());
    let model_catalog = FileModelCatalogStore;
    model_catalog
        .save_model_metadata(
            &model_store,
            &metadata_fixture(model_ref.clone(), vec![ModelCapability::Chat]),
        )
        .expect("save model metadata");
    model_catalog
        .save_variant_metadata(
            &model_store,
            &model_ref,
            &ModelVariantMetadata {
                format: ModelFormat::Gguf,
                status: ModelVariantStatus::Imported,
                import_method: ModelImportMethod::Add,
                relative_source_path: SOURCE_DIRNAME.to_string(),
            },
        )
        .expect("save variant metadata");
    let source = model_store.variant_source_dir(&model_ref, ModelFormat::Gguf);
    fs::create_dir_all(&source).expect("source dir");
    fs::write(source.join("model.gguf"), "fixture").expect("gguf fixture");
    let definition = ClusterDefinition {
        schema_version: CLUSTER_SCHEMA_VERSION,
        cluster_ref: ClusterRef::parse("local-assistant").expect("cluster ref"),
        route_update_policy: Default::default(),
        routes: [(
            ClusterRouteKey::Chat,
            ClusterRouteTarget::LocalModel {
                model_ref,
                runtime_profile: None,
            },
        )]
        .into_iter()
        .collect(),
    };
    let resolver = StdClusterRouteExecutionUseCase::new(
        &layout_resolver,
        &model_catalog,
        &FileModelCapabilityProofStore,
    );

    let blocked = resolver
        .resolve_cluster_route(ClusterRouteResolveRequest {
            layout: layout_input(&home, LayoutResolveMode::ReadOnly),
            definition: definition.clone(),
            route: ClusterRouteKey::Chat,
            allow_unverified: false,
        })
        .expect("blocked decision");
    assert!(matches!(
        blocked.decision,
        ClusterRouteExecutionDecision::Blocked {
            code: ClusterRouteExecutionBlockerCode::NotReady,
            ..
        }
    ));

    let allowed = resolver
        .resolve_cluster_route(ClusterRouteResolveRequest {
            layout: layout_input(&home, LayoutResolveMode::ReadOnly),
            definition,
            route: ClusterRouteKey::Chat,
            allow_unverified: true,
        })
        .expect("ready decision");
    assert!(matches!(
        allowed.decision,
        ClusterRouteExecutionDecision::Ready(_)
    ));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn route_execution_reports_missing_and_provider_targets_without_fallback() {
    let root = unique_path("cluster-route-execution-blockers");
    let home = root.join("home");
    let resolver = StdClusterRouteExecutionUseCase::new(
        &StdRuntimeLayoutResolver,
        &FileModelCatalogStore,
        &FileModelCapabilityProofStore,
    );
    let cluster_ref = ClusterRef::parse("local-assistant").expect("cluster ref");
    let missing = resolver
        .resolve_cluster_route(ClusterRouteResolveRequest {
            layout: layout_input(&home, LayoutResolveMode::ReadOnly),
            definition: ClusterDefinition {
                schema_version: CLUSTER_SCHEMA_VERSION,
                cluster_ref: cluster_ref.clone(),
                route_update_policy: Default::default(),
                routes: Default::default(),
            },
            route: ClusterRouteKey::Embedding,
            allow_unverified: true,
        })
        .expect("missing decision");
    assert!(matches!(
        missing.decision,
        ClusterRouteExecutionDecision::Blocked {
            code: ClusterRouteExecutionBlockerCode::MissingRoute,
            ..
        }
    ));

    let provider = resolver
        .resolve_cluster_route(ClusterRouteResolveRequest {
            layout: layout_input(&home, LayoutResolveMode::ReadOnly),
            definition: ClusterDefinition {
                schema_version: CLUSTER_SCHEMA_VERSION,
                cluster_ref,
                route_update_policy: Default::default(),
                routes: [(
                    ClusterRouteKey::Embedding,
                    ClusterRouteTarget::Provider {
                        provider: CloudProvider::OpenAI,
                        provider_model: "text-embedding-test".to_string(),
                    },
                )]
                .into_iter()
                .collect(),
            },
            route: ClusterRouteKey::Embedding,
            allow_unverified: true,
        })
        .expect("provider decision");
    assert!(matches!(
        provider.decision,
        ClusterRouteExecutionDecision::Blocked {
            code: ClusterRouteExecutionBlockerCode::UnsupportedTarget,
            ..
        }
    ));
    let _ = fs::remove_dir_all(root);
}

fn layout_input(home: &std::path::Path, mode: LayoutResolveMode) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode,
        home_dir: Some(home.to_path_buf()),
        data_root_dir: None,
    }
}

fn unique_path(label: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("tentgent-{label}-{nanos}"))
}

fn readiness_for(
    root: &std::path::Path,
    target: ClusterRouteTarget,
    metadata: Option<ModelMetadata>,
    proofs: Vec<ModelCapabilityProof>,
    auth: FakeAuthStatus,
) -> crate::features::cluster::domain::ClusterReadinessReport {
    let home = root.join("home");
    let layout_resolver = StdRuntimeLayoutResolver;
    let runtime_layout = layout_resolver
        .resolve(layout_input(&home, LayoutResolveMode::Create))
        .expect("runtime layout");
    let model_layout = ModelStoreLayout::from_models_dir(runtime_layout.models_dir.clone());
    let model_catalog = FileModelCatalogStore;
    if let Some(metadata) = metadata {
        model_catalog
            .save_model_metadata(&model_layout, &metadata)
            .expect("save model metadata");
    }
    let proof_store = FileModelCapabilityProofStore;
    for proof in proofs {
        proof_store
            .save_capability_proof(&model_layout, &proof)
            .expect("save proof");
    }

    let cluster_ref = ClusterRef::parse("local-assistant").expect("cluster ref");
    let definition = ClusterDefinition {
        schema_version: CLUSTER_SCHEMA_VERSION,
        cluster_ref: cluster_ref.clone(),
        route_update_policy: Default::default(),
        routes: [(ClusterRouteKey::Chat, target)].into_iter().collect(),
    };
    let cluster_store =
        crate::features::cluster::domain::ClusterStoreLayout::from_home_dir(home.clone());
    let cluster_catalog = FileClusterCatalogStore;
    cluster_catalog
        .save_cluster(&cluster_store, &definition)
        .expect("save cluster");

    StdClusterReadinessUseCase::new(
        &layout_resolver,
        &cluster_catalog,
        &model_catalog,
        &proof_store,
        &auth,
    )
    .inspect_cluster_readiness(ClusterReadinessInspectRequest {
        layout: layout_input(&home, LayoutResolveMode::ReadOnly),
        cluster_ref,
    })
    .expect("readiness")
    .readiness
}

fn metadata_fixture(model_ref: ModelRef, capabilities: Vec<ModelCapability>) -> ModelMetadata {
    ModelMetadata {
        short_ref: model_ref.short_ref().to_string(),
        model_ref,
        source_kind: ModelSourceKind::Local,
        source_repo: None,
        source_revision: None,
        source_path: Some("/tmp/source".to_string()),
        primary_format: ModelFormat::Gguf,
        detected_formats: vec![ModelFormat::Gguf],
        mlx_runtime_family: None,
        model_capabilities: capabilities,
        model_capability_source: default_model_capability_source(),
        file_count: 1,
        total_bytes: 11,
        imported_at: "2026-07-08T00:00:00Z".to_string(),
    }
}

fn proof_fixture(
    model_ref: ModelRef,
    capability: ModelCapability,
    status: ModelCapabilityProofStatus,
    backend: impl Into<String>,
    runtime_profile: Option<ServerRuntimeProfileSelection>,
    error: Option<String>,
) -> ModelCapabilityProof {
    ModelCapabilityProof {
        model_ref,
        capability,
        status,
        source: ModelCapabilityProofSource::ManualProbe,
        primary_format: ModelFormat::Gguf,
        mlx_runtime_family: None,
        backend: backend.into(),
        runtime_version: None,
        runtime_profile: runtime_profile
            .as_ref()
            .map(|profile| profile.profile_id.clone()),
        runtime_profile_version: runtime_profile.map(|profile| profile.profile_version),
        server_ref: None,
        checked_at: "2026-07-08T00:00:00Z".to_string(),
        error,
    }
}

#[derive(Default)]
struct FakeAuthStatus {
    statuses: Vec<AuthKeyStatus>,
}

impl AuthStatusUseCase for FakeAuthStatus {
    fn status(&self, request: AuthStatusRequest) -> KernelResult<AuthStatusReport> {
        Ok(AuthStatusReport {
            statuses: self
                .statuses
                .iter()
                .filter(|status| request.providers.contains(&status.provider))
                .cloned()
                .collect(),
        })
    }
}
