use std::fs;

use serde::Deserialize;

use crate::{
    features::{
        resource_guard::{blocker, ResourceBlocker, ResourceBlockerCode, ResourceOperation},
        server::domain::ServerCapability,
    },
    foundation::{
        error::{KernelError, KernelResult},
        layout::RuntimeLayout,
    },
};

#[derive(Debug, Deserialize)]
struct ServerRefs {
    #[serde(default)]
    server_ref: String,
    short_ref: String,
    #[serde(default)]
    capability: Option<ServerCapability>,
    #[serde(default)]
    model_ref: Option<String>,
    #[serde(default)]
    cluster_ref: Option<String>,
    #[serde(default)]
    adapter_ref: Option<String>,
    #[serde(default)]
    default_adapter_ref: Option<String>,
    #[serde(default)]
    allowed_adapters: Vec<String>,
    #[serde(default)]
    adapter_refs: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ProcessRecord {
    pid: u32,
}

pub(crate) fn server_spec_blockers(
    context: &super::ResourceGuardProbeContext<'_>,
    layout: &RuntimeLayout,
    operation: &ResourceOperation,
) -> KernelResult<Vec<ResourceBlocker>> {
    let mut blockers = Vec::new();
    if !layout.servers_dir.exists() {
        return Ok(blockers);
    }
    for entry in fs::read_dir(&layout.servers_dir).map_err(guard_error)? {
        let entry = entry.map_err(guard_error)?;
        if !entry.file_type().map_err(guard_error)?.is_dir() {
            continue;
        }
        let spec_path = entry.path().join("server.toml");
        if !spec_path.exists() {
            continue;
        }
        let spec: ServerRefs =
            toml::from_str(&fs::read_to_string(&spec_path).map_err(guard_error)?)
                .map_err(guard_error)?;
        match operation {
            ResourceOperation::DeleteModel { model_ref } => {
                if ref_matches(spec.model_ref.as_deref(), model_ref) {
                    let mut value = blocker(
                        operation,
                        "server-spec",
                        ResourceBlockerCode::ModelInUse,
                        &spec.short_ref,
                        "server spec uses this model",
                    );
                    value.field = Some("model_ref".to_string());
                    value.capability = spec.capability.map(|value| value.to_string());
                    blockers.push(value);
                }
            }
            ResourceOperation::RemoveModelCapability {
                model_ref,
                capability: _,
            }
            | ResourceOperation::ReplaceModelCapabilities {
                model_ref,
                removed_capabilities: _,
            } => {
                let removed = match operation {
                    ResourceOperation::RemoveModelCapability { capability, .. } => {
                        vec![*capability]
                    }
                    ResourceOperation::ReplaceModelCapabilities {
                        removed_capabilities,
                        ..
                    } => removed_capabilities.clone(),
                    _ => Vec::new(),
                };
                if ref_matches(spec.model_ref.as_deref(), model_ref)
                    && spec.capability.is_some_and(|capability| {
                        removed.contains(&capability.required_model_capability())
                    })
                {
                    let mut value = blocker(
                        operation,
                        "server-spec",
                        ResourceBlockerCode::CapabilityInUse,
                        &spec.short_ref,
                        "server spec uses a capability being removed",
                    );
                    value.field = Some("capability".to_string());
                    value.capability = spec.capability.map(|value| value.to_string());
                    blockers.push(value);
                }
            }
            ResourceOperation::DeleteAdapter { adapter_ref, .. }
            | ResourceOperation::RebindAdapter { adapter_ref, .. } => {
                for (field, reference) in adapter_fields(&spec) {
                    if reference == adapter_ref || adapter_ref.starts_with(reference) {
                        let mut value = blocker(
                            operation,
                            "server-spec",
                            if matches!(operation, ResourceOperation::DeleteAdapter { .. }) {
                                ResourceBlockerCode::AdapterInUse
                            } else {
                                ResourceBlockerCode::AdapterRebindInUse
                            },
                            &spec.short_ref,
                            "server spec references this adapter",
                        );
                        value.field = Some(field.to_string());
                        blockers.push(value);
                    }
                }
            }
            ResourceOperation::DeleteCluster { cluster_ref } => {
                if spec.cluster_ref.as_deref() == Some(cluster_ref) {
                    let mut value = blocker(
                        operation,
                        "server-spec",
                        ResourceBlockerCode::ClusterInUse,
                        &spec.short_ref,
                        "server spec targets this cluster",
                    );
                    value.field = Some("cluster_ref".to_string());
                    blockers.push(value);
                }
            }
            ResourceOperation::DeleteServerSpec { server_ref }
                if (!spec.server_ref.is_empty() && spec.server_ref == *server_ref)
                    || spec.short_ref == *server_ref =>
            {
                let process_path = entry.path().join("process.toml");
                if process_path.exists() {
                    let process: ProcessRecord =
                        toml::from_str(&fs::read_to_string(process_path).map_err(guard_error)?)
                            .map_err(guard_error)?;
                    if context.process_probe.is_process_running(process.pid)? {
                        let mut value = blocker(
                            operation,
                            "server-process",
                            ResourceBlockerCode::ServerRunning,
                            &spec.short_ref,
                            "server process is still running; stop it before removing the spec",
                        );
                        value.next_actions =
                            vec![format!("tentgent server stop {}", spec.short_ref)];
                        blockers.push(value);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(blockers)
}

fn ref_matches(value: Option<&str>, expected: &str) -> bool {
    value.is_some_and(|value| value == expected || expected.starts_with(value))
}

fn adapter_fields(spec: &ServerRefs) -> Vec<(&'static str, &str)> {
    let mut refs = Vec::new();
    if let Some(value) = spec.adapter_ref.as_deref() {
        refs.push(("adapter_ref", value));
    }
    if let Some(value) = spec.default_adapter_ref.as_deref() {
        refs.push(("default_adapter_ref", value));
    }
    refs.extend(
        spec.allowed_adapters
            .iter()
            .map(|value| ("allowed_adapters", value.as_str())),
    );
    refs.extend(
        spec.adapter_refs
            .iter()
            .map(|value| ("adapter_refs", value.as_str())),
    );
    refs
}

fn guard_error(error: impl std::fmt::Display) -> KernelError {
    KernelError::ResourceCoordinationUnavailable(format!(
        "resource guard server probe failed: {error}"
    ))
}
