use miette::{miette, Result};
use tentgent_kernel::features::resource_guard::ResourceMutationOutcome;

pub(super) fn project_resource_mutation<T>(outcome: ResourceMutationOutcome<T>) -> Result<T> {
    match outcome {
        ResourceMutationOutcome::Applied(value) => Ok(value),
        ResourceMutationOutcome::Blocked(rejection) => {
            let mut lines = vec![format!(
                "blocked [{}] {} for {}",
                rejection.code, rejection.operation, rejection.resource
            )];
            for blocker in &rejection.blockers {
                lines.push(format!(
                    "blocker [{}] {} {}: {}",
                    blocker.code, blocker.kind, blocker.reference, blocker.reason
                ));
            }
            let mut actions = Vec::new();
            for action in rejection
                .blockers
                .iter()
                .flat_map(|blocker| blocker.next_actions.iter())
            {
                if !actions.contains(action) {
                    actions.push(action.clone());
                }
            }
            for action in actions {
                lines.push(format!("next: {action}"));
            }
            Err(miette!(lines.join("\n")))
        }
        ResourceMutationOutcome::Busy(busy) => Err(miette!(format!(
            "busy [{}] {}: {}\nretry: after {} ms",
            busy.code,
            busy.key.label(),
            busy.description,
            busy.retry_after_millis
        ))),
    }
}

#[cfg(test)]
mod tests {
    use tentgent_kernel::features::{
        resource_coordination::{
            ResourceBusy, ResourceCoordinationCode, ResourceKey, ResourceKind,
        },
        resource_guard::{
            ResourceBlocker, ResourceBlockerCode, ResourceGuardCode, ResourceGuardRejection,
            ResourceMutationOutcome,
        },
    };

    use super::project_resource_mutation;

    #[test]
    fn blocked_projection_includes_codes_and_deduplicated_actions() {
        let error = project_resource_mutation::<()>(ResourceMutationOutcome::Blocked(
            ResourceGuardRejection {
                code: ResourceGuardCode::ModelInUse,
                operation: "delete-model".to_string(),
                resource: "model-a".to_string(),
                description: "model is in use".to_string(),
                blockers: vec![ResourceBlocker {
                    kind: "server".to_string(),
                    code: ResourceBlockerCode::ServerRunning,
                    reference: "server-a".to_string(),
                    reason: "server is running".to_string(),
                    resource_kind: None,
                    resource_ref: None,
                    operation: None,
                    capability: None,
                    route: None,
                    field: None,
                    owner: None,
                    next_actions: vec!["stop server-a".to_string(), "stop server-a".to_string()],
                }],
            },
        ))
        .expect_err("blocked mutation");
        let message = error.to_string();
        assert!(message.contains("blocked [model_in_use] delete-model for model-a"));
        assert!(message.contains("blocker [server-running] server server-a"));
        assert_eq!(message.matches("next: stop server-a").count(), 1);
    }

    #[test]
    fn busy_projection_includes_key_and_retry_delay() {
        let error = project_resource_mutation::<()>(ResourceMutationOutcome::Busy(ResourceBusy {
            code: ResourceCoordinationCode::ResourceBusy,
            operation_id: "operation-a".to_string(),
            key: ResourceKey::new(ResourceKind::Model, "model-a"),
            attempts: 3,
            waited_millis: 20,
            holders: Vec::new(),
            retry_after_millis: 100,
            description: "model transition is busy".to_string(),
        }))
        .expect_err("busy mutation");
        let message = error.to_string();
        assert!(message.contains("busy [resource-busy] model:model-a"));
        assert!(message.contains("retry: after 100 ms"));
    }
}
