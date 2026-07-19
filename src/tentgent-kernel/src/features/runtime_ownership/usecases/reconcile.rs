use std::{path::Path, time::SystemTime};

use crate::{
    features::{
        resource_coordination::{ResourceKey, ResourceLockMode, ResourceLockRequest},
        runtime_ownership::{
            now_text, OwnershipProcessProbe, RuntimeGenerationHealth, RuntimeGenerationOperation,
            RuntimeGenerationState, RuntimeOwnershipLayout, RuntimeReconcileAction,
            RuntimeReconcileRequest, RuntimeReconcileResult,
        },
    },
    foundation::{
        error::{KernelError, KernelResult},
        layout::RuntimeLayout,
    },
};

use super::StdRuntimeOwnershipUseCase;

impl StdRuntimeOwnershipUseCase {
    pub fn reconcile_runtime_ownership(
        &self,
        layout: &RuntimeLayout,
        request: RuntimeReconcileRequest,
    ) -> KernelResult<RuntimeReconcileResult> {
        if request.purge_quarantine && !request.apply {
            return Err(KernelError::RuntimeOwnershipUnavailable(
                "--purge-quarantine requires --apply".to_string(),
            ));
        }
        let cutoff = reconcile_invocation_cutoff();
        let before_inspection = self.inspect_runtime_ownership(layout)?;
        let before = before_inspection.summary.clone();
        let mut actions = Vec::new();

        let _maintenance = if request.apply {
            match self.coordinator.acquire(
                layout,
                ResourceLockRequest::new(
                    "reconcile-runtime-ownership",
                    vec![(ResourceKey::maintenance(), ResourceLockMode::Exclusive)],
                ),
            )? {
                Ok(permit) => Some(permit),
                Err(busy) => {
                    return Err(KernelError::RuntimeOwnershipUnavailable(format!(
                        "{} ({})",
                        busy.description,
                        busy.key.label()
                    )))
                }
            }
        } else {
            None
        };

        let current = if request.apply {
            self.inspect_runtime_ownership_during_maintenance(layout)?
        } else {
            self.inspect_runtime_ownership(layout)?
        };
        for generation in current.generations.iter().filter(|generation| {
            generation.state == RuntimeGenerationState::Starting
                && (generation.endpoint.is_some() || generation.launch_target.is_some())
        }) {
            let RuntimeGenerationHealth::Matching { status, endpoint } =
                self.health_probe.probe_generation_health(generation)?
            else {
                continue;
            };
            if status == "closing" {
                continue;
            }
            if request.apply {
                let ownership = RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir);
                let mut adopted = generation.clone();
                adopted.state = RuntimeGenerationState::Ready;
                adopted.operation = RuntimeGenerationOperation::Reconcile;
                adopted.endpoint = Some(endpoint);
                adopted.diagnostic = None;
                adopted.updated_at = now_text();
                self.store.write_generation(&ownership, &adopted)?;
            }
            actions.push(RuntimeReconcileAction {
                kind: "runtime-generation".to_string(),
                record: generation.runtime_key.clone(),
                action: "adopt-matching-generation".to_string(),
                applied: request.apply,
                description: format!(
                    "runtime generation {} has a matching live worker and can be adopted",
                    generation.generation_id
                ),
            });
        }
        for issue in current.issues.iter().filter(|issue| issue.recoverable) {
            let action = match issue.kind.as_str() {
                "route-claim" => {
                    if request.apply {
                        let ownership =
                            RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir);
                        self.store.remove_claim(&ownership, &issue.record)?;
                    }
                    "remove-stale-claim"
                }
                "runtime-generation" => {
                    if request.apply {
                        let ownership =
                            RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir);
                        self.store.remove_generation(&ownership, &issue.record)?;
                    }
                    "remove-stale-generation"
                }
                "ownership-operation" => {
                    if request.apply {
                        let ownership =
                            RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir);
                        self.store.remove_operation(&ownership, &issue.record)?;
                    }
                    "remove-stale-operation"
                }
                _ => continue,
            };
            actions.push(RuntimeReconcileAction {
                kind: issue.kind.clone(),
                record: issue.record.clone(),
                action: action.to_string(),
                applied: request.apply,
                description: issue.description.clone(),
            });
        }

        let ownership = RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir);
        let malformed = current
            .issues
            .iter()
            .filter_map(|issue| malformed_record_path(issue, &ownership).map(|path| (issue, path)))
            .collect::<Vec<_>>();
        for (issue, path) in malformed {
            let can_quarantine =
                malformed_record_is_proven_inactive(path, self.process_probe.as_ref())?;
            if request.apply && can_quarantine {
                let quarantined = self.store.quarantine_record(&ownership, path)?;
                actions.push(RuntimeReconcileAction {
                    kind: issue.kind.clone(),
                    record: issue.record.clone(),
                    action: "quarantine-malformed-record".to_string(),
                    applied: true,
                    description: format!("quarantined as {quarantined}"),
                });
            } else {
                actions.push(RuntimeReconcileAction {
                    kind: issue.kind.clone(),
                    record: issue.record.clone(),
                    action: "quarantine-malformed-record".to_string(),
                    applied: false,
                    description: if can_quarantine {
                        issue.description.clone()
                    } else {
                        format!(
                            "{}; live-process exclusion could not be proven, so quarantine is blocked",
                            issue.description
                        )
                    },
                });
            }
        }

        if request.purge_quarantine {
            let ownership = RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir);
            for record in self
                .store
                .purge_quarantine_before(&ownership, &cutoff.to_string())?
            {
                actions.push(RuntimeReconcileAction {
                    kind: "quarantine".to_string(),
                    record,
                    action: "purge-quarantine".to_string(),
                    applied: true,
                    description: "removed a record quarantined before this invocation".to_string(),
                });
            }
        }
        drop(_maintenance);
        let after = self.inspect_runtime_ownership(layout)?.summary;
        Ok(RuntimeReconcileResult {
            before,
            after,
            actions,
        })
    }
}

