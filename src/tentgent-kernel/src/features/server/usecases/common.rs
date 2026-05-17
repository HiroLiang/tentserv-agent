use crate::features::model::domain::{ModelCapability, ModelRefSelector, ModelStoreLayout};
use crate::features::model::ports::ModelCatalogStore;
use crate::features::server::domain::{
    normalize_server_host, parse_server_runtime_selection, ServerRef, ServerRuntimeKind,
    ServerRuntimeSelection, ServerRuntimeTarget, ServerSpec, ServerStoreLayout,
    DEFAULT_SERVER_PORT,
};
use crate::features::server::ports::ServerIdentityGenerator;
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayout;

pub(super) fn server_store_layout(layout: &RuntimeLayout) -> ServerStoreLayout {
    ServerStoreLayout::from_home_and_servers_dir(
        layout.home_dir.clone(),
        layout.servers_dir.clone(),
    )
}

pub(super) fn model_store_layout(layout: &RuntimeLayout) -> ModelStoreLayout {
    ModelStoreLayout::from_models_dir(layout.models_dir.clone())
}

pub(super) fn resolve_server_runtime_target(
    runtime_ref: &str,
    layout: &RuntimeLayout,
    model_catalog: &dyn ModelCatalogStore,
) -> KernelResult<ServerRuntimeTarget> {
    match parse_server_runtime_selection(runtime_ref)
        .map_err(|err| KernelError::UnsupportedTarget(err.to_string()))?
    {
        ServerRuntimeSelection::LocalModel { selector } => {
            let model_store = model_store_layout(layout);
            let model = model_catalog.inspect_model(&model_store, &selector)?;
            let metadata = &model.metadata;
            if !metadata.supports_capability(ModelCapability::Chat) {
                return Err(KernelError::UnsupportedTarget(format!(
                    "model `{}` does not advertise chat capability",
                    metadata.model_ref
                )));
            }

            Ok(ServerRuntimeTarget::LocalModel {
                model_ref: metadata.model_ref.clone(),
                backend: super::super::domain::ServerRuntimeBackend::from_model_format(
                    metadata.primary_format,
                ),
            })
        }
        ServerRuntimeSelection::CloudProvider {
            provider,
            provider_model,
        } => Ok(ServerRuntimeTarget::CloudProvider {
            provider,
            provider_model,
        }),
    }
}

pub(super) fn ensure_server_spec_launchable(
    spec: &ServerSpec,
    layout: &RuntimeLayout,
    model_catalog: &dyn ModelCatalogStore,
) -> KernelResult<()> {
    match spec.runtime_kind {
        ServerRuntimeKind::Cloud => Ok(()),
        ServerRuntimeKind::Local => {
            let Some(model_ref) = spec.model_ref.as_ref() else {
                return Err(KernelError::ServerStoreUnavailable(format!(
                    "local server spec `{}` is missing model_ref",
                    spec.short_ref
                )));
            };
            let model_store = model_store_layout(layout);
            let selector = ModelRefSelector::parse(model_ref.as_str())
                .map_err(|err| KernelError::ServerStoreUnavailable(err.to_string()))?;
            let model = model_catalog.inspect_model(&model_store, &selector)?;
            if !model.metadata.supports_capability(ModelCapability::Chat) {
                return Err(KernelError::UnsupportedTarget(format!(
                    "model `{}` does not advertise chat capability",
                    model.metadata.model_ref
                )));
            }
            Ok(())
        }
    }
}

pub(super) fn build_server_spec(
    target: ServerRuntimeTarget,
    host: Option<&str>,
    port: Option<u16>,
    lazy_load: bool,
    idle_seconds: Option<u64>,
    created_at: String,
    identity: &dyn ServerIdentityGenerator,
) -> KernelResult<ServerSpec> {
    let host = normalize_server_host(host)
        .map_err(|err| KernelError::UnsupportedTarget(err.to_string()))?;
    let port = port.unwrap_or(DEFAULT_SERVER_PORT);
    let server_ref =
        identity.server_ref_for_target(&target, &host, port, lazy_load, idle_seconds)?;
    Ok(spec_for_ref(
        server_ref,
        target,
        host,
        port,
        lazy_load,
        idle_seconds,
        created_at,
    ))
}

fn spec_for_ref(
    server_ref: ServerRef,
    target: ServerRuntimeTarget,
    host: String,
    port: u16,
    lazy_load: bool,
    idle_seconds: Option<u64>,
    created_at: String,
) -> ServerSpec {
    let short_ref = server_ref.short_ref().to_string();
    match target {
        ServerRuntimeTarget::LocalModel { model_ref, .. } => ServerSpec {
            server_ref,
            short_ref,
            runtime_kind: ServerRuntimeKind::Local,
            model_ref: Some(model_ref),
            provider: None,
            provider_model: None,
            host,
            port,
            lazy_load,
            idle_seconds,
            created_at,
        },
        ServerRuntimeTarget::CloudProvider {
            provider,
            provider_model,
        } => ServerSpec {
            server_ref,
            short_ref,
            runtime_kind: ServerRuntimeKind::Cloud,
            model_ref: None,
            provider: Some(provider),
            provider_model: Some(provider_model),
            host,
            port,
            lazy_load,
            idle_seconds,
            created_at,
        },
    }
}
