use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use tentgent_kernel::features::cluster::domain::{
    ClusterRouteExecutionBlockerCode, ClusterRouteExecutionDecision,
};

use crate::server::local::LocalServerError;

#[derive(Debug)]
pub(super) struct ClusterServerError {
    status: StatusCode,
    code: String,
    message: String,
}

impl ClusterServerError {
    pub(super) fn definition_reload_failed(message: String) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "cluster_definition_reload_failed".to_string(),
            message,
        }
    }

    pub(super) fn route_unavailable(message: String) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: ClusterRouteExecutionBlockerCode::Unavailable
                .as_str()
                .to_string(),
            message,
        }
    }

    pub(super) fn unsupported_path(path: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "cluster_route_unsupported".to_string(),
            message: format!("cluster server path `{path}` is not supported"),
        }
    }

    pub(super) fn from_decision(decision: ClusterRouteExecutionDecision) -> Self {
        let ClusterRouteExecutionDecision::Blocked {
            code, description, ..
        } = decision
        else {
            return Self::definition_reload_failed(
                "cluster route resolver returned an unexpected ready decision".to_string(),
            );
        };
        let status = match code {
            ClusterRouteExecutionBlockerCode::MissingRoute
            | ClusterRouteExecutionBlockerCode::UnsupportedTarget => StatusCode::BAD_REQUEST,
            ClusterRouteExecutionBlockerCode::NotReady
            | ClusterRouteExecutionBlockerCode::ProofStale
            | ClusterRouteExecutionBlockerCode::ProofFailed
            | ClusterRouteExecutionBlockerCode::Unsupported => StatusCode::CONFLICT,
            ClusterRouteExecutionBlockerCode::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        };
        Self {
            status,
            code: code.as_str().to_string(),
            message: description,
        }
    }
}

impl From<LocalServerError> for ClusterServerError {
    fn from(error: LocalServerError) -> Self {
        if error.code == "local_proxy_failed" {
            return Self::route_unavailable(error.message);
        }
        Self {
            status: error.status,
            code: error.code.to_string(),
            message: error.message,
        }
    }
}

impl IntoResponse for ClusterServerError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({
                "error": self.code,
                "message": self.message,
            })),
        )
            .into_response()
    }
}

impl std::fmt::Display for ClusterServerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ClusterServerError {}

#[cfg(test)]
mod tests {
    use axum::response::IntoResponse;

    use super::*;

    #[test]
    fn runtime_supervisor_failure_maps_to_route_unavailable() {
        let response = ClusterServerError::from(LocalServerError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "local_proxy_failed",
            message: "runtime launch failed".to_string(),
        })
        .into_response();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