fn malformed_record_path<'a>(
    issue: &'a crate::features::runtime_ownership::RuntimeOwnershipIssue,
    ownership: &RuntimeOwnershipLayout,
) -> Option<&'a Path> {
    if issue.recoverable
        || !(issue.description.starts_with("malformed ownership record")
            || issue.description.starts_with("unreadable ownership record"))
    {
        return None;
    }
    let path = Path::new(&issue.record);
    let parent = path.parent()?;
    [
        ownership.claims.as_path(),
        ownership.generations.as_path(),
        ownership.operations.as_path(),
    ]
    .contains(&parent)
    .then_some(path)
}

fn malformed_record_is_proven_inactive(
    path: &Path,
    process_probe: &dyn OwnershipProcessProbe,
) -> KernelResult<bool> {
    let body = std::fs::read_to_string(path).map_err(|error| {
        KernelError::RuntimeOwnershipUnavailable(format!(
            "read malformed ownership record `{}` failed: {error}",
            path.display()
        ))
    })?;
    let value: toml::Value = match toml::from_str(&body) {
        Ok(value) => value,
        Err(_) => return Ok(false),
    };
    let mut pids = Vec::new();
    collect_pids(&value, None, &mut pids);
    if pids.is_empty() {
        return Ok(false);
    }
    for pid in pids {
        if process_probe.is_process_running(pid)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn collect_pids(value: &toml::Value, key: Option<&str>, pids: &mut Vec<u32>) {
    match value {
        toml::Value::Integer(pid)
            if matches!(key, Some("pid")) && *pid > 0 && *pid <= i64::from(u32::MAX) =>
        {
            pids.push(*pid as u32);
        }
        toml::Value::Array(values) => {
            for value in values {
                collect_pids(value, key, pids);
            }
        }
        toml::Value::Table(values) => {
            for (key, value) in values {
                collect_pids(value, Some(key), pids);
            }
        }
        _ => {}
    }
}

pub fn reconcile_invocation_cutoff() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
