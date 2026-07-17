use std::collections::BTreeSet;

use crate::{
    features::{
        resource_coordination::{ResourceKey, ResourceLockMode, ResourceLockRequest},
        runtime_ownership::{
            OwnershipOperationRecord, OwnershipProcessProbe, RouteGenerationClaim,
            RuntimeGenerationHealth, RuntimeGenerationHealthProbe, RuntimeGenerationRecord,
            RuntimeGenerationState, RuntimeOwnershipInspection, RuntimeOwnershipIssue,
            RuntimeOwnershipLayout, RuntimeOwnershipStatus, RuntimeOwnershipSummary,
        },
        server::{domain::ServerProcessIdentityStatus, ports::ServerProcessIdentityProbe},
    },
    foundation::{
        error::{KernelError, KernelResult},
        layout::RuntimeLayout,
    },
};

use super::StdRuntimeOwnershipUseCase;

impl StdRuntimeOwnershipUseCase {
    pub fn inspect_runtime_ownership(
        &self,
        layout: &RuntimeLayout,
    ) -> KernelResult<RuntimeOwnershipInspection> {
        self.inspect_runtime_ownership_with_health(layout, true)
    }

    pub fn summarize_runtime_ownership(
        &self,
        layout: &RuntimeLayout,
    ) -> KernelResult<RuntimeOwnershipInspection> {
        self.inspect_runtime_ownership_with_health(layout, false)
    }

    pub(crate) fn inspect_runtime_ownership_during_maintenance(
        &self,
        layout: &RuntimeLayout,
    ) -> KernelResult<RuntimeOwnershipInspection> {
        self.build_runtime_ownership_inspection(layout, true, false)
    }

    fn inspect_runtime_ownership_with_health(
        &self,
        layout: &RuntimeLayout,
        include_health: bool,
    ) -> KernelResult<RuntimeOwnershipInspection> {
        self.build_runtime_ownership_inspection(layout, include_health, include_health)
    }

    fn build_runtime_ownership_inspection(
        &self,
        layout: &RuntimeLayout,
        include_health: bool,
        lock_snapshot: bool,
    ) -> KernelResult<RuntimeOwnershipInspection> {
        let ownership = RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir);
        let (claims, generations, operations, mut issues) = if lock_snapshot {
            self.read_focused_snapshot(layout, &ownership)?
        } else {
            self.read_unlocked_snapshot(&ownership)?
        };
        append_stale_claims(
            self.process_probe.as_ref(),
            include_health.then_some(self.route_owner_probe.as_ref()),
            layout,
            &claims,
            &mut issues,
        )?;
        append_stale_generations(
            self.process_probe.as_ref(),
            include_health.then_some(self.health_probe.as_ref()),
            &generations,
            &mut issues,
        )?;
        append_stale_operations(self.process_probe.as_ref(), &operations, &mut issues)?;
        let summary = runtime_ownership_summary(&claims, &generations, &operations, &issues);
        Ok(RuntimeOwnershipInspection {
            claims,
            generations,
            operations,
            issues,
            summary,
        })
    }

    fn read_focused_snapshot(
        &self,
        layout: &RuntimeLayout,
        ownership: &RuntimeOwnershipLayout,
    ) -> KernelResult<OwnershipSnapshot> {
        if !ownership.root.exists() {
            return self.read_unlocked_snapshot(ownership);
        }
        for _ in 0..3 {
            let (discovered_claims, discovered_generations, _, _) =
                self.read_unlocked_snapshot(ownership)?;
            let mut keys = BTreeSet::from([ResourceKey::maintenance()]);
            keys.extend(
                discovered_claims
                    .iter()
                    .map(RouteGenerationClaim::claim_key),
            );
            keys.extend(
                discovered_generations
                    .iter()
                    .map(|generation| generation.identity.physical_key()),
            );
            let locks = keys
                .iter()
                .cloned()
                .map(|key| (key, ResourceLockMode::Shared))
                .collect();
            let permit = self
                .coordinator
                .acquire(
                    layout,
                    ResourceLockRequest::new("inspect-runtime-ownership", locks),
                )?
                .map_err(|busy| {
                    KernelError::RuntimeOwnershipUnavailable(format!(
                        "{}; retry after {} ms",
                        busy.description, busy.retry_after_millis
                    ))
                })?;
            let snapshot = self.read_unlocked_snapshot(ownership)?;
            let fully_locked = snapshot
                .0
                .iter()
                .map(RouteGenerationClaim::claim_key)
                .chain(
                    snapshot
                        .1
                        .iter()
                        .map(|generation| generation.identity.physical_key()),
                )
                .all(|key| keys.contains(&key));
            drop(permit);
            if fully_locked {
                return Ok(snapshot);
            }
        }
        Err(KernelError::RuntimeOwnershipUnavailable(
            "runtime ownership changed repeatedly while taking an inspection snapshot; retry"
                .to_string(),
        ))
    }

    fn read_unlocked_snapshot(
        &self,
        ownership: &RuntimeOwnershipLayout,
    ) -> KernelResult<OwnershipSnapshot> {
        let (claims, mut issues) = self.store.list_claims(ownership)?;
        let (generations, generation_issues) = self.store.list_generations(ownership)?;
        issues.extend(generation_issues);
        let (operations, operation_issues) = self.store.list_operations(ownership)?;
        issues.extend(operation_issues);
        Ok((claims, generations, operations, issues))
    }
}

type OwnershipSnapshot = (
    Vec<RouteGenerationClaim>,
    Vec<RuntimeGenerationRecord>,
    Vec<OwnershipOperationRecord>,
    Vec<RuntimeOwnershipIssue>,
);

