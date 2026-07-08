use std::fs;
use std::path::{Component, Path};

use crate::features::auth::domain::Provider;
use crate::features::cloud::domain::{
    CloudEndpointCapability, provider_capabilities, provider_supports,
};
use crate::features::cluster::domain::{
    CLUSTER_SCHEMA_VERSION, ClusterDefinition, ClusterRef, ClusterRouteKey, ClusterRouteTarget,
    ClusterStoreLayout, MAX_CLUSTER_DEFINITION_BYTES,
};
use crate::features::model::domain::{ModelFormat, ModelStoreLayout};
use crate::features::model::ports::ModelCatalogStore;
use crate::features::server::domain::{
    CloudProvider, ServerCapability, ServerRuntimeBackend, ensure_server_model_capability,
};
use crate::features::server::profile::local_server_runtime_profile_for;
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayout;

pub(super) fn cluster_store_layout(layout: &RuntimeLayout) -> ClusterStoreLayout {
    ClusterStoreLayout::from_home_dir(layout.home_dir.clone())
}

pub(super) fn model_store_layout(layout: &RuntimeLayout) -> ModelStoreLayout {
    ModelStoreLayout::from_models_dir(layout.models_dir.clone())
}

pub(super) fn read_cluster_definition_file(
    source_path: &Path,
    force_unsafe_source: bool,
) -> KernelResult<ClusterDefinition> {
    ensure_safe_definition_source(source_path, force_unsafe_source)?;
    let body = fs::read_to_string(source_path).map_err(|err| {
        KernelError::ServerStoreUnavailable(format!(
            "read cluster definition `{}` failed: {err}",
            source_path.display()
        ))
    })?;
    toml::from_str(&body).map_err(|err| {
        KernelError::UnsupportedTarget(format!(
            "parse cluster definition `{}` failed: {err}",
            source_path.display()
        ))
    })
}

pub(super) fn validate_cluster_definition(
    definition: ClusterDefinition,
    expected_cluster_ref: Option<&ClusterRef>,
    layout: &RuntimeLayout,
    model_catalog: &dyn ModelCatalogStore,
) -> KernelResult<ClusterDefinition> {
    if definition.schema_version != CLUSTER_SCHEMA_VERSION {
        return Err(KernelError::UnsupportedTarget(format!(
            "unsupported cluster schema_version `{}`; expected `{CLUSTER_SCHEMA_VERSION}`",
            definition.schema_version
        )));
    }
    if let Some(expected) = expected_cluster_ref {
        if &definition.cluster_ref != expected {
            return Err(KernelError::UnsupportedTarget(format!(
                "cluster_ref mismatch: path uses `{expected}` but definition contains `{}`",
                definition.cluster_ref
            )));
        }
    }
    if definition.routes.is_empty() {
        return Err(KernelError::UnsupportedTarget(
            "cluster definition must declare at least one route".to_string(),
        ));
    }

    let model_store = model_store_layout(layout);
    for (route, target) in &definition.routes {
        validate_route(route, target, &model_store, model_catalog)?;
    }

    Ok(definition)
}

fn validate_route(
    route: &ClusterRouteKey,
    target: &ClusterRouteTarget,
    model_store: &ModelStoreLayout,
    model_catalog: &dyn ModelCatalogStore,
) -> KernelResult<()> {
    match target {
        ClusterRouteTarget::LocalModel {
            model_ref,
            runtime_profile,
        } => {
            let metadata = model_catalog.load_model_metadata(model_store, model_ref)?;
            ensure_server_model_capability(
                route.server_capability(),
                &metadata.model_ref,
                &metadata.model_capabilities,
            )
            .map_err(|err| KernelError::UnsupportedTarget(err.to_string()))?;
            let backend = server_runtime_backend_for_format(
                route.server_capability(),
                metadata.primary_format,
            )?;
            if let Some(runtime_profile) = runtime_profile.as_ref() {
                let Some(expected) =
                    local_server_runtime_profile_for(route.server_capability(), backend)
                else {
                    return Err(KernelError::UnsupportedTarget(format!(
                        "cluster route `{route}` does not support runtime profile `{}` for backend `{backend}`",
                        runtime_profile.label()
                    )));
                };
                if expected.selection != *runtime_profile {
                    return Err(KernelError::UnsupportedTarget(format!(
                        "cluster route `{route}` expected runtime profile `{}`, got `{}`",
                        expected.selection.label(),
                        runtime_profile.label()
                    )));
                }
            }
            Ok(())
        }
        ClusterRouteTarget::Provider {
            provider,
            provider_model,
        } => {
            if provider_model.trim().is_empty() {
                return Err(KernelError::UnsupportedTarget(format!(
                    "cluster route `{route}` provider model must not be blank"
                )));
            }
            ensure_provider_supports_route(*provider, *route)
        }
    }
}

