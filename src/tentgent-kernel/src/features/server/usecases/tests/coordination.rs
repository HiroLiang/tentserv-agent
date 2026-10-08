use std::sync::{Arc, Mutex};

use crate::features::resource_coordination::{
    infra::FileResourceCoordinator, ResourceBusy, ResourceCoordinator, ResourceKey, ResourceKind,
    ResourceLockMode, ResourceLockRequest, ResourcePermit,
};
use crate::features::resource_guard::StdResourceGuard;
use crate::features::server::infra::StdServerProcessIdentityProbe;
use crate::foundation::layout::RuntimeLayout;

use super::*;

/// Applies a definition update in the preflight/acquire gap, before real locks.
#[derive(Default)]
struct RepointingCoordinator {
    replacements: Mutex<std::collections::VecDeque<ClusterDefinition>>,
    model_updates: Mutex<std::collections::VecDeque<ModelMetadata>>,
    requests: Mutex<Vec<Vec<(ResourceKey, ResourceLockMode)>>>,
}

impl ResourceCoordinator for RepointingCoordinator {
    fn acquire(
        &self,
        layout: &RuntimeLayout,
        request: ResourceLockRequest,
    ) -> KernelResult<Result<ResourcePermit, ResourceBusy>> {
        self.requests.lock().unwrap().push(request.locks.clone());
        if let Some(definition) = self.replacements.lock().unwrap().pop_front() {
            FileClusterCatalogStore.save_cluster(
                &ClusterStoreLayout::from_home_dir(layout.home_dir.clone()),
                &definition,
            )?;
        }
        if let Some(metadata) = self.model_updates.lock().unwrap().pop_front() {
            FileModelCatalogStore.save_model_metadata(
                &ModelStoreLayout::from_models_dir(layout.models_dir.clone()),
                &metadata,
            )?;
        }
        FileResourceCoordinator.acquire(
            layout,
            request.with_limits(std::time::Duration::from_millis(100), 1),
        )
    }
}

struct ClusterCoordinationFixture {
    fixture: Fixture,
    first: ModelRef,
    second: ModelRef,
    cluster_ref: ClusterRef,
    catalog: FileServerCatalogStore<StaticProcessProbe>,
}

impl ClusterCoordinationFixture {
    fn new(label: &str) -> Self {
        let mut fixture = Fixture::new(label);
        fixture.write_model_format_capabilities(ModelFormat::Gguf, vec![ModelCapability::Chat]);
        fixture.write_model_capabilities(vec![ModelCapability::Chat]);
        let first = fixture.model_ref.clone();
        fixture.model_ref = ModelRef::parse("b".repeat(64)).unwrap();
        fixture.write_model_capabilities(vec![ModelCapability::Chat]);
        let second = fixture.model_ref.clone();
        let cluster_ref = ClusterRef::parse("coordinated-chat").unwrap();
        let layout = StdRuntimeLayoutResolver
            .resolve(fixture.layout_input(LayoutResolveMode::ReadOnly))
            .unwrap();
        FileClusterCatalogStore
            .save_cluster(
                &ClusterStoreLayout::from_home_dir(layout.home_dir),
                &cluster_definition(cluster_ref.clone(), first.clone(), false),
            )
            .unwrap();
        Self {
            fixture,
            first,
            second,
            cluster_ref,
            catalog: FileServerCatalogStore::new(StaticProcessProbe { running: false }),
        }
    }

    fn usecase(&self, coordinator: Arc<dyn ResourceCoordinator>) -> StdServerUseCase<'_> {
        StdServerUseCase::new_with_dependencies(
            &StdRuntimeLayoutResolver,
            &StdServerStoreLayoutInitializer,
            &FileModelCatalogStore,
            &FileModelCapabilityProofStore,
            &FileClusterCatalogStore,
            &StdServerIdentityGenerator,
            &self.catalog,
            &StaticProcessController,
            &StaticClock,
            coordinator,
            Arc::new(StdResourceGuard::default()),
            Arc::new(StdServerProcessIdentityProbe),
        )
    }

    fn request(&self) -> ServerPrepareRequest {
        ServerPrepareRequest {
            layout: self.fixture.layout_input(LayoutResolveMode::Create),
            target: ServerPrepareTarget::Cluster {
                cluster_ref: self.cluster_ref.clone(),
            },
            host: None,
            port: Some(8798),
            lazy_load: Some(true).into(),
            idle_seconds: None.into(),
            model_idle_seconds: None.into(),
            allow_unverified: true,
        }
    }

    fn coordinator(&self, targets: &[ModelRef]) -> Arc<RepointingCoordinator> {
        Arc::new(RepointingCoordinator {
            replacements: Mutex::new(
                targets
                    .iter()
                    .map(|target| {
                        cluster_definition(self.cluster_ref.clone(), target.clone(), false)
                    })
                    .collect(),
            ),
            requests: Mutex::default(),
            model_updates: Mutex::default(),
        })
    }
}

