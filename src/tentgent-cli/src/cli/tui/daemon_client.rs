use std::time::Duration;

use reqwest::StatusCode;
use serde_json::Value;
use tentgent_core::config::DaemonUrlSource;

const DAEMON_HTTP_TIMEOUT: Duration = Duration::from_millis(700);
#[cfg(test)]
pub(super) const AUTO_REFRESH_PATHS: [&str; 3] = ["/healthz", "/v1/status", "/v1/doctor"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TuiTokenSource {
    Session,
    Flag,
    Env,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DaemonConnectionState {
    Ready,
    AuthRequired,
    Down,
    Timeout,
    DaemonProtocolError,
    DaemonError,
}

impl DaemonConnectionState {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::AuthRequired => "auth required",
            Self::Down => "down",
            Self::Timeout => "timeout",
            Self::DaemonProtocolError => "protocol error",
            Self::DaemonError => "daemon error",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct DaemonSnapshot {
    pub(super) state: DaemonConnectionState,
    pub(super) detail: String,
    pub(super) status: Option<Value>,
    pub(super) doctor: Option<Value>,
}

impl DaemonSnapshot {
    pub(super) fn down(detail: impl Into<String>) -> Self {
        Self {
            state: DaemonConnectionState::Down,
            detail: detail.into(),
            status: None,
            doctor: None,
        }
    }

    pub(super) fn idle() -> Self {
        Self::down("not checked yet")
    }
}

pub(super) struct DaemonClient {
    base_url: String,
    token: Option<String>,
    token_source: TuiTokenSource,
    client: reqwest::Client,
}

impl DaemonClient {
    pub(super) fn new(
        base_url: String,
        token: Option<String>,
        token_source: TuiTokenSource,
    ) -> miette::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(DAEMON_HTTP_TIMEOUT)
            .build()
            .map_err(|error| miette::miette!("failed to build daemon client: {error}"))?;
        Ok(Self {
            base_url,
            token,
            token_source,
            client,
        })
    }

    pub(super) async fn refresh_auto(&self) -> DaemonSnapshot {
        let health = self.client.get(self.endpoint("/healthz")).send().await;
        match health {
            Ok(response) if response.status().is_success() => {}
            Ok(response) => {
                return DaemonSnapshot::down(format!(
                    "GET /healthz returned {}",
                    response.status()
                ));
            }
            Err(error) if error.is_timeout() => {
                return snapshot_for_health_failure(
                    DaemonHealthFailure::Timeout,
                    &error.to_string(),
                );
            }
            Err(error) => {
                return snapshot_for_health_failure(DaemonHealthFailure::Down, &error.to_string());
            }
        }

        let status = self.get("/v1/status").send().await;
        let status_response = match status {
            Ok(response) => response,
            Err(error) if error.is_timeout() => {
                return DaemonSnapshot {
                    state: DaemonConnectionState::Timeout,
                    detail: format!("GET /v1/status timed out after /healthz succeeded: {error}"),
                    status: None,
                    doctor: None,
                };
            }
            Err(error) => {
                return DaemonSnapshot {
                    state: DaemonConnectionState::DaemonError,
                    detail: format!("GET /v1/status failed after /healthz succeeded: {error}"),
                    status: None,
                    doctor: None,
                };
            }
        };

        if let Some(snapshot) =
            snapshot_for_status_failure(status_response.status(), self.token_source)
        {
            return snapshot;
        }

        let status_json = match status_response.json::<Value>().await {
            Ok(value) => value,
            Err(error) => {
                return protocol_error_snapshot(format!("invalid /v1/status JSON: {error}"))
            }
        };

        let doctor = self.optional_json("/v1/doctor").await;

        DaemonSnapshot {
            state: DaemonConnectionState::Ready,
            detail: "daemon HTTP is reachable".to_string(),
            status: Some(status_json),
            doctor,
        }
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        let request = self.client.get(self.endpoint(path));
        match self.token.as_deref() {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    async fn optional_json(&self, path: &str) -> Option<Value> {
        let response = self.get(path).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.json::<Value>().await.ok()
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.base_url.trim_end_matches('/'), path)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DaemonHealthFailure {
    Down,
    Timeout,
}

pub(super) fn snapshot_for_health_failure(
    failure: DaemonHealthFailure,
    detail: &str,
) -> DaemonSnapshot {
    match failure {
        DaemonHealthFailure::Down => DaemonSnapshot {
            state: DaemonConnectionState::Down,
            detail: format!("GET /healthz failed: {detail}"),
            status: None,
            doctor: None,
        },
        DaemonHealthFailure::Timeout => DaemonSnapshot {
            state: DaemonConnectionState::Timeout,
            detail: format!("GET /healthz timed out: {detail}"),
            status: None,
            doctor: None,
        },
    }
}

pub(super) fn snapshot_for_status_failure(
    status: StatusCode,
    token_source: TuiTokenSource,
) -> Option<DaemonSnapshot> {
    if status.is_success() {
        return None;
    }
    if status == StatusCode::UNAUTHORIZED {
        let hint = match token_source {
            TuiTokenSource::Session => "the current TUI token was rejected",
            TuiTokenSource::Flag => "the --token value was rejected",
            TuiTokenSource::Env => "TENTGENT_DAEMON_TOKEN was rejected",
            TuiTokenSource::None => "set --token or TENTGENT_DAEMON_TOKEN",
        };
        return Some(DaemonSnapshot {
            state: DaemonConnectionState::AuthRequired,
            detail: format!("/healthz is ready, but /v1/status requires bearer auth; {hint}"),
            status: None,
            doctor: None,
        });
    }
    if status.is_server_error() {
        return Some(DaemonSnapshot {
            state: DaemonConnectionState::DaemonError,
            detail: format!("/v1/status returned {status}"),
            status: None,
            doctor: None,
        });
    }

    Some(DaemonSnapshot {
        state: DaemonConnectionState::DaemonError,
        detail: format!("/v1/status returned {status}"),
        status: None,
        doctor: None,
    })
}

pub(super) fn protocol_error_snapshot(detail: impl Into<String>) -> DaemonSnapshot {
    DaemonSnapshot {
        state: DaemonConnectionState::DaemonProtocolError,
        detail: detail.into(),
        status: None,
        doctor: None,
    }
}

pub(super) fn url_source_label(source: DaemonUrlSource) -> &'static str {
    match source {
        DaemonUrlSource::Flag => "flag",
        DaemonUrlSource::Env => "env",
        DaemonUrlSource::Config => "config",
        DaemonUrlSource::Metadata => "metadata",
        DaemonUrlSource::Default => "default",
    }
}

pub(super) fn token_source_label(source: TuiTokenSource) -> &'static str {
    match source {
        TuiTokenSource::Session => "session",
        TuiTokenSource::Flag => "flag",
        TuiTokenSource::Env => "env",
        TuiTokenSource::None => "none",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping_treats_401_as_auth_required_not_down() {
        let snapshot = snapshot_for_status_failure(StatusCode::UNAUTHORIZED, TuiTokenSource::None)
            .expect("snapshot");

        assert_eq!(snapshot.state, DaemonConnectionState::AuthRequired);
        assert!(snapshot.detail.contains("/healthz is ready"));
    }

    #[test]
    fn status_mapping_covers_5xx_and_protocol_error() {
        let snapshot =
            snapshot_for_status_failure(StatusCode::INTERNAL_SERVER_ERROR, TuiTokenSource::Env)
                .expect("snapshot");
        assert_eq!(snapshot.state, DaemonConnectionState::DaemonError);

        let protocol = protocol_error_snapshot("invalid json");
        assert_eq!(protocol.state, DaemonConnectionState::DaemonProtocolError);
    }

    #[test]
    fn health_mapping_covers_down_and_timeout() {
        let down = snapshot_for_health_failure(DaemonHealthFailure::Down, "connection refused");
        assert_eq!(down.state, DaemonConnectionState::Down);

        let timeout = snapshot_for_health_failure(DaemonHealthFailure::Timeout, "deadline");
        assert_eq!(timeout.state, DaemonConnectionState::Timeout);
    }

    #[test]
    fn automatic_refresh_paths_do_not_include_auth_route() {
        assert!(!AUTO_REFRESH_PATHS.contains(&"/v1/auth"));
    }
}
