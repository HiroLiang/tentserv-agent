use tentgent_kernel::features::runtime_ownership::{
    RuntimeExecutionIdentity, RuntimeOwnershipView,
};

pub(super) fn render_runtime_ownership(view: &RuntimeOwnershipView) {
    for line in runtime_ownership_lines(view) {
        println!("{line}");
    }
}

fn runtime_ownership_lines(view: &RuntimeOwnershipView) -> Vec<String> {
    let mut lines = vec![format!(
        "Ownership [{}{}]: {} ({} route claims, {} runtime generations, {} transitions, {} stale, {} malformed)",
        view.scope.kind,
        view.scope
            .reference
            .as_deref()
            .map(|reference| format!(" {reference}"))
            .unwrap_or_default(),
        view.summary.status,
        view.summary.route_claim_count,
        view.summary.active_generation_count,
        view.summary.active_operation_count,
        view.summary.stale_record_count,
        view.summary.malformed_record_count,
    )];
    for claim in &view.claims {
        lines.push(format!(
            "  route {} {} -> {} ({})",
            claim.cluster_ref,
            claim.route,
            identity_label(&claim.target),
            claim.state.as_str(),
        ));
    }
    for generation in &view.generations {
        lines.push(format!(
            "  runtime {} ({}, runtime_idle_seconds={}, model_idle_seconds={}){}",
            identity_label(&generation.identity),
            generation.state.as_str(),
            generation.policy.runtime_idle_seconds,
            generation.policy.model_idle_seconds,
            generation
                .diagnostic
                .as_deref()
                .map(|diagnostic| format!(": {diagnostic}"))
                .unwrap_or_default(),
        ));
    }
    for issue in &view.issues {
        lines.push(format!(
            "  {} {}: {}",
            if issue.recoverable {
                "warning"
            } else {
                "blocked"
            },
            issue.kind,
            issue.description,
        ));
    }
    lines
}

fn identity_label(identity: &RuntimeExecutionIdentity) -> String {
    match identity {
        RuntimeExecutionIdentity::ModelBound {
            model_ref,
            capability,
            profile,
        } => format!(
            "model {} / {} / {}",
            short_ref(model_ref),
            capability,
            profile.label(),
        ),
        RuntimeExecutionIdentity::Unbound {
            capability,
            profile,
        } => format!("unbound {} / {}", capability, profile.label()),
    }
}

fn short_ref(reference: &str) -> &str {
    reference.get(..12).unwrap_or(reference)
}

#[cfg(test)]
mod tests {
    use tentgent_kernel::features::{
        cluster::domain::ClusterRouteKey,
        runtime::infra::ModelRuntimeCapability,
        runtime_ownership::{
            RouteClaimOperation, RouteClaimState, RuntimeGenerationOperation,
            RuntimeGenerationState, RuntimeLaunchPolicyRecord, RuntimeOwnershipClaimView,
            RuntimeOwnershipGenerationView, RuntimeOwnershipIssueView, RuntimeOwnershipScopeView,
            RuntimeOwnershipStatus, RuntimeOwnershipSummary,
        },
    };

    use super::*;

    #[test]
    fn focused_rendering_uses_safe_projection_and_short_model_reference() {
        let model_ref = "a".repeat(64);
        let identity = RuntimeExecutionIdentity::model_bound(
            model_ref.clone(),
            ModelRuntimeCapability::Chat,
            None,
        );
        let view = RuntimeOwnershipView {
            summary: RuntimeOwnershipSummary {
                status: RuntimeOwnershipStatus::Attention,
                route_claim_count: 1,
                active_generation_count: 1,
                active_operation_count: 0,
                stale_record_count: 0,
                malformed_record_count: 0,
            },
            scope: RuntimeOwnershipScopeView {
                kind: "cluster".to_string(),
                reference: Some("local-assistant".to_string()),
            },
            claims: vec![RuntimeOwnershipClaimView {
                state: RouteClaimState::Active,
                server_ref: "server-a".to_string(),
                cluster_ref: "local-assistant".to_string(),
                route: ClusterRouteKey::Chat.to_string(),
                definition_hash: "definition-a".to_string(),
                target: identity.clone(),
                operation: RouteClaimOperation::Acquire,
                acquired_at: "2026-07-16T00:00:00Z".to_string(),
                updated_at: "2026-07-16T00:00:00Z".to_string(),
            }],
            generations: vec![RuntimeOwnershipGenerationView {
                state: RuntimeGenerationState::Closing,
                identity,
                policy: RuntimeLaunchPolicyRecord {
                    runtime_idle_seconds: 300,
                    model_idle_seconds: 0,
                    legacy_unbounded_model: false,
                },
                operation: RuntimeGenerationOperation::Close,
                diagnostic: Some("waiting for verified shutdown".to_string()),
                started_at: "2026-07-16T00:00:00Z".to_string(),
                updated_at: "2026-07-16T00:00:01Z".to_string(),
            }],
            issues: vec![RuntimeOwnershipIssueView {
                kind: "stale-generation".to_string(),
                description: "reconcile after shutdown".to_string(),
                recoverable: true,
            }],
        };

        let lines = runtime_ownership_lines(&view);
        let output = lines.join("\n");
        assert!(output.contains("Ownership [cluster local-assistant]: attention"));
        assert!(output.contains("model aaaaaaaaaaaa / chat / default@1"));
        assert!(output.contains("(active)"));
        assert!(output.contains(
            "(closing, runtime_idle_seconds=300, model_idle_seconds=0): waiting for verified shutdown"
        ));
        assert!(output.contains("warning stale-generation"));
        assert!(!output.contains(&model_ref));
        assert!(!output.contains("process_token"));
    }
}
