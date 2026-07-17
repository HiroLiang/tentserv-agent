use tentgent_kernel::features::{
    runtime::infra::{ModelRuntimeBinding, ModelRuntimeCapability, ModelRuntimeDaemonEndpoint},
    server::domain::{ServerCapability, ServerRuntimeProfileSelection},
};

use crate::provider_compat::ProviderCompatRejection;

use super::{error::LocalServerError, LocalServerState};

pub(super) async fn ensure_model_endpoint(
    state: &LocalServerState,
) -> Result<ModelRuntimeDaemonEndpoint, LocalServerError> {
    let capability = model_runtime_capability(state.config.capability);
    let runtime_profile = state
        .config
        .runtime_profile
        .as_deref()
        .map(ServerRuntimeProfileSelection::parse_label)
        .transpose()
        .map_err(LocalServerError::internal)?;
    state
        .supervisor
        .ensure_model_bound_with_profile_and_policy(
            &state.layout,
            &state.runtime,
            &state.executable_resolver,
            ModelRuntimeBinding {
                capability,
                model_ref: &state.config.model_ref,
                runtime_profile: runtime_profile.as_ref(),
            },
            &state.launch_policy,
        )
        .await
        .map_err(|err| LocalServerError::internal(err.to_string()))
}

pub(super) fn ensure_local_provider_capability(
    actual: ServerCapability,
    required: ServerCapability,
    route: &str,
) -> Result<(), LocalServerError> {
    if actual == required {
        return Ok(());
    }
    Err(ProviderCompatRejection::unsupported_capability(format!(
        "{route} requires a {} local server; this server is bound to {}",
        required.as_str(),
        actual.as_str()
    ))
    .into())
}

pub(super) fn model_runtime_capability(capability: ServerCapability) -> ModelRuntimeCapability {
    match capability {
        ServerCapability::AudioSpeech => ModelRuntimeCapability::AudioSpeech,
        ServerCapability::AudioTranscription => ModelRuntimeCapability::AudioTranscription,
        ServerCapability::Chat => ModelRuntimeCapability::Chat,
        ServerCapability::Embedding => ModelRuntimeCapability::Embedding,
        ServerCapability::ImageGeneration => ModelRuntimeCapability::ImageGeneration,
        ServerCapability::Rerank => ModelRuntimeCapability::Rerank,
        ServerCapability::VideoUnderstanding => ModelRuntimeCapability::VideoUnderstanding,
        ServerCapability::VisionChat => ModelRuntimeCapability::VisionChat,
    }
}
