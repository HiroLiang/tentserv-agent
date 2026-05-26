use crate::features::model::domain::{
    ModelCapability, ModelFormat, ModelRefSelector, ModelStoreLayout,
};
use crate::features::model::ports::ModelCatalogStore;
use crate::features::server::domain::{
    ensure_server_model_capability, infer_server_capability_from_model_capabilities,
    normalize_server_host, parse_server_runtime_selection, ServerCapability, ServerRef,
    ServerRuntimeKind, ServerRuntimeSelection, ServerRuntimeTarget, ServerSpec, ServerStoreLayout,
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
    capability: Option<ServerCapability>,
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
            let capability = resolve_local_server_capability(
                capability,
                &metadata.model_ref,
                &metadata.model_capabilities,
            )?;
            ensure_model_compatible_with_server(
                capability,
                &metadata.model_ref,
                &metadata.model_capabilities,
            )?;
            ensure_server_capability_implemented(capability)?;
            let backend = server_runtime_backend_for_format(capability, metadata.primary_format)?;

            Ok(ServerRuntimeTarget::LocalModel {
                model_ref: metadata.model_ref.clone(),
                backend,
                capability,
            })
        }
        ServerRuntimeSelection::CloudProvider {
            provider,
            provider_model,
        } => {
            let capability = capability.unwrap_or(ServerCapability::Chat);
            if capability != ServerCapability::Chat {
                return Err(KernelError::UnsupportedTarget(format!(
                    "cloud server capability `{capability}` is not implemented yet"
                )));
            }
            Ok(ServerRuntimeTarget::CloudProvider {
                provider,
                provider_model,
            })
        }
    }
}

fn resolve_local_server_capability(
    requested: Option<ServerCapability>,
    model_ref: &crate::features::model::domain::ModelRef,
    model_capabilities: &[ModelCapability],
) -> KernelResult<ServerCapability> {
    if let Some(capability) = requested {
        return Ok(capability);
    }
    infer_server_capability_from_model_capabilities(model_capabilities).ok_or_else(|| {
        KernelError::UnsupportedTarget(format!(
            "model `{model_ref}` does not advertise a server-compatible capability"
        ))
    })
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
            ensure_model_compatible_with_server(
                spec.capability,
                &model.metadata.model_ref,
                &model.metadata.model_capabilities,
            )?;
            ensure_server_capability_implemented(spec.capability)?;
            server_runtime_backend_for_format(spec.capability, model.metadata.primary_format)?;
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
        ServerRuntimeTarget::LocalModel {
            model_ref,
            capability,
            ..
        } => ServerSpec {
            server_ref,
            short_ref,
            runtime_kind: ServerRuntimeKind::Local,
            capability,
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
            capability: ServerCapability::Chat,
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

fn ensure_model_compatible_with_server(
    server_capability: ServerCapability,
    model_ref: &crate::features::model::domain::ModelRef,
    model_capabilities: &[ModelCapability],
) -> KernelResult<()> {
    ensure_server_model_capability(server_capability, model_ref, model_capabilities)
        .map_err(|err| KernelError::UnsupportedTarget(err.to_string()))
}

fn ensure_server_capability_implemented(capability: ServerCapability) -> KernelResult<()> {
    match capability {
        ServerCapability::AudioSpeech
        | ServerCapability::AudioTranscription
        | ServerCapability::Chat
        | ServerCapability::Embedding
        | ServerCapability::ImageGeneration
        | ServerCapability::Rerank
        | ServerCapability::VideoUnderstanding
        | ServerCapability::VisionChat => Ok(()),
    }
}

fn server_runtime_backend_for_format(
    capability: ServerCapability,
    format: ModelFormat,
) -> KernelResult<super::super::domain::ServerRuntimeBackend> {
    match capability {
        ServerCapability::Chat => match format {
            ModelFormat::Safetensors => {
                Ok(super::super::domain::ServerRuntimeBackend::TransformersPeft)
            }
            ModelFormat::Mlx => Ok(super::super::domain::ServerRuntimeBackend::Mlx),
            ModelFormat::Gguf => Ok(super::super::domain::ServerRuntimeBackend::LlamaCpp),
            ModelFormat::Diffusers => unsupported_server_format(capability, format),
        },
        ServerCapability::Embedding => match format {
            ModelFormat::Safetensors => {
                Ok(super::super::domain::ServerRuntimeBackend::TransformersPeft)
            }
            ModelFormat::Mlx => Ok(super::super::domain::ServerRuntimeBackend::Mlx),
            ModelFormat::Gguf => Ok(super::super::domain::ServerRuntimeBackend::LlamaCpp),
            ModelFormat::Diffusers => unsupported_server_format(capability, format),
        },
        ServerCapability::Rerank
        | ServerCapability::AudioSpeech
        | ServerCapability::AudioTranscription
        | ServerCapability::VideoUnderstanding
        | ServerCapability::VisionChat => match format {
            ModelFormat::Safetensors => {
                Ok(super::super::domain::ServerRuntimeBackend::TransformersPeft)
            }
            ModelFormat::Mlx => Ok(super::super::domain::ServerRuntimeBackend::Mlx),
            ModelFormat::Gguf | ModelFormat::Diffusers => {
                unsupported_server_format(capability, format)
            }
        },
        ServerCapability::ImageGeneration => match format {
            ModelFormat::Diffusers => Ok(super::super::domain::ServerRuntimeBackend::Diffusers),
            ModelFormat::Mlx => Ok(super::super::domain::ServerRuntimeBackend::Mlx),
            ModelFormat::Safetensors | ModelFormat::Gguf => {
                unsupported_server_format(capability, format)
            }
        },
    }
}

fn unsupported_server_format(
    capability: ServerCapability,
    format: ModelFormat,
) -> KernelResult<super::super::domain::ServerRuntimeBackend> {
    Err(KernelError::UnsupportedTarget(format!(
        "server capability `{capability}` does not support `{format}` model format yet"
    )))
}
