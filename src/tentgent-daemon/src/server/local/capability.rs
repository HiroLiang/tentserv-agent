use tentgent_kernel::features::{
    runtime::infra::{ModelRuntimeCapability, ModelRuntimeDaemonEndpoint},
    server::domain::ServerCapability,
};

use crate::provider_compat::ProviderCompatRejection;

use super::{error::LocalServerError, LocalServerState};

pub(super) async fn ensure_model_endpoint(
    state: &LocalServerState,
) -> Result<ModelRuntimeDaemonEndpoint, LocalServerError> {
    let capability = model_runtime_capability(state.config.capability);
    state
        .supervisor
        .ensure_model_bound_with_policy(
            &state.layout,
            &state.runtime,
            &state.executable_resolver,
            capability,
            &state.config.model_ref,
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
