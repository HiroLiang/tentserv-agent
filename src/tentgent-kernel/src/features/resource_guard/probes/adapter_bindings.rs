use walkdir::WalkDir;

use crate::{
    features::{
        adapter::domain::AdapterMetadata,
        model::domain::ModelCapability,
        resource_guard::{blocker, ResourceBlocker, ResourceBlockerCode, ResourceOperation},
    },
    foundation::{
        error::{KernelError, KernelResult},
        layout::RuntimeLayout,
    },
};

pub(crate) fn adapter_binding_blockers(
    layout: &RuntimeLayout,
    operation: &ResourceOperation,
) -> KernelResult<Vec<ResourceBlocker>> {
    let mut blockers = Vec::new();
    if !layout.adapters_dir.exists() {
        return Ok(blockers);
    }
    for entry in WalkDir::new(&layout.adapters_dir).into_iter() {
        let entry = entry.map_err(guard_error)?;
        if entry.file_name() != "adapter.toml" || !entry.file_type().is_file() {
            continue;
        }
        let metadata: AdapterMetadata =
            toml::from_str(&std::fs::read_to_string(entry.path()).map_err(guard_error)?)
                .map_err(guard_error)?;
        let Some(base_model_ref) = metadata.base_model_ref.as_ref() else {
            continue;
        };
        let capability = metadata.target_capability.unwrap_or(ModelCapability::Chat);
        let matched = match operation {
            ResourceOperation::DeleteModel { model_ref } => base_model_ref.as_str() == model_ref,
            ResourceOperation::RemoveModelCapability {
                model_ref,
                capability: removed,
            } => base_model_ref.as_str() == model_ref && capability == *removed,
            ResourceOperation::ReplaceModelCapabilities {
                model_ref,
                removed_capabilities,
            } => base_model_ref.as_str() == model_ref && removed_capabilities.contains(&capability),
            _ => false,
        };
        if matched {
            let mut value = blocker(
                operation,
                "adapter-binding",
                if matches!(operation, ResourceOperation::DeleteModel { .. }) {
                    ResourceBlockerCode::ModelInUse
                } else {
                    ResourceBlockerCode::CapabilityInUse
                },
                metadata.short_ref,
                "adapter is bound to this base model capability",
            );
            value.field = Some("base_model_ref".to_string());
            value.capability = Some(capability.to_string());
            blockers.push(value);
        }
    }
    Ok(blockers)
}

fn guard_error(error: impl std::fmt::Display) -> KernelError {
    KernelError::ResourceCoordinationUnavailable(format!(
        "resource guard adapter probe failed: {error}"
    ))
}
