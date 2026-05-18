use std::time::Duration;

use crate::features::daemon::domain::daemon_status_probe_warning;
use crate::features::daemon::ports::{
    DaemonHttpReadinessProbe, DaemonPortFuture, DaemonStatusProbeOutcome,
};
use crate::foundation::error::KernelResult;

use super::error::daemon_runtime_error;

pub const DEFAULT_DAEMON_PROBE_TIMEOUT: Duration = Duration::from_millis(500);

/// Reqwest-backed daemon HTTP readiness probe.
#[derive(Debug, Clone)]
pub struct ReqwestDaemonHttpReadinessProbe {
    client: reqwest::Client,
}

impl Default for ReqwestDaemonHttpReadinessProbe {
    fn default() -> Self {
        Self::new(DEFAULT_DAEMON_PROBE_TIMEOUT)
            .expect("default daemon readiness client should be constructible")
    }
}

impl ReqwestDaemonHttpReadinessProbe {
    pub fn new(timeout: Duration) -> KernelResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|err| {
                daemon_runtime_error(format!("build daemon readiness client failed: {err}"))
            })?;
        Ok(Self { client })
    }
}

impl DaemonHttpReadinessProbe for ReqwestDaemonHttpReadinessProbe {
    fn probe_healthz<'a>(&'a self, daemon_url: &'a str) -> DaemonPortFuture<'a, ()> {
        Box::pin(async move {
            let response = self
                .client
                .get(endpoint_url(daemon_url, "/healthz"))
                .send()
                .await
                .map_err(|err| {
                    daemon_runtime_error(format!("daemon health probe failed: {err}"))
                })?;
            if response.status().is_success() {
                Ok(())
            } else {
                Err(daemon_runtime_error(format!(
                    "GET /healthz returned {}",
                    response.status()
                )))
            }
        })
    }

    fn probe_status<'a>(
        &'a self,
        daemon_url: &'a str,
        token: &'a str,
    ) -> DaemonPortFuture<'a, DaemonStatusProbeOutcome> {
        Box::pin(async move {
            let response = self
                .client
                .get(endpoint_url(daemon_url, "/v1/status"))
                .bearer_auth(token)
                .send()
                .await;
            let response = match response {
                Ok(response) => response,
                Err(err) => {
                    return Ok(DaemonStatusProbeOutcome {
                        status_warning: Some(format!(
                            "daemon ready but /v1/status could not be confirmed: {err}"
                        )),
                    });
                }
            };
            let status = response.status();
            Ok(DaemonStatusProbeOutcome {
                status_warning: daemon_status_probe_warning(
                    status.as_u16(),
                    status.is_success(),
                    status,
                ),
            })
        })
    }
}

fn endpoint_url(daemon_url: &str, path: &str) -> String {
    format!("{}{}", daemon_url.trim_end_matches('/'), path)
}
