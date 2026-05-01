use std::{fmt, net::IpAddr};

use miette::{miette, Result};

pub const DAEMON_TOKEN_ENV_VAR: &str = "TENTGENT_DAEMON_TOKEN";

#[derive(Clone)]
pub struct DaemonSecurityConfig {
    token: Option<Vec<u8>>,
}

impl fmt::Debug for DaemonSecurityConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DaemonSecurityConfig")
            .field("token_enabled", &self.token_enabled())
            .finish()
    }
}

impl DaemonSecurityConfig {
    pub fn from_env() -> Self {
        Self::from_token_value(std::env::var(DAEMON_TOKEN_ENV_VAR).ok().as_deref())
    }

    pub fn from_token_value(value: Option<&str>) -> Self {
        let token = value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.as_bytes().to_vec());
        Self { token }
    }

    pub fn disabled() -> Self {
        Self { token: None }
    }

    pub fn token_enabled(&self) -> bool {
        self.token.is_some()
    }

    pub(crate) fn authorize_header(
        &self,
        authorization: Option<&str>,
    ) -> std::result::Result<(), AuthFailureClass> {
        let Some(expected) = &self.token else {
            return Ok(());
        };
        let authorization = authorization.ok_or(AuthFailureClass::Missing)?;
        let Some(actual) = authorization.strip_prefix("Bearer ") else {
            return Err(AuthFailureClass::Malformed);
        };
        if actual.is_empty() {
            return Err(AuthFailureClass::Malformed);
        }
        if constant_time_eq(actual.as_bytes(), expected) {
            Ok(())
        } else {
            Err(AuthFailureClass::Mismatch)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthFailureClass {
    Missing,
    Malformed,
    Mismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindHostClass {
    Loopback,
    Wildcard,
    NonLoopback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindSafetyReport {
    pub host_class: BindHostClass,
    pub warnings: Vec<String>,
}

pub fn check_bind_safety(
    host: &str,
    token_enabled: bool,
    allow_unsafe_bind: bool,
) -> Result<BindSafetyReport> {
    let host_class = classify_bind_host(host);
    let mut warnings = Vec::new();

    if host_class == BindHostClass::Loopback {
        return Ok(BindSafetyReport {
            host_class,
            warnings,
        });
    }

    if token_enabled {
        warnings.push(format!(
            "daemon is binding to unsafe host `{host}` with bearer token auth enabled; this local guard is not a public-service security model"
        ));
        return Ok(BindSafetyReport {
            host_class,
            warnings,
        });
    }

    if allow_unsafe_bind {
        warnings.push(format!(
            "daemon is binding to unsafe host `{host}` without daemon token auth because --allow-unsafe-bind was provided; this can expose daemon lifecycle and chat endpoints to your network"
        ));
        return Ok(BindSafetyReport {
            host_class,
            warnings,
        });
    }

    Err(miette!(
        "refusing to bind daemon to unsafe host `{host}` without `{}`; set the token or pass --allow-unsafe-bind for explicit local-network experiments",
        DAEMON_TOKEN_ENV_VAR
    ))
}

pub fn classify_bind_host(host: &str) -> BindHostClass {
    let host = host.trim();
    if host.eq_ignore_ascii_case("localhost") {
        return BindHostClass::Loopback;
    }

    let unbracketed = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);

    match unbracketed.parse::<IpAddr>() {
        Ok(address) if address.is_unspecified() => BindHostClass::Wildcard,
        Ok(address) if address.is_loopback() => BindHostClass::Loopback,
        Ok(_) => BindHostClass::NonLoopback,
        Err(_) => BindHostClass::NonLoopback,
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let max_len = left.len().max(right.len());
    let mut diff = left.len() ^ right.len();
    for index in 0..max_len {
        let left = left.get(index).copied().unwrap_or(0);
        let right = right.get(index).copied().unwrap_or(0);
        diff |= usize::from(left ^ right);
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_token_is_trimmed_and_blank_disables_auth() {
        assert!(!DaemonSecurityConfig::from_token_value(None).token_enabled());
        assert!(!DaemonSecurityConfig::from_token_value(Some("   ")).token_enabled());

        let security = DaemonSecurityConfig::from_token_value(Some("  secret  "));
        assert!(security.token_enabled());
        assert!(security.authorize_header(Some("Bearer secret")).is_ok());
    }

    #[test]
    fn daemon_token_authorization_uses_uniform_failure_classes() {
        let security = DaemonSecurityConfig::from_token_value(Some("secret"));

        assert_eq!(
            security.authorize_header(None),
            Err(AuthFailureClass::Missing)
        );
        assert_eq!(
            security.authorize_header(Some("Basic secret")),
            Err(AuthFailureClass::Malformed)
        );
        assert_eq!(
            security.authorize_header(Some("Bearer wrong")),
            Err(AuthFailureClass::Mismatch)
        );
        assert!(security.authorize_header(Some("Bearer secret")).is_ok());
    }

    #[test]
    fn bind_host_classification_covers_loopback_wildcard_and_unsafe_hosts() {
        assert_eq!(classify_bind_host("localhost"), BindHostClass::Loopback);
        assert_eq!(classify_bind_host("127.0.0.1"), BindHostClass::Loopback);
        assert_eq!(classify_bind_host("::1"), BindHostClass::Loopback);
        assert_eq!(classify_bind_host("[::1]"), BindHostClass::Loopback);
        assert_eq!(classify_bind_host("0.0.0.0"), BindHostClass::Wildcard);
        assert_eq!(classify_bind_host("::"), BindHostClass::Wildcard);
        assert_eq!(
            classify_bind_host("192.168.1.10"),
            BindHostClass::NonLoopback
        );
        assert_eq!(
            classify_bind_host("agent.local"),
            BindHostClass::NonLoopback
        );
    }

    #[test]
    fn bind_safety_requires_token_or_explicit_unsafe_for_non_loopback() {
        assert!(check_bind_safety("127.0.0.1", false, false).is_ok());
        assert!(check_bind_safety("127.0.0.1", true, false).is_ok());
        assert!(check_bind_safety("0.0.0.0", false, false).is_err());

        let token_report = check_bind_safety("0.0.0.0", true, false).expect("token");
        assert_eq!(token_report.host_class, BindHostClass::Wildcard);
        assert_eq!(token_report.warnings.len(), 1);

        let unsafe_report = check_bind_safety("192.168.1.10", false, true).expect("unsafe");
        assert_eq!(unsafe_report.host_class, BindHostClass::NonLoopback);
        assert_eq!(unsafe_report.warnings.len(), 1);
    }
}