fn assert_chat_locks(locks: &[(ResourceKey, ResourceLockMode)], model_ref: &ModelRef) {
    assert!(locks.contains(&(
        ResourceKey::new(ResourceKind::Model, model_ref.to_string()),
        ResourceLockMode::Shared,
    )));
    assert!(locks.contains(&(
        ResourceKey::new(ResourceKind::ModelCapability, format!("{model_ref}|chat")),
        ResourceLockMode::Shared,
    )));
}

#[test]
fn cluster_prepare_retries_complete_set_when_chat_target_changes() {
    let fixture = ClusterCoordinationFixture::new("prepare-repoint");
    let coordinator = fixture.coordinator(std::slice::from_ref(&fixture.second));
    let servers = fixture.usecase(coordinator.clone());
    let prepared = servers.prepare_server(fixture.request()).unwrap();
    assert!(prepared.outcome.created);
    let requests = coordinator.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_chat_locks(&requests[0], &fixture.first);
    assert_chat_locks(&requests[1], &fixture.second);
}

#[test]
fn cluster_start_retries_complete_set_when_chat_target_changes() {
    let fixture = ClusterCoordinationFixture::new("start-repoint");
    let prepared = fixture
        .usecase(Arc::new(FileResourceCoordinator))
        .prepare_server(fixture.request())
        .unwrap();
    let coordinator = fixture.coordinator(std::slice::from_ref(&fixture.second));
    let servers = fixture.usecase(coordinator.clone());
    let authorization = servers
        .resolve_for_start_guarded(ServerResolveForStartRequest {
            layout: fixture.fixture.layout_input(LayoutResolveMode::ReadOnly),
            selector: ServerRefSelector::parse(
                prepared.outcome.inspection.spec.server_ref.as_str(),
            )
            .unwrap(),
            allow_unverified: true,
        })
        .unwrap();
    let requests = coordinator.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_chat_locks(&requests[0], &fixture.first);
    assert_chat_locks(&requests[1], &fixture.second);
    assert!(authorization.permit.covers(
        &ResourceKey::new(ResourceKind::Model, fixture.second.to_string()),
        ResourceLockMode::Shared,
    ));
    assert!(!authorization.permit.covers(
        &ResourceKey::new(ResourceKind::Model, fixture.first.to_string()),
        ResourceLockMode::Shared,
    ));
}

#[test]
fn cluster_prepare_stops_after_three_changing_lock_sets_without_saving_spec() {
    let fixture = ClusterCoordinationFixture::new("prepare-unstable");
    let coordinator = fixture.coordinator(&[
        fixture.second.clone(),
        fixture.first.clone(),
        fixture.second.clone(),
    ]);
    let servers = fixture.usecase(coordinator.clone());
    let error = servers.prepare_server(fixture.request()).unwrap_err();
    assert!(matches!(
        error,
        crate::foundation::error::KernelError::ResourceStateUnstable { .. }
    ));
    assert_eq!(coordinator.requests.lock().unwrap().len(), 3);
    assert!(servers
        .list_servers(ServerListRequest {
            layout: fixture.fixture.layout_input(LayoutResolveMode::ReadOnly),
            running_only: false,
        })
        .unwrap()
        .servers
        .is_empty());
}

#[test]
fn cluster_start_stops_after_three_changing_lock_sets() {
    let fixture = ClusterCoordinationFixture::new("start-unstable");
    let prepared = fixture
        .usecase(Arc::new(FileResourceCoordinator))
        .prepare_server(fixture.request())
        .unwrap();
    let coordinator = fixture.coordinator(&[
        fixture.second.clone(),
        fixture.first.clone(),
        fixture.second.clone(),
    ]);
    let servers = fixture.usecase(coordinator.clone());
    let error = servers
        .resolve_for_start_guarded(ServerResolveForStartRequest {
            layout: fixture.fixture.layout_input(LayoutResolveMode::ReadOnly),
            selector: ServerRefSelector::parse(
                prepared.outcome.inspection.spec.server_ref.as_str(),
            )
            .unwrap(),
            allow_unverified: true,
        })
        .unwrap_err();
    assert!(matches!(
        error,
        crate::foundation::error::KernelError::ResourceStateUnstable { .. }
    ));
    assert_eq!(coordinator.requests.lock().unwrap().len(), 3);
}

