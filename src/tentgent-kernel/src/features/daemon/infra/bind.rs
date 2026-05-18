use std::net::IpAddr;

use crate::features::daemon::ports::{
    DaemonBindHostClass, DaemonBindSafetyChecker, DaemonBindSafetyReport, DaemonBindSafetyRequest,
};
use crate::foundation::error::KernelResult;

use super::error::daemon_runtime_error;

pub const DAEMON_TOKEN_ENV_VAR: &str = "TENTGENT_DAEMON_TOKEN";

/// Standard daemon bind-safety checker.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdDaemonBindSafetyChecker;

impl DaemonBindSafetyChecker for StdDaemonBindSafetyChecker {
    fn check_bind_safety(
        &self,
        request: DaemonBindSafetyRequest,
    ) -> KernelResult<DaemonBindSafetyReport> {
        check_bind_safety(request)
    }
}

fn check_bind_safety(request: DaemonBindSafetyRequest) -> KernelResult<DaemonBindSafetyReport> {
    let host = request.bind.host.as_str();
    let host_class = classify_bind_host(host);
    let mut warnings = Vec::new();

    if host_class == DaemonBindHostClass::Loopback {
        return Ok(DaemonBindSafetyReport {
            host_class,
            warnings,
        });
    }

    if request.token_enabled {
        warnings.push(format!(
            "daemon is binding to unsafe host `{host}` with bearer token auth enabled; this local guard is not a public-service security model"
        ));
        return Ok(DaemonBindSafetyReport {
            host_class,
            warnings,
        });
    }

    if request.allow_unsafe_bind {
        warnings.push(format!(
            "daemon is binding to unsafe host `{host}` without daemon token auth because --allow-unsafe-bind was provided; this can expose daemon lifecycle and chat endpoints to your network"
        ));
        return Ok(DaemonBindSafetyReport {
            host_class,
            warnings,
        });
    }

    Err(daemon_runtime_error(format!(
        "refusing to bind daemon to unsafe host `{host}` without `{DAEMON_TOKEN_ENV_VAR}`; set the token or pass --allow-unsafe-bind for explicit local-network experiments"
    )))
}

fn classify_bind_host(host: &str) -> DaemonBindHostClass {
    let host = host.trim();
    if host.eq_ignore_ascii_case("localhost") {
        return DaemonBindHostClass::Loopback;
    }

    let unbracketed = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);

    match unbracketed.parse::<IpAddr>() {
        Ok(address) if address.is_unspecified() => DaemonBindHostClass::Wildcard,
        Ok(address) if address.is_loopback() => DaemonBindHostClass::Loopback,
        Ok(_) => DaemonBindHostClass::NonLoopback,
        Err(_) => DaemonBindHostClass::NonLoopback,
    }
}

#[cfg(test)]
pub(super) fn classify_bind_host_for_test(host: &str) -> DaemonBindHostClass {
    classify_bind_host(host)
}