pub fn runtime_ownership_summary(
    claims: &[RouteGenerationClaim],
    generations: &[RuntimeGenerationRecord],
    operations: &[OwnershipOperationRecord],
    issues: &[RuntimeOwnershipIssue],
) -> RuntimeOwnershipSummary {
    let malformed_record_count = issues.iter().filter(|issue| !issue.recoverable).count();
    let stale_record_count = issues.iter().filter(|issue| issue.recoverable).count();
    let status = if malformed_record_count > 0 {
        RuntimeOwnershipStatus::Blocked
    } else if stale_record_count > 0 {
        RuntimeOwnershipStatus::Attention
    } else {
        RuntimeOwnershipStatus::Healthy
    };
    RuntimeOwnershipSummary {
        route_claim_count: claims.len(),
        active_generation_count: generations
            .iter()
            .filter(|record| {
                matches!(
                    record.state,
                    RuntimeGenerationState::Starting | RuntimeGenerationState::Ready
                )
            })
            .count(),
        active_operation_count: operations.len(),
        stale_record_count,
        malformed_record_count,
        status,
    }
}

fn append_stale_operations(
    process_probe: &dyn OwnershipProcessProbe,
    operations: &[OwnershipOperationRecord],
    issues: &mut Vec<RuntimeOwnershipIssue>,
) -> KernelResult<()> {
    for operation in operations {
        if !process_probe.is_process_running(operation.pid)? {
            issues.push(RuntimeOwnershipIssue {
                kind: "ownership-operation".to_string(),
                record: operation.operation_id.clone(),
                description: format!(
                    "ownership operation `{}` was left by process {}; run runtime reconcile --apply",
                    operation.operation, operation.pid
                ),
                recoverable: true,
            });
        }
    }
    Ok(())
}

fn append_stale_claims(
    process_probe: &dyn OwnershipProcessProbe,
    identity_probe: Option<&dyn ServerProcessIdentityProbe>,
    layout: &RuntimeLayout,
    claims: &[RouteGenerationClaim],
    issues: &mut Vec<RuntimeOwnershipIssue>,
) -> KernelResult<()> {
    for claim in claims {
        let running = process_probe.is_process_running(claim.process.pid)?;
        let status = match identity_probe {
            Some(probe) => Some(probe.probe_process_identity(
                layout,
                &claim.server_ref,
                claim.process.pid,
                &claim.process.token,
            )?),
            None => None,
        };
        if !running || status == Some(ServerProcessIdentityStatus::Stopped) {
            issues.push(RuntimeOwnershipIssue {
                kind: "route-claim".to_string(),
                record: claim.owner_id.clone(),
                description: format!(
                    "route claim owner process {} is no longer running; stop/remove the owning server if present, then run runtime reconcile --apply",
                    claim.process.pid
                ),
                recoverable: true,
            });
        } else if let Some(
            ServerProcessIdentityStatus::Mismatch { description }
            | ServerProcessIdentityStatus::Unavailable { description },
        ) = status
        {
            issues.push(RuntimeOwnershipIssue {
                kind: "route-claim".to_string(),
                record: claim.owner_id.clone(),
                description: format!(
                    "route claim owner process {} is live but unverifiable: {description}; stop the owning server before recovery",
                    claim.process.pid
                ),
                recoverable: false,
            });
        }
    }
    Ok(())
}

fn append_stale_generations(
    process_probe: &dyn OwnershipProcessProbe,
    health_probe: Option<&dyn RuntimeGenerationHealthProbe>,
    generations: &[RuntimeGenerationRecord],
    issues: &mut Vec<RuntimeOwnershipIssue>,
) -> KernelResult<()> {
    for record in generations {
        let health = match health_probe {
            Some(probe) if record.endpoint.is_some() || record.launch_target.is_some() => {
                Some(probe.probe_generation_health(record)?)
            }
            _ => None,
        };
        if matches!(&health, Some(RuntimeGenerationHealth::Matching { .. })) {
            continue;
        }
        let pid = record
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.pid)
            .unwrap_or(record.launcher.pid);
        if !process_probe.is_process_running(pid)? {
            if record.endpoint.is_none() && record.launch_target.is_some() {
                let description = match health {
                    Some(RuntimeGenerationHealth::Mismatch { description })
                    | Some(RuntimeGenerationHealth::Unavailable { description }) => description,
                    _ => "worker identity could not be verified".to_string(),
                };
                issues.push(RuntimeOwnershipIssue {
                    kind: "runtime-generation".to_string(),
                    record: record.runtime_key.clone(),
                    description: format!(
                        "runtime generation {} launcher exited and the prepared worker is unverifiable: {description}; confirm the endpoint is stopped before recovery",
                        record.generation_id
                    ),
                    recoverable: false,
                });
                continue;
            }
            issues.push(RuntimeOwnershipIssue {
                kind: "runtime-generation".to_string(),
                record: record.runtime_key.clone(),
                description: format!(
                    "runtime generation {} process {pid} is no longer running; run runtime reconcile --apply",
                    record.generation_id
                ),
                recoverable: true,
            });
            continue;
        }
        match health {
            None | Some(RuntimeGenerationHealth::Matching { .. }) => {}
            Some(RuntimeGenerationHealth::Mismatch { description })
            | Some(RuntimeGenerationHealth::Unavailable { description }) => {
                issues.push(RuntimeOwnershipIssue {
                    kind: "runtime-generation".to_string(),
                    record: record.runtime_key.clone(),
                    description: format!(
                        "runtime generation {} process is live but unverifiable: {description}; stop the runtime before recovery",
                        record.generation_id
                    ),
                    recoverable: false,
                });
            }
        }
    }
    Ok(())
}
