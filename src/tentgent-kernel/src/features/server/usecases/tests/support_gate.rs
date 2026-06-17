use super::*;

#[test]
fn standard_server_usecase_allows_verified_local_support_status() {
    let fixture = Fixture::new("gate-verified");
    fixture.write_model_capabilities(vec![ModelCapability::Chat]);
    fixture.write_capability_proof(
        ModelCapability::Chat,
        ModelCapabilityProofStatus::Verified,
        "safetensors",
        None,
    );
    let deps = ServerUseCaseFixture::new(StaticProcessProbe { running: false });
    let servers = deps.usecase();

    let prepared = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: fixture.model_ref.short_ref().to_string(),
            capability: Some(ServerCapability::Chat),
            host: None,
            port: Some(8790),
            lazy_load: false,
            idle_seconds: None,
            allow_unverified: false,
        })
        .expect("verified support should allow local server preparation");

    assert!(prepared.outcome.created);
}

#[test]
fn standard_server_usecase_allows_catalog_supported_local_status() {
    let fixture = Fixture::new("gate-supported");
    fixture.write_hf_model_format_capabilities(
        ModelFormat::Safetensors,
        vec![ModelCapability::Chat],
        "Qwen/Qwen2.5-0.5B-Instruct",
    );
    let deps = ServerUseCaseFixture::new(StaticProcessProbe { running: false });
    let servers = deps.usecase();

    let prepared = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: fixture.model_ref.short_ref().to_string(),
            capability: Some(ServerCapability::Chat),
            host: None,
            port: Some(8791),
            lazy_load: false,
            idle_seconds: None,
            allow_unverified: false,
        })
        .expect("supported catalog hint should allow local server preparation");

    assert!(prepared.outcome.created);
}

#[test]
fn standard_server_usecase_rejects_failed_local_support_status() {
    let fixture = Fixture::new("gate-failed");
    fixture.write_model_capabilities(vec![ModelCapability::Chat]);
    fixture.write_capability_proof(
        ModelCapability::Chat,
        ModelCapabilityProofStatus::Failed,
        "safetensors",
        Some("runtime failed"),
    );
    let deps = ServerUseCaseFixture::new(StaticProcessProbe { running: false });
    let servers = deps.usecase();

    let err = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: fixture.model_ref.short_ref().to_string(),
            capability: Some(ServerCapability::Chat),
            host: None,
            port: Some(8792),
            lazy_load: false,
            idle_seconds: None,
            allow_unverified: true,
        })
        .expect_err("failed support status should block local server preparation");

    let message = err.to_string();
    assert!(message.contains("support status `failed`"));
    assert!(message.contains("runtime failed"));
}

#[test]
fn standard_server_usecase_rejects_unknown_support_without_override() {
    let fixture = Fixture::new("gate-unknown");
    fixture.write_model_capabilities(vec![ModelCapability::Chat]);
    let deps = ServerUseCaseFixture::new(StaticProcessProbe { running: false });
    let servers = deps.usecase();

    let err = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: fixture.model_ref.short_ref().to_string(),
            capability: Some(ServerCapability::Chat),
            host: None,
            port: Some(8793),
            lazy_load: false,
            idle_seconds: None,
            allow_unverified: false,
        })
        .expect_err("unknown support status should block by default");

    let message = err.to_string();
    assert!(message.contains("support status `unknown`"));
    assert!(message.contains("--allow-unverified"));
}

#[test]
fn standard_server_usecase_allows_unknown_support_with_override() {
    let fixture = Fixture::new("gate-unknown-allowed");
    fixture.write_model_capabilities(vec![ModelCapability::Chat]);
    let deps = ServerUseCaseFixture::new(StaticProcessProbe { running: false });
    let servers = deps.usecase();

    let prepared = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: fixture.model_ref.short_ref().to_string(),
            capability: Some(ServerCapability::Chat),
            host: None,
            port: Some(8794),
            lazy_load: false,
            idle_seconds: None,
            allow_unverified: true,
        })
        .expect("unknown support should be allowed with explicit override");

    assert!(prepared.outcome.created);
}

