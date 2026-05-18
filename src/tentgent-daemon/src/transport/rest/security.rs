use axum::http::{header, HeaderMap};

pub const DAEMON_TOKEN_ENV_VAR: &str = "TENTGENT_DAEMON_TOKEN";

#[derive(Debug, Clone)]
pub struct DaemonSecurityConfig {
    token: Option<Vec<u8>>,
}

impl DaemonSecurityConfig {
    pub fn from_env() -> Self {
        Self::from_token_value(std::env::var(DAEMON_TOKEN_ENV_VAR).ok().as_deref())
    }

    fn from_token_value(value: Option<&str>) -> Self {
        let token = value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.as_bytes().to_vec());
        Self { token }
    }

    pub fn token_enabled(&self) -> bool {
        self.token.is_some()
    }

    pub fn authorize_headers(
        &self,
        headers: &HeaderMap,
    ) -> Result<(), DaemonTokenAuthorizationError> {
        let Some(expected) = &self.token else {
            return Err(DaemonTokenAuthorizationError::Disabled);
        };
        let authorization = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or(DaemonTokenAuthorizationError::Missing)?;
        let Some(actual) = authorization.strip_prefix("Bearer ") else {
            return Err(DaemonTokenAuthorizationError::Malformed);
        };
        if actual.is_empty() {
            return Err(DaemonTokenAuthorizationError::Malformed);
        }
        if constant_time_eq(actual.as_bytes(), expected) {
            Ok(())
        } else {
            Err(DaemonTokenAuthorizationError::Mismatch)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonTokenAuthorizationError {
    Disabled,
    Missing,
    Malformed,
    Mismatch,
}

pub fn daemon_token_enabled() -> bool {
    DaemonSecurityConfig::from_env().token_enabled()
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
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn blank_token_disables_security() {
        assert!(!DaemonSecurityConfig::from_token_value(None).token_enabled());
        assert!(!DaemonSecurityConfig::from_token_value(Some("   ")).token_enabled());
    }

    #[test]
    fn bearer_token_is_trimmed_and_authorized() {
        let security = DaemonSecurityConfig::from_token_value(Some("  secret  "));
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret"),
        );

        assert!(security.token_enabled());
        assert_eq!(security.authorize_headers(&headers), Ok(()));
    }

    #[test]
    fn bearer_token_rejects_uniform_failures() {
        let security = DaemonSecurityConfig::from_token_value(Some("secret"));
        let mut headers = HeaderMap::new();

        assert_eq!(
            security.authorize_headers(&headers),
            Err(DaemonTokenAuthorizationError::Missing)
        );

        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic secret"),
        );
        assert_eq!(
            security.authorize_headers(&headers),
            Err(DaemonTokenAuthorizationError::Malformed)
        );

        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer nope"),
        );
        assert_eq!(
            security.authorize_headers(&headers),
            Err(DaemonTokenAuthorizationError::Mismatch)
        );
    }
}
