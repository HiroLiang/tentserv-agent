//! User config schema and pure resolution rules.

use serde::{Deserialize, Serialize};

use crate::foundation::net::http_url_from_host_port;

pub const CONFIG_FILE_NAME: &str = "config.toml";
pub const CONFIG_SCHEMA_VERSION: u32 = 1;
pub const DAEMON_URL_ENV_VAR: &str = "TENTGENT_DAEMON_URL";
pub const DEFAULT_DAEMON_HOST: &str = "127.0.0.1";
pub const DEFAULT_DAEMON_PORT: u16 = 8790;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TentgentConfig {
    pub schema_version: u32,
    #[serde(default)]
    pub tui: TuiConfig,
    #[serde(default)]
    pub daemon: DaemonConfig,
}

impl Default for TentgentConfig {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            tui: TuiConfig::default(),
            daemon: DaemonConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TuiConfig {
    pub last_section: String,
    pub auto_start_daemon: bool,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            last_section: "status".to_string(),
            auto_start_daemon: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonEndpoint {
    pub host: String,
    pub port: u16,
}

impl Default for DaemonEndpoint {
    fn default() -> Self {
        Self {
            host: DEFAULT_DAEMON_HOST.to_string(),
            port: DEFAULT_DAEMON_PORT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonUrlSource {
    Flag,
    Env,
    Config,
    Metadata,
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonUrlResolution {
    pub url: String,
    pub source: DaemonUrlSource,
    pub config_error: Option<DaemonUrlValidationError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonUrlInputs<'a> {
    pub flag_url: Option<&'a str>,
    pub env_url: Option<&'a str>,
    pub config_url: Option<&'a str>,
    pub metadata_endpoint: Option<&'a DaemonEndpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonUrlValidationError {
    pub origin: String,
    pub url: String,
}

impl std::fmt::Display for DaemonUrlValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "daemon URL `{}` from {} must be an absolute http or https URL",
            self.url, self.origin
        )
    }
}

impl std::error::Error for DaemonUrlValidationError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonTokenSource {
    Flag,
    Env,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonTokenResolution {
    pub token: Option<String>,
    pub source: DaemonTokenSource,
}

pub fn resolve_daemon_url(inputs: DaemonUrlInputs<'_>) -> DaemonUrlResolution {
    if let Some(url) = clean(inputs.flag_url) {
        return DaemonUrlResolution {
            url: url.to_string(),
            source: DaemonUrlSource::Flag,
            config_error: None,
        };
    }

    if let Some(url) = clean(inputs.env_url) {
        return DaemonUrlResolution {
            url: url.to_string(),
            source: DaemonUrlSource::Env,
            config_error: None,
        };
    }

    let mut config_error = None;
    if let Some(url) = clean(inputs.config_url) {
        match validate_daemon_url(url, "config") {
            Ok(()) => {
                return DaemonUrlResolution {
                    url: url.to_string(),
                    source: DaemonUrlSource::Config,
                    config_error: None,
                };
            }
            Err(error) => {
                config_error = Some(error);
            }
        }
    }

    if let Some(endpoint) = inputs.metadata_endpoint {
        return DaemonUrlResolution {
            url: daemon_url(&endpoint.host, endpoint.port),
            source: DaemonUrlSource::Metadata,
            config_error,
        };
    }

    DaemonUrlResolution {
        url: default_daemon_url(),
        source: DaemonUrlSource::Default,
        config_error,
    }
}

pub fn resolve_daemon_token(
    flag_token: Option<&str>,
    env_token: Option<&str>,
) -> DaemonTokenResolution {
    if let Some(token) = clean(flag_token) {
        return DaemonTokenResolution {
            token: Some(token.to_string()),
            source: DaemonTokenSource::Flag,
        };
    }

    if let Some(token) = clean(env_token) {
        return DaemonTokenResolution {
            token: Some(token.to_string()),
            source: DaemonTokenSource::Env,
        };
    }

    DaemonTokenResolution {
        token: None,
        source: DaemonTokenSource::None,
    }
}

pub fn validate_daemon_url(url: &str, origin: &str) -> Result<(), DaemonUrlValidationError> {
    let trimmed = url.trim();
    let Some(rest) = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
    else {
        return Err(invalid_daemon_url(origin, url));
    };

    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .filter(|authority| !authority.is_empty())
        .ok_or_else(|| invalid_daemon_url(origin, url))?;

    if authority_has_host(authority) {
        Ok(())
    } else {
        Err(invalid_daemon_url(origin, url))
    }
}

pub fn daemon_url(host: &str, port: u16) -> String {
    http_url_from_host_port(host, port)
}

pub fn default_daemon_url() -> String {
    daemon_url(DEFAULT_DAEMON_HOST, DEFAULT_DAEMON_PORT)
}

pub fn is_secret_like_config_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("token") || lower.contains("secret") || lower.contains("api_key")
}

fn clean(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn authority_has_host(authority: &str) -> bool {
    let without_userinfo = authority.rsplit('@').next().unwrap_or(authority);
    if let Some(rest) = without_userinfo.strip_prefix('[') {
        return rest
            .split_once(']')
            .map(|(host, _)| !host.trim().is_empty())
            .unwrap_or(false);
    }

    let host = without_userinfo
        .split_once(':')
        .map(|(host, _)| host)
        .unwrap_or(without_userinfo);
    !host.trim().is_empty()
}

fn invalid_daemon_url(origin: &str, url: &str) -> DaemonUrlValidationError {
    DaemonUrlValidationError {
        origin: origin.to_string(),
        url: url.to_string(),
    }
}
