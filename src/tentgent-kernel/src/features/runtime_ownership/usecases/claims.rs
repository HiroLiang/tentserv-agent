use crate::{
    features::{
        resource_coordination::{ResourceBusy, ResourceLockMode, ResourceLockRequest},
        runtime_ownership::{
            now_text, RouteClaimOperation, RouteClaimState, RouteGenerationClaim,
            RuntimeOwnershipLayout,
        },
        server::domain::ServerProcessIdentityStatus,
    },
    foundation::{error::KernelResult, layout::RuntimeLayout},
};

use super::StdRuntimeOwnershipUseCase;

#[derive(Debug, Clone)]
pub struct RouteClaimAcquireRequest {
    pub claim: RouteGenerationClaim,
}

#[derive(Debug)]
pub enum RouteClaimTransition {
    Acquired(RouteGenerationClaim),
    Existing(RouteGenerationClaim),
    Busy(ResourceBusy),
}

impl StdRuntimeOwnershipUseCase {
    pub fn acquire_route_claim(
        &self,
        layout: &RuntimeLayout,
        request: RouteClaimAcquireRequest,
    ) -> KernelResult<RouteClaimTransition> {
        let ownership = RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir);
        self.store.ensure_layout(&ownership)?;
        let claim = request.claim;
        let mut locks = vec![
            (
                crate::features::resource_coordination::ResourceKey::maintenance(),
                ResourceLockMode::Shared,
            ),
            (
                crate::features::resource_coordination::ResourceKey::new(
                    crate::features::resource_coordination::ResourceKind::Cluster,
                    claim.cluster_ref.to_string(),
                ),
                ResourceLockMode::Shared,
            ),
            (
                crate::features::resource_coordination::ResourceKey::new(
                    crate::features::resource_coordination::ResourceKind::Server,
                    &claim.server_ref,
                ),
                ResourceLockMode::Shared,
            ),
            (claim.claim_key(), ResourceLockMode::Exclusive),
        ];
        for key in claim.target.transition_keys().into_iter().skip(1) {
            locks.push((key, ResourceLockMode::Shared));
        }
        let permit = match self.coordinator.acquire(
            layout,
            ResourceLockRequest::new("acquire-route-claim", locks),
        )? {
            Ok(permit) => permit,
            Err(busy) => return Ok(RouteClaimTransition::Busy(busy)),
        };
        let _operation = self.begin_operation(
            layout,
            "acquire-route-claim",
            permit.operation_id().to_string(),
            permit.keys().to_vec(),
        )?;
        if let Some(existing) = self.store.read_claim(&ownership, &claim.owner_id)? {
            if existing.process == claim.process {
                drop(permit);
                return Ok(RouteClaimTransition::Existing(existing));
            }
            let existing_status = self.route_owner_probe.probe_process_identity(
                layout,
                &existing.server_ref,
                existing.process.pid,
                &existing.process.token,
            )?;
            if existing_status == ServerProcessIdentityStatus::Matching {
                drop(permit);
                return Ok(RouteClaimTransition::Existing(existing));
            }
            let incoming_matches = self.route_owner_probe.probe_process_identity(
                layout,
                &claim.server_ref,
                claim.process.pid,
                &claim.process.token,
            )? == ServerProcessIdentityStatus::Matching;
            let existing_stopped = existing_status == ServerProcessIdentityStatus::Stopped
                || !self
                    .process_probe
                    .is_process_running(existing.process.pid)?;
            if !incoming_matches && !existing_stopped {
                drop(permit);
                return Ok(RouteClaimTransition::Existing(existing));
            }
            self.store.write_claim(&ownership, &claim)?;
            drop(permit);
            return Ok(RouteClaimTransition::Acquired(claim));
        }
        self.store.write_claim(&ownership, &claim)?;
        drop(permit);
        Ok(RouteClaimTransition::Acquired(claim))
    }

    pub fn retire_route_claim(
        &self,
        layout: &RuntimeLayout,
        owner_id: &str,
    ) -> KernelResult<Option<ResourceBusy>> {
        self.change_claim(layout, owner_id, false)
    }

    pub fn release_route_claim(
        &self,
        layout: &RuntimeLayout,
        owner_id: &str,
    ) -> KernelResult<Option<ResourceBusy>> {
        self.change_claim(layout, owner_id, true)
    }

    fn change_claim(
        &self,
        layout: &RuntimeLayout,
        owner_id: &str,
        remove: bool,
    ) -> KernelResult<Option<ResourceBusy>> {
        let ownership = RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir);
        let Some(claim) = self.store.read_claim(&ownership, owner_id)? else {
            return Ok(None);
        };
        let locks = vec![
            (
                crate::features::resource_coordination::ResourceKey::maintenance(),
                ResourceLockMode::Shared,
            ),
            (claim.claim_key(), ResourceLockMode::Exclusive),
        ];
        let permit = match self.coordinator.acquire(
            layout,
            ResourceLockRequest::new(
                if remove {
                    "release-route-claim"
                } else {
                    "retire-route-claim"
                },
                locks,
            ),
        )? {
            Ok(permit) => permit,
            Err(busy) => return Ok(Some(busy)),
        };
        let _operation = self.begin_operation(
            layout,
            if remove {
                "release-route-claim"
            } else {
                "retire-route-claim"
            },
            permit.operation_id().to_string(),
            permit.keys().to_vec(),
        )?;
        let Some(mut claim) = self.store.read_claim(&ownership, owner_id)? else {
            return Ok(None);
        };
        if remove {
            self.store.remove_claim(&ownership, owner_id)?;
        } else {
            claim.state = RouteClaimState::Retiring;
            claim.operation = RouteClaimOperation::Retire;
            claim.updated_at = now_text();
            self.store.write_claim(&ownership, &claim)?;
        }
        drop(permit);
        Ok(None)
    }
}
