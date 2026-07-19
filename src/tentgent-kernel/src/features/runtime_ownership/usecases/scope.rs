use std::collections::BTreeSet;

use crate::{
    features::{
        resource_coordination::ResourceKey,
        runtime_ownership::{
            OwnershipOperationRecord, RouteGenerationClaim, RuntimeExecutionIdentity,
            RuntimeGenerationRecord, RuntimeOwnershipClaimView, RuntimeOwnershipGenerationView,
            RuntimeOwnershipIssue, RuntimeOwnershipIssueView, RuntimeOwnershipScope,
            RuntimeOwnershipScopeView, RuntimeOwnershipView,
        },
    },
    foundation::{error::KernelResult, layout::RuntimeLayout},
};

use super::{runtime_ownership_summary, StdRuntimeOwnershipUseCase};

impl StdRuntimeOwnershipUseCase {
    pub fn inspect_runtime_ownership_scope(
        &self,
        layout: &RuntimeLayout,
        scope: RuntimeOwnershipScope,
    ) -> KernelResult<RuntimeOwnershipView> {
        let inspection = self.inspect_runtime_ownership(layout)?;
        let (claims, identities, scope_view, scope_keys) = match scope {
            RuntimeOwnershipScope::Global => (
                inspection.claims.clone(),
                inspection
                    .generations
                    .iter()
                    .map(|generation| generation.identity.clone())
                    .collect::<BTreeSet<_>>(),
                RuntimeOwnershipScopeView {
                    kind: "global".to_string(),
                    reference: None,
                },
                None,
            ),
            RuntimeOwnershipScope::Cluster { cluster_ref } => {
                let claims = inspection
                    .claims
                    .iter()
                    .filter(|claim| claim.cluster_ref == cluster_ref)
                    .cloned()
                    .collect::<Vec<_>>();
                let identities = claims
                    .iter()
                    .map(|claim| claim.target.clone())
                    .collect::<BTreeSet<_>>();
                let mut keys = BTreeSet::from([ResourceKey::new(
                    crate::features::resource_coordination::ResourceKind::Cluster,
                    cluster_ref.to_string(),
                )]);
                extend_identity_keys(&mut keys, &identities);
                (
                    claims,
                    identities,
                    RuntimeOwnershipScopeView {
                        kind: "cluster".to_string(),
                        reference: Some(cluster_ref.to_string()),
                    },
                    Some(keys),
                )
            }
            RuntimeOwnershipScope::Server {
                server_ref,
                runtime_identities,
            } => {
                let claims = inspection
                    .claims
                    .iter()
                    .filter(|claim| claim.server_ref == server_ref)
                    .cloned()
                    .collect::<Vec<_>>();
                let mut identities = runtime_identities.into_iter().collect::<BTreeSet<_>>();
                identities.extend(claims.iter().map(|claim| claim.target.clone()));
                let mut keys = BTreeSet::from([ResourceKey::new(
                    crate::features::resource_coordination::ResourceKind::Server,
                    server_ref.clone(),
                )]);
                extend_identity_keys(&mut keys, &identities);
                (
                    claims,
                    identities,
                    RuntimeOwnershipScopeView {
                        kind: "server".to_string(),
                        reference: Some(server_ref),
                    },
                    Some(keys),
                )
            }
        };
        let generations = inspection
            .generations
            .into_iter()
            .filter(|generation| identities.contains(&generation.identity))
            .collect::<Vec<_>>();
        let operations = inspection
            .operations
            .into_iter()
            .filter(|operation| operation_in_scope(operation, scope_keys.as_ref()))
            .collect::<Vec<_>>();
        let record_ids = scoped_record_ids(&claims, &generations, &operations);
        let issues = inspection
            .issues
            .into_iter()
            .filter(|issue| issue_in_scope(issue, scope_keys.is_none(), &record_ids))
            .collect::<Vec<_>>();
        let summary = runtime_ownership_summary(&claims, &generations, &operations, &issues);
        Ok(RuntimeOwnershipView {
            summary,
            scope: scope_view,
            claims: claims.into_iter().map(claim_view).collect(),
            generations: generations.into_iter().map(generation_view).collect(),
            issues: issues.into_iter().map(issue_view).collect(),
        })
    }
}

fn extend_identity_keys(
    keys: &mut BTreeSet<ResourceKey>,
    identities: &BTreeSet<RuntimeExecutionIdentity>,
) {
    for identity in identities {
        keys.extend(identity.transition_keys().into_iter().skip(1));
    }
}

fn operation_in_scope(
    operation: &OwnershipOperationRecord,
    scope_keys: Option<&BTreeSet<ResourceKey>>,
) -> bool {
    scope_keys.map_or(true, |keys| {
        operation.resource_keys.iter().any(|key| keys.contains(key))
    })
}

fn scoped_record_ids(
    claims: &[RouteGenerationClaim],
    generations: &[RuntimeGenerationRecord],
    operations: &[OwnershipOperationRecord],
) -> BTreeSet<String> {
    claims
        .iter()
        .map(|claim| claim.owner_id.clone())
        .chain(
            generations
                .iter()
                .map(|generation| generation.runtime_key.clone()),
        )
        .chain(
            operations
                .iter()
                .map(|operation| operation.operation_id.clone()),
        )
        .collect()
}

fn issue_in_scope(
    issue: &RuntimeOwnershipIssue,
    global: bool,
    record_ids: &BTreeSet<String>,
) -> bool {
    global
        || record_ids.contains(&issue.record)
        || std::path::Path::new(&issue.record)
            .file_stem()
            .and_then(|value| value.to_str())
            .is_some_and(|value| record_ids.contains(value))
}

fn claim_view(claim: RouteGenerationClaim) -> RuntimeOwnershipClaimView {
    RuntimeOwnershipClaimView {
        state: claim.state,
        server_ref: claim.server_ref,
        cluster_ref: claim.cluster_ref.to_string(),
        route: claim.route.to_string(),
        definition_hash: claim.definition_hash,
        target: claim.target,
        operation: claim.operation,
        acquired_at: claim.acquired_at,
        updated_at: claim.updated_at,
    }
}

fn generation_view(generation: RuntimeGenerationRecord) -> RuntimeOwnershipGenerationView {
    RuntimeOwnershipGenerationView {
        state: generation.state,
        identity: generation.identity,
        policy: generation.policy,
        operation: generation.operation,
        diagnostic: generation.diagnostic,
        started_at: generation.started_at,
        updated_at: generation.updated_at,
    }
}

fn issue_view(issue: RuntimeOwnershipIssue) -> RuntimeOwnershipIssueView {
    RuntimeOwnershipIssueView {
        kind: issue.kind,
        description: issue.description,
        recoverable: issue.recoverable,
    }
}
