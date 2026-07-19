use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use tentgent_kernel::features::{
    resource_coordination::ResourceBusy, resource_guard::ResourceGuardRejection,
};
use tentgent_kernel::foundation::error::KernelError;

use super::response::ErrorResponse;

#[derive(Debug)]
pub struct RestError {
    status: StatusCode,
    body: ErrorResponse,
}

impl RestError {
    pub fn bad_request(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }

    pub fn not_found(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, message)
    }

    pub fn conflict(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    pub fn payload_too_large(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, code, message)
    }

    pub fn internal(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, code, message)
    }

    pub fn store_lookup(code: impl Into<String>, message: String) -> Self {
        if message.contains("was not found") || message.contains("not found") {
            Self::not_found("not_found", message)
        } else if message.contains("ambiguous") {
            Self::conflict("ambiguous_ref", message)
        } else {
            Self::internal(code, message)
        }
    }

    pub fn kernel(code: impl Into<String>, error: KernelError) -> Self {
        match error {
            KernelError::ResourceStateUnstable { .. } => {
                Self::conflict("resource-state-unstable", error.to_string())
            }
            error => Self::internal(code, error.to_string()),
        }
    }

    pub fn resource_guard(rejection: ResourceGuardRejection) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            body: ErrorResponse {
                error: rejection.code.to_string(),
                message: rejection.description,
                blockers: rejection.blockers,
            },
        }
    }

    pub fn resource_busy(busy: ResourceBusy) -> Self {
        Self::conflict(
            busy.code.to_string(),
            format!(
                "{}; retry after {} ms",
                busy.description, busy.retry_after_millis
            ),
        )
    }

    pub fn guarded<T>(
        outcome: tentgent_kernel::features::resource_guard::ResourceMutationOutcome<T>,
    ) -> Result<T, Self> {
        match outcome {
            tentgent_kernel::features::resource_guard::ResourceMutationOutcome::Applied(value) => {
                Ok(value)
            }
            tentgent_kernel::features::resource_guard::ResourceMutationOutcome::Blocked(
                rejection,
            ) => Err(Self::resource_guard(rejection)),
            tentgent_kernel::features::resource_guard::ResourceMutationOutcome::Busy(busy) => {
                Err(Self::resource_busy(busy))
            }
        }
    }

    fn new(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            body: ErrorResponse {
                error: code.into(),
                message: message.into(),
                blockers: Vec::new(),
            },
        }
    }
}

impl IntoResponse for RestError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use tentgent_kernel::features::resource_coordination::{
        ResourceBusy, ResourceCoordinationCode, ResourceKey, ResourceKind,
    };

    use super::*;

    #[test]
    fn unstable_resource_state_is_a_retryable_conflict_code() {
        let error = RestError::resource_busy(ResourceBusy {
            code: ResourceCoordinationCode::ResourceStateUnstable,
            operation_id: "operation-a".to_string(),
            key: ResourceKey::new(ResourceKind::Adapter, "adapter-a"),
            attempts: 3,
            waited_millis: 100,
            holders: Vec::new(),
            retry_after_millis: 50,
            description: "adapter dependencies changed during authorization".to_string(),
        });

        assert_eq!(error.status, StatusCode::CONFLICT);
        assert_eq!(error.body.error, "resource-state-unstable");
        assert!(error.body.message.contains("retry after 50 ms"));
    }

    #[test]
    fn kernel_unstable_state_preserves_the_public_conflict_code() {
        let error = RestError::kernel(
            "internal_error",
            KernelError::ResourceStateUnstable {
                resource: "model-a/chat".to_string(),
                description: "capability metadata kept changing".to_string(),
                retry_after_millis: 50,
            },
        );

        assert_eq!(error.status, StatusCode::CONFLICT);
        assert_eq!(error.body.error, "resource-state-unstable");
    }
}
