use serde_json::Value;

use crate::features::server::domain::{ServerInspection, ServerRuntimeKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerReadinessObservation {
    Ready,
    Starting,
    IdentityMismatch,
}

/// Process existence and HTTP reachability alone are not load readiness.
pub fn observe_server_readiness(
    inspection: &ServerInspection,
    payload: &Value,
) -> ServerReadinessObservation {
    let identity_matches = payload["server_ref"].as_str()
        == Some(inspection.spec.server_ref.as_str())
        && payload["runtime_home"].as_str() == Some(inspection.home_dir.to_string_lossy().as_ref())
        && inspection
            .process
            .as_ref()
            .and_then(|process| process.process_token.as_deref())
            .is_none_or(|token| payload["process_token"].as_str() == Some(token));
    if !identity_matches {
        return ServerReadinessObservation::IdentityMismatch;
    }
    let ready = payload["ok"].as_bool() == Some(true)
        && (inspection.spec.runtime_kind != ServerRuntimeKind::Local
            || (payload["ready"].as_bool() == Some(true)
                && payload["status"].as_str() == Some("ready")));
    if ready {
        ServerReadinessObservation::Ready
    } else {
        ServerReadinessObservation::Starting
    }
}