fn ensure_safe_definition_source(
    source_path: &Path,
    force_unsafe_source: bool,
) -> KernelResult<()> {
    let metadata = fs::symlink_metadata(source_path).map_err(|err| {
        KernelError::ServerStoreUnavailable(format!(
            "read cluster definition metadata `{}` failed: {err}",
            source_path.display()
        ))
    })?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        return Err(KernelError::UnsupportedTarget(format!(
            "cluster definition `{}` must not be a symlink",
            source_path.display()
        )));
    }
    if !file_type.is_file() {
        return Err(KernelError::UnsupportedTarget(format!(
            "cluster definition `{}` must be a regular file",
            source_path.display()
        )));
    }
    if metadata.len() > MAX_CLUSTER_DEFINITION_BYTES {
        return Err(KernelError::UnsupportedTarget(format!(
            "cluster definition `{}` is too large; limit is {} bytes",
            source_path.display(),
            MAX_CLUSTER_DEFINITION_BYTES
        )));
    }
    if !force_unsafe_source && is_obvious_secret_path(source_path) {
        return Err(KernelError::UnsupportedTarget(format!(
            "cluster definition `{}` is under an auth or secret-bearing path; move it to a non-secret location or pass --force to read it anyway",
            source_path.display()
        )));
    }
    Ok(())
}

fn is_obvious_secret_path(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(value) => {
            let value = value.to_string_lossy().to_ascii_lowercase();
            matches!(
                value.as_str(),
                ".ssh"
                    | ".gnupg"
                    | ".gpg"
                    | "keychains"
                    | "keychain"
                    | ".env"
                    | "auth.toml"
                    | "auth.env"
                    | "credentials"
                    | "credentials.json"
                    | "secrets"
                    | "secrets.toml"
                    | "secrets.json"
            )
        }
        _ => false,
    })
}

fn ensure_provider_supports_route(
    provider: CloudProvider,
    route: ClusterRouteKey,
) -> KernelResult<()> {
    let Some(capability) = cloud_endpoint_capability_for_route(route) else {
        return Err(KernelError::UnsupportedTarget(format!(
            "provider route `{route}` is not supported for cloud providers"
        )));
    };
    let auth_provider = auth_provider_for_cloud_provider(provider);
    if provider_supports(auth_provider, capability) {
        return Ok(());
    }

    let values = provider_capabilities(auth_provider)
        .iter()
        .map(|capability| capability.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Err(KernelError::UnsupportedTarget(format!(
        "provider `{provider}` does not support cluster route `{route}`; supported cloud capabilities are [{values}]"
    )))
}

pub(super) fn cloud_endpoint_capability_for_route(
    route: ClusterRouteKey,
) -> Option<CloudEndpointCapability> {
    match route.server_capability() {
        ServerCapability::Chat => Some(CloudEndpointCapability::Chat),
        ServerCapability::VisionChat => Some(CloudEndpointCapability::VisionChat),
        ServerCapability::Embedding => Some(CloudEndpointCapability::Embedding),
        ServerCapability::AudioTranscription | ServerCapability::Rerank => None,
        ServerCapability::AudioSpeech
        | ServerCapability::ImageGeneration
        | ServerCapability::VideoUnderstanding => None,
    }
}

pub(super) fn auth_provider_for_cloud_provider(provider: CloudProvider) -> Provider {
    match provider {
        CloudProvider::OpenAI => Provider::OpenAI,
        CloudProvider::Anthropic => Provider::Anthropic,
        CloudProvider::Gemini => Provider::Gemini,
    }
}

pub(super) fn server_runtime_backend_for_format(
    capability: ServerCapability,
    format: ModelFormat,
) -> KernelResult<ServerRuntimeBackend> {
    match capability {
        ServerCapability::Chat => match format {
            ModelFormat::Safetensors => Ok(ServerRuntimeBackend::TransformersPeft),
            ModelFormat::Mlx => Ok(ServerRuntimeBackend::Mlx),
            ModelFormat::Gguf => Ok(ServerRuntimeBackend::LlamaCpp),
            ModelFormat::Diffusers => unsupported_server_format(capability, format),
        },
        ServerCapability::Embedding => match format {
            ModelFormat::Safetensors => Ok(ServerRuntimeBackend::TransformersPeft),
            ModelFormat::Mlx => Ok(ServerRuntimeBackend::Mlx),
            ModelFormat::Gguf => Ok(ServerRuntimeBackend::LlamaCpp),
            ModelFormat::Diffusers => unsupported_server_format(capability, format),
        },
        ServerCapability::Rerank
        | ServerCapability::AudioTranscription
        | ServerCapability::VisionChat => match format {
            ModelFormat::Safetensors => Ok(ServerRuntimeBackend::TransformersPeft),
            ModelFormat::Mlx => Ok(ServerRuntimeBackend::Mlx),
            ModelFormat::Gguf | ModelFormat::Diffusers => {
                unsupported_server_format(capability, format)
            }
        },
        ServerCapability::AudioSpeech
        | ServerCapability::ImageGeneration
        | ServerCapability::VideoUnderstanding => unsupported_server_format(capability, format),
    }
}

fn unsupported_server_format(
    capability: ServerCapability,
    format: ModelFormat,
) -> KernelResult<ServerRuntimeBackend> {
    Err(KernelError::UnsupportedTarget(format!(
        "cluster route capability `{capability}` does not support `{format}` model format yet"
    )))
}
