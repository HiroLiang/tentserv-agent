use super::*;
use crate::features::server::domain::ServerRuntimeTarget;
use crate::features::server::options::{LifecycleInput, LoadMode};
use crate::features::server::ports::ServerIdentityGenerator;

fn with_servers(label: &str, test: impl FnOnce(&Fixture, &StdServerUseCase<'_>)) {
    let fixture = Fixture::new(label);
    let catalog = FileServerCatalogStore::new(StaticProcessProbe { running: false });
    let servers = StdServerUseCase::new(
        &StdRuntimeLayoutResolver,
        &StdServerStoreLayoutInitializer,
        &FileModelCatalogStore,
        &FileModelCapabilityProofStore,
        &StdServerIdentityGenerator,
        &catalog,
        &StaticProcessController,
        &StaticClock,
    );
    test(&fixture, &servers);
}

fn request(fixture: &Fixture, runtime_ref: &str) -> ServerPrepareRequest {
    ServerPrepareRequest {
        layout: fixture.layout_input(LayoutResolveMode::Create),
        target: ServerPrepareTarget::RuntimeRef {
            runtime_ref: runtime_ref.into(),
            capability: None,
        },
        host: None,
        port: Some(8780),
        lazy_load: LifecycleInput::Omitted,
        idle_seconds: LifecycleInput::Omitted,
        model_idle_seconds: LifecycleInput::Omitted,
        allow_unverified: true,
    }
}

#[test]
fn cloud_rejects_each_explicit_lifecycle_input_including_defaults_and_null() {
    with_servers("cloud-options", |fixture, servers| {
        for value in [None, Some(false), Some(true)] {
            let mut input = request(fixture, "openai:gpt-4.1-mini");
            input.lazy_load = LifecycleInput::Provided(value);
            let error = servers.prepare_server(input).unwrap_err().to_string();
            assert!(error.contains("lazy_load"), "{error}");
            assert!(error.contains("not applicable"), "{error}");
        }
        for value in [None, Some(0), Some(300)] {
            for model in [false, true] {
                let mut input = request(fixture, "openai:gpt-4.1-mini");
                if model {
                    input.model_idle_seconds = LifecycleInput::Provided(value);
                } else {
                    input.idle_seconds = LifecycleInput::Provided(value);
                }
                let error = servers.prepare_server(input).unwrap_err().to_string();
                assert!(
                    error.contains(if model {
                        "model_idle_seconds"
                    } else {
                        "runtime_idle_seconds"
                    }),
                    "{error}"
                );
            }
        }
        assert!(servers
            .list_servers(ServerListRequest {
                layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
                running_only: false,
            })
            .unwrap()
            .servers
            .is_empty());
    });
}

#[test]
fn cloud_canonical_identity_and_legacy_read_start_are_preserved() {
    with_servers("cloud-legacy", |fixture, servers| {
        let target = ServerRuntimeTarget::CloudProvider {
            provider: CloudProvider::OpenAI,
            provider_model: "gpt-4.1-mini".into(),
            capability: ServerCapability::Chat,
        };
        let prepared = servers
            .prepare_server(request(fixture, "openai:gpt-4.1-mini"))
            .unwrap();
        let expected = StdServerIdentityGenerator
            .server_ref_for_target(&target, "127.0.0.1", 8780, false, false, None)
            .unwrap();
        assert_eq!(prepared.outcome.inspection.spec.server_ref, expected);
        assert!(
            !servers
                .prepare_server(request(fixture, "openai:gpt-4.1-mini"))
                .unwrap()
                .outcome
                .created
        );

        let mut legacy = prepared.outcome.inspection.spec;
        legacy.lazy_load = true;
        legacy.idle_seconds = Some(42);
        legacy.server_ref = StdServerIdentityGenerator
            .server_ref_for_target(&target, "127.0.0.1", 8780, false, true, Some(42))
            .unwrap();
        legacy.short_ref = legacy.server_ref.short_ref().into();
        let catalog = FileServerCatalogStore::new(StaticProcessProbe { running: false });
        catalog.save_server_spec(&prepared.store, &legacy).unwrap();
        let path = prepared
            .store
            .server_dir(&legacy.server_ref)
            .join("server.toml");
        let before = std::fs::read(&path).unwrap();
        let start = servers
            .resolve_for_start(ServerResolveForStartRequest {
                layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
                selector: ServerRefSelector::parse(legacy.server_ref.as_str()).unwrap(),
                allow_unverified: false,
            })
            .unwrap();
        assert_eq!(start.inspection.spec, legacy);
        assert_eq!(std::fs::read(path).unwrap(), before);
    });
}

#[test]
fn image_lazy_only_covers_inferred_capability_both_backends_and_legacy_start() {
    for format in [ModelFormat::Diffusers, ModelFormat::Mlx] {
        with_servers("image-options", |fixture, servers| {
            fixture.write_model_format_capabilities(format, vec![ModelCapability::ImageGeneration]);
            for lazy in [
                LifecycleInput::Omitted,
                LifecycleInput::Provided(None),
                Some(false).into(),
            ] {
                for capability in [None, Some(ServerCapability::ImageGeneration)] {
                    let mut input = request(fixture, fixture.model_ref.as_str());
                    input.target = ServerPrepareTarget::RuntimeRef {
                        runtime_ref: fixture.model_ref.to_string(),
                        capability,
                    };
                    input.lazy_load = lazy;
                    let error = servers.prepare_server(input).unwrap_err().to_string();
                    assert!(
                        error.contains("requires --lazy-load"),
                        "{format:?}: {error}"
                    );
                }
            }
            let mut input = request(fixture, fixture.model_ref.as_str());
            input.lazy_load = Some(true).into();
            let prepared = servers.prepare_server(input).unwrap();
            let mut legacy = prepared.outcome.inspection.spec;
            let selector = ServerRefSelector::parse(legacy.server_ref.as_str()).unwrap();
            servers
                .resolve_for_start(ServerResolveForStartRequest {
                    layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
                    selector: selector.clone(),
                    allow_unverified: true,
                })
                .unwrap();
            // Simulate an older readable eager spec; no load-mode validation on decode.
            legacy.lazy_load = false;
            let catalog = FileServerCatalogStore::new(StaticProcessProbe { running: false });
            catalog.save_server_spec(&prepared.store, &legacy).unwrap();
            let before = std::fs::read(prepared.outcome.inspection.spec_path).unwrap();
            for allow_unverified in [false, true] {
                let error = servers
                    .resolve_for_start(ServerResolveForStartRequest {
                        layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
                        selector: selector.clone(),
                        allow_unverified,
                    })
                    .unwrap_err()
                    .to_string();
                assert!(error.contains("requires --lazy-load"), "{error}");
            }
            let inspected = servers
                .inspect_server(ServerInspectRequest {
                    layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
                    selector: selector.clone(),
                })
                .unwrap()
                .inspection;
            assert_eq!(inspected.spec, legacy);
            assert_eq!(std::fs::read(inspected.spec_path).unwrap(), before);
            assert_eq!(
                servers
                    .list_servers(ServerListRequest {
                        layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
                        running_only: false,
                    })
                    .unwrap()
                    .servers
                    .len(),
                1
            );
            servers
                .remove_server(ServerRemoveRequest {
                    layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
                    selector,
                })
                .unwrap();
        });
    }
}

#[test]
fn local_null_and_explicit_defaults_keep_identity_and_idle_validation() {
    with_servers("local-defaults", |fixture, servers| {
        fixture.write_chat_model();
        let reference = fixture.model_ref.as_str();
        let first = servers.prepare_server(request(fixture, reference)).unwrap();
        for (lazy, runtime, model) in [
            (
                LifecycleInput::Provided(None),
                LifecycleInput::Provided(None),
                LifecycleInput::Provided(None),
            ),
            (Some(false).into(), Some(300).into(), Some(0).into()),
        ] {
            let mut input = request(fixture, reference);
            input.lazy_load = lazy;
            input.idle_seconds = runtime;
            input.model_idle_seconds = model;
            let next = servers.prepare_server(input).unwrap();
            assert!(!next.outcome.created);
            assert_eq!(
                next.outcome.inspection.spec.server_ref,
                first.outcome.inspection.spec.server_ref
            );
        }
        let mut input = request(fixture, reference);
        input.idle_seconds = Some(1).into();
        input.model_idle_seconds = Some(2).into();
        assert!(servers
            .prepare_server(input)
            .unwrap_err()
            .to_string()
            .contains("model_idle_seconds"));
        assert_eq!(LoadMode::from_lazy_load(false), LoadMode::Eager);
        assert_eq!(LoadMode::from_lazy_load(true), LoadMode::Lazy);
    });
}
