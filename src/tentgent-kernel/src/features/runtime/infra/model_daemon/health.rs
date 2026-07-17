use serde::Deserialize;

use crate::foundation::error::KernelResult;

use super::supervisor::ModelRuntimeDaemonEndpoint;

#[derive(Debug, Deserialize)]
pub(super) struct HealthPayload {
    pub status: String,
    pub pid: u32,
    #[serde(default)]
    pub process_token: Option<String>,
    pub runtime: HealthRuntimePayload,
}

#[derive(Debug, Deserialize)]
pub(super) struct HealthRuntimePayload {
    pub capability: String,
    pub model_ref: Option<String>,
}

pub(super) fn mismatch_reason(
    payload: &HealthPayload,
    expected_pid: u32,
    expected_process_token: &str,
    expected_capability: &str,
    expected_model_ref: Option<&str>,
) -> Option<String> {
    if payload.status != "ok" {
        return Some(format!(
            "healthz status is `{}` instead of `ok`",
            payload.status
        ));
    }
    if payload.pid != expected_pid {
        return Some("healthz pid does not match the spawned runtime".to_string());
    }
    if !expected_process_token.starts_with("legacy-pid-") {
        match payload.process_token.as_deref() {
            None => {
                return Some(
                    "healthz process_token is missing; the Python runtime environment is older than this Tentgent build; run `tentgent runtime bootstrap --profile local-model`"
                        .to_string(),
                );
            }
            Some(actual) if actual != expected_process_token => {
                return Some(
                    "healthz process_token does not match the spawned runtime".to_string(),
                );
            }
            Some(_) => {}
        }
    }
    if payload.runtime.capability != expected_capability {
        return Some("healthz capability does not match the requested runtime".to_string());
    }
    if payload.runtime.model_ref.as_deref() != expected_model_ref {
        return Some("healthz model_ref does not match the requested runtime".to_string());
    }
    None
}

pub(super) async fn endpoint_health_mismatch(
    client: &reqwest::Client,
    endpoint: &ModelRuntimeDaemonEndpoint,
) -> KernelResult<Option<String>> {
    let response = match client.get(endpoint.url("/healthz")).send().await {
        Ok(response) => response,
        Err(error) => return Ok(Some(format!("healthz is unavailable: {error}"))),
    };
    if !response.status().is_success() {
        return Ok(Some(format!(
            "healthz returned HTTP status {}",
            response.status()
        )));
    }
    let payload = match response.json::<HealthPayload>().await {
        Ok(payload) => payload,
        Err(error) => return Ok(Some(format!("healthz response is invalid: {error}"))),
    };
    Ok(mismatch_reason(
        &payload,
        endpoint.pid,
        &endpoint.process_token,
        endpoint.capability.as_str(),
        endpoint.model_ref.as_deref(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{mismatch_reason, HealthPayload, HealthRuntimePayload};

    fn payload(process_token: Option<&str>) -> HealthPayload {
        HealthPayload {
            status: "ok".to_string(),
            pid: 42,
            process_token: process_token.map(ToOwned::to_owned),
            runtime: HealthRuntimePayload {
                capability: "chat".to_string(),
                model_ref: Some("model-ref".to_string()),
            },
        }
    }

    #[test]
    fn reports_stale_python_runtime_when_process_token_is_missing() {
        let reason = mismatch_reason(
            &payload(None),
            42,
            "current-token",
            "chat",
            Some("model-ref"),
        )
        .expect("missing token should be rejected");

        assert!(reason.contains("Python runtime environment is older"));
        assert!(reason.contains("tentgent runtime bootstrap --profile local-model"));
    }

    #[test]
    fn accepts_matching_current_and_legacy_runtime_identities() {
        assert_eq!(
            mismatch_reason(
                &payload(Some("current-token")),
                42,
                "current-token",
                "chat",
                Some("model-ref"),
            ),
            None
        );
        assert_eq!(
            mismatch_reason(
                &payload(None),
                42,
                "legacy-pid-42",
                "chat",
                Some("model-ref"),
            ),
            None
        );
    }
}