#[test]
fn local_gate_borrows_selected_capability_without_locking_other_capabilities() {
    let fixture = ClusterCoordinationFixture::new("selected-capability");
    let layout = StdRuntimeLayoutResolver
        .resolve(fixture.fixture.layout_input(LayoutResolveMode::ReadOnly))
        .unwrap();
    let _other_capability = FileResourceCoordinator
        .acquire(
            &layout,
            ResourceLockRequest::new(
                "hold-unselected-capability",
                vec![(
                    ResourceKey::new(
                        ResourceKind::ModelCapability,
                        format!("{}|embedding", fixture.first),
                    ),
                    ResourceLockMode::Exclusive,
                )],
            ),
        )
        .unwrap()
        .unwrap();
    let servers = fixture.usecase(Arc::new(FileResourceCoordinator));
    let mut request = fixture.request();
    request.target = ServerPrepareTarget::RuntimeRef {
        runtime_ref: fixture.first.to_string(),
        capability: Some(ServerCapability::Chat),
    };
    let prepared = servers.prepare_server(request).unwrap();
    servers
        .resolve_for_start_guarded(ServerResolveForStartRequest {
            layout: fixture.fixture.layout_input(LayoutResolveMode::ReadOnly),
            selector: ServerRefSelector::parse(
                prepared.outcome.inspection.spec.server_ref.as_str(),
            )
            .unwrap(),
            allow_unverified: true,
        })
        .unwrap();
}

#[test]
fn local_prepare_replans_changed_backend_or_inferred_capability_before_proof_access() {
    for change_format in [false, true] {
        let fixture = ClusterCoordinationFixture::new(if change_format {
            "changed-backend"
        } else {
            "changed-inferred-capability"
        });
        let layout = StdRuntimeLayoutResolver
            .resolve(fixture.fixture.layout_input(LayoutResolveMode::ReadOnly))
            .unwrap();
        let store = ModelStoreLayout::from_models_dir(layout.models_dir);
        let mut changed = FileModelCatalogStore
            .load_model_metadata(&store, &fixture.first)
            .unwrap();
        let expected_capability = if change_format {
            changed.primary_format = ModelFormat::Gguf;
            changed.detected_formats = vec![ModelFormat::Gguf];
            ServerCapability::Chat
        } else {
            changed.model_capabilities = vec![ModelCapability::Embedding];
            ServerCapability::Embedding
        };
        let coordinator = Arc::new(RepointingCoordinator {
            model_updates: Mutex::new([changed].into()),
            ..Default::default()
        });
        let servers = fixture.usecase(coordinator.clone());
        let mut request = fixture.request();
        request.target = ServerPrepareTarget::RuntimeRef {
            runtime_ref: fixture.first.to_string(),
            capability: None,
        };
        let result = servers.prepare_server(request).unwrap();
        assert_eq!(
            result.outcome.inspection.spec.capability,
            Some(expected_capability)
        );
        let requests = coordinator.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        if change_format {
            let model_locks = |locks: &Vec<(ResourceKey, ResourceLockMode)>| {
                locks
                    .iter()
                    .filter(|(key, _)| {
                        matches!(
                            key.kind,
                            ResourceKind::Model | ResourceKind::ModelCapability
                        )
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                model_locks(&requests[0]),
                model_locks(&requests[1]),
                "backend can change without changing proof keys"
            );
            assert_ne!(
                requests[0], requests[1],
                "the replacement spec must have its newly resolved server identity"
            );
            assert_eq!(
                result
                    .outcome
                    .inspection
                    .spec
                    .runtime_profile
                    .unwrap()
                    .label(),
                "local-chat-llama-cpp-v1",
            );
        } else {
            assert_ne!(requests[0], requests[1]);
            assert!(requests[1].contains(&(
                ResourceKey::new(
                    ResourceKind::ModelCapability,
                    format!("{}|embedding", fixture.first)
                ),
                ResourceLockMode::Shared,
            )));
        }
    }
}
