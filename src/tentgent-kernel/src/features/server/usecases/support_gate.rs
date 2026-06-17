//! Local server support-status gating.

use crate::features::model::{
    domain::{ModelMetadata, ModelStoreLayout},
    ports::ModelCapabilityProofStore,
    support_catalog::built_in_support_hints_for_model,
    support_status::{
        ModelSupportQuery, ModelSupportResolution, ModelSupportStatus, ModelSupportStatusResolver,
    },
};
use crate::features::server::domain::ServerCapability;
use crate::features::server::domain::ServerRuntimeProfileSelection;
use crate::foundation::{
    error::{KernelError, KernelResult},
    layout::RuntimeLayout,
};

pub(super) fn ensure_local_server_support_status_allows_start(
    metadata: &ModelMetadata,
    capability: ServerCapability,
    layout: &RuntimeLayout,
    proofs: &dyn ModelCapabilityProofStore,
    runtime_profile: Option<&ServerRuntimeProfileSelection>,
    allow_unverified: bool,
) -> KernelResult<()> {
    let model_store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
    let model_capability = capability.required_model_capability();
    let mut query = ModelSupportQuery::from_metadata(metadata, model_capability);
    if let Some(runtime_profile) = runtime_profile {
        query = query.with_runtime_profile(
            runtime_profile.profile_id.clone(),
            runtime_profile.profile_version,
        );
    }
    let stored_proofs = proofs.list_capability_proofs(&model_store, &metadata.model_ref)?;
    let hints = built_in_support_hints_for_model(metadata);
    let resolution = ModelSupportStatusResolver.resolve(metadata, &query, &stored_proofs, &hints);

    if local_server_support_status_allows_start(resolution.status, allow_unverified) {
        return Ok(());
    }

    Err(KernelError::UnsupportedTarget(
        local_server_support_gate_message(metadata, capability, &resolution, allow_unverified),
    ))
}

fn local_server_support_status_allows_start(
    status: ModelSupportStatus,
    allow_unverified: bool,
) -> bool {
    matches!(
        status,
        ModelSupportStatus::Verified | ModelSupportStatus::Supported
    ) || (allow_unverified
        && matches!(
            status,
            ModelSupportStatus::Unknown | ModelSupportStatus::Stale
        ))
}

fn local_server_support_gate_message(
    metadata: &ModelMetadata,
    capability: ServerCapability,
    resolution: &ModelSupportResolution,
    allow_unverified: bool,
) -> String {
    let mut parts = vec![
        format!(
            "local server start blocked for model `{}` capability `{}`",
            metadata.short_ref, capability
        ),
        format!("support status `{}`", resolution.status),
        format!("evidence `{}`", resolution.evidence),
        format!("reason: {}", resolution.reason),
    ];

    if let Some(stale_reason) = resolution.stale_reason.as_deref() {
        parts.push(format!("stale reason: {stale_reason}"));
    }
    if let Some(failure_reason) = resolution.failure_reason.as_deref() {
        parts.push(format!("failure: {failure_reason}"));
    }
    parts.push(format!(
        "next action: {}",
        local_server_support_next_action(resolution, allow_unverified)
    ));

    parts.join("; ")
}

fn local_server_support_next_action(
    resolution: &ModelSupportResolution,
    allow_unverified: bool,
) -> &'static str {
    match resolution.status {
        ModelSupportStatus::Verified | ModelSupportStatus::Supported => {
            "retry after resolving the non-support startup error"
        }
        ModelSupportStatus::Failed => {
            "inspect the failed proof, fix the runtime issue, clear the failed proof, then retry"
        }
        ModelSupportStatus::Unsupported => "choose a different model, capability, or backend",
        ModelSupportStatus::Unknown if allow_unverified => {
            "unknown was allowed; retry after resolving the non-support startup error"
        }
        ModelSupportStatus::Unknown => {
            "run with --allow-unverified or record a support proof before starting"
        }
        ModelSupportStatus::Stale if allow_unverified => {
            "stale proof was allowed; retry after resolving the non-support startup error"
        }
        ModelSupportStatus::Stale => {
            "rerun verification or use --allow-unverified to retry this stale tuple"
        }
    }
}