#[test]
fn standard_server_usecase_rechecks_existing_specs_on_start() {
    let fixture = Fixture::new("gate-start");
    fixture.write_model_capabilities(vec![ModelCapability::Chat]);
    let deps = ServerUseCaseFixture::new(StaticProcessProbe { running: false });
    let servers = deps.usecase();
    let prepared = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: fixture.model_ref.short_ref().to_string(),
            capability: Some(ServerCapability::Chat),
            host: None,
            port: Some(8795),
            lazy_load: false,
            idle_seconds: None,
            allow_unverified: true,
        })
        .expect("prepare with override");

    let selector = ServerRefSelector::parse(prepared.outcome.inspection.spec.short_ref.clone())
        .expect("selector");
    let err = servers
        .resolve_for_start(ServerResolveForStartRequest {
            layout: fixture.layout_input(LayoutResolveMode::ReadOnly),
            selector,
            allow_unverified: false,
        })
        .expect_err("start should re-check support gate without override");

    assert!(err.to_string().contains("support status `unknown`"));
}

#[test]
fn standard_server_usecase_rejects_stale_support_without_override() {
    let fixture = Fixture::new("gate-stale");
    fixture.write_model_capabilities(vec![ModelCapability::Chat]);
    fixture.write_capability_proof(
        ModelCapability::Chat,
        ModelCapabilityProofStatus::Verified,
        "old-backend",
        None,
    );
    let deps = ServerUseCaseFixture::new(StaticProcessProbe { running: false });
    let servers = deps.usecase();

    let err = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: fixture.model_ref.short_ref().to_string(),
            capability: Some(ServerCapability::Chat),
            host: None,
            port: Some(8796),
            lazy_load: false,
            idle_seconds: None,
            allow_unverified: false,
        })
        .expect_err("stale support status should block by default");

    let message = err.to_string();
    assert!(message.contains("support status `stale`"));
    assert!(message.contains("backend changed"));
}

#[test]
fn standard_server_usecase_allows_stale_support_with_override() {
    let fixture = Fixture::new("gate-stale-allowed");
    fixture.write_model_capabilities(vec![ModelCapability::Chat]);
    fixture.write_capability_proof(
        ModelCapability::Chat,
        ModelCapabilityProofStatus::Verified,
        "old-backend",
        None,
    );
    let deps = ServerUseCaseFixture::new(StaticProcessProbe { running: false });
    let servers = deps.usecase();

    let prepared = servers
        .prepare_server(ServerPrepareRequest {
            layout: fixture.layout_input(LayoutResolveMode::Create),
            runtime_ref: fixture.model_ref.short_ref().to_string(),
            capability: Some(ServerCapability::Chat),
            host: None,
            port: Some(8797),
            lazy_load: false,
            idle_seconds: None,
            allow_unverified: true,
        })
        .expect("stale support should be allowed with explicit override");

    assert!(prepared.outcome.created);
}

struct ServerUseCaseFixture {
    layout_resolver: StdRuntimeLayoutResolver,
    initializer: StdServerStoreLayoutInitializer,
    model_catalog: FileModelCatalogStore,
    model_proofs: FileModelCapabilityProofStore,
    identity: StdServerIdentityGenerator,
    catalog: FileServerCatalogStore<StaticProcessProbe>,
    controller: StaticProcessController,
    clock: StaticClock,
}

impl ServerUseCaseFixture {
    fn new(process_probe: StaticProcessProbe) -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            initializer: StdServerStoreLayoutInitializer,
            model_catalog: FileModelCatalogStore,
            model_proofs: FileModelCapabilityProofStore,
            identity: StdServerIdentityGenerator,
            catalog: FileServerCatalogStore::new(process_probe),
            controller: StaticProcessController,
            clock: StaticClock,
        }
    }

    fn usecase(&self) -> StdServerUseCase<'_> {
        StdServerUseCase::new(
            &self.layout_resolver,
            &self.initializer,
            &self.model_catalog,
            &self.model_proofs,
            &self.identity,
            &self.catalog,
            &self.controller,
            &self.clock,
        )
    }
}
