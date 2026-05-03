use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use reqwest::Url;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::daemon::{DaemonProcessMetadata, DEFAULT_DAEMON_HOST, DEFAULT_DAEMON_PORT};

pub const CONFIG_FILE_NAME: &str = "config.toml";
pub const CONFIG_SCHEMA_VERSION: u32 = 1;
pub const DAEMON_URL_ENV_VAR: &str = "TENTGENT_DAEMON_URL";
pub const DEFAULT_DAEMON_URL: &str = "http://127.0.0.1:8790";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config `{path}`: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to write config `{path}`: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse config `{path}`: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("config `{path}` contains unsupported secret-like field `{field}`")]
    SecretField { path: PathBuf, field: String },
    #[error("daemon URL `{url}` from {origin} must be an absolute http or https URL")]
    InvalidDaemonUrl { origin: String, url: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TentgentConfig {
    pub schema_version: u32,
    #[serde(default)]
    pub tui: TuiConfig,
    #[serde(default)]
    pub daemon: DaemonConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TuiConfig {
    pub last_section: String,
    pub auto_start_daemon: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub url: Option<String>,
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
    pub config_error: Option<String>,
}

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

#[derive(Debug, Clone)]
pub struct DaemonUrlInputs<'a> {
    pub flag_url: Option<&'a str>,
    pub env_url: Option<&'a str>,
    pub config_url: Option<&'a str>,
    pub metadata: Option<&'a DaemonProcessMetadata>,
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

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            last_section: "status".to_string(),
            auto_start_daemon: false,
        }
    }
}

impl TentgentConfig {
    pub fn load(home: &Path) -> Result<Self, ConfigError> {
        let path = config_path(home);
        if !path.exists() {
            return Ok(Self::default());
        }

        let body = fs::read_to_string(&path).map_err(|source| ConfigError::Read {
            path: path.clone(),
            source,
        })?;
        reject_secret_like_fields(&path, &body)?;
        let mut config: Self = toml::from_str(&body).map_err(|error| ConfigError::Parse {
            path,
            message: error.to_string(),
        })?;
        if config.schema_version == 0 {
            config.schema_version = CONFIG_SCHEMA_VERSION;
        }
        Ok(config)
    }

    pub fn save(&self, home: &Path) -> Result<(), ConfigError> {
        let path = config_path(home);
        let dir = path.parent().unwrap_or(home);
        fs::create_dir_all(dir).map_err(|source| ConfigError::Write {
            path: dir.to_path_buf(),
            source,
        })?;

        let body = toml::to_string_pretty(self).map_err(|error| ConfigError::Parse {
            path: path.clone(),
            message: error.to_string(),
        })?;
        reject_secret_like_fields(&path, &body)?;

        let temp_path = path.with_extension("toml.tmp");
        {
            let mut file = File::create(&temp_path).map_err(|source| ConfigError::Write {
                path: temp_path.clone(),
                source,
            })?;
            file.write_all(body.as_bytes())
                .map_err(|source| ConfigError::Write {
                    path: temp_path.clone(),
                    source,
                })?;
            file.sync_all().map_err(|source| ConfigError::Write {
                path: temp_path.clone(),
                source,
            })?;
        }
        fs::rename(&temp_path, &path).map_err(|source| ConfigError::Write {
            path: path.clone(),
            source,
        })?;
        Ok(())
    }
}

pub fn config_path(home: &Path) -> PathBuf {
    home.join(CONFIG_FILE_NAME)
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
                config_error = Some(error.to_string());
            }
        }
    }

    if let Some(metadata) = inputs.metadata {
        return DaemonUrlResolution {
            url: daemon_url(&metadata.host, metadata.port),
            source: DaemonUrlSource::Metadata,
            config_error,
        };
    }

    DaemonUrlResolution {
        url: DEFAULT_DAEMON_URL.to_string(),
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

pub fn validate_daemon_url(url: &str, source: &str) -> Result<(), ConfigError> {
    let parsed = Url::parse(url).map_err(|_| ConfigError::InvalidDaemonUrl {
        origin: source.to_string(),
        url: url.to_string(),
    })?;
    let valid_scheme = matches!(parsed.scheme(), "http" | "https");
    if valid_scheme && parsed.host().is_some() {
        Ok(())
    } else {
        Err(ConfigError::InvalidDaemonUrl {
            origin: source.to_string(),
            url: url.to_string(),
        })
    }
}

pub fn daemon_url(host: &str, port: u16) -> String {
    format!("http://{}:{port}", host_for_url(host))
}

pub fn default_daemon_metadata_url() -> String {
    daemon_url(DEFAULT_DAEMON_HOST, DEFAULT_DAEMON_PORT)
}

fn clean(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn host_for_url(host: &str) -> String {
    let trimmed = host.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        trimmed.to_string()
    } else if trimmed.contains(':') {
        format!("[{trimmed}]")
    } else {
        trimmed.to_string()
    }
}

fn reject_secret_like_fields(path: &Path, body: &str) -> Result<(), ConfigError> {
    let value: toml::Value = toml::from_str(body).map_err(|error| ConfigError::Parse {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    if let Some(field) = find_secret_like_field(&value, None) {
        return Err(ConfigError::SecretField {
            path: path.to_path_buf(),
            field,
        });
    }
    Ok(())
}

fn find_secret_like_field(value: &toml::Value, prefix: Option<&str>) -> Option<String> {
    let table = value.as_table()?;
    for (key, value) in table {
        let path = match prefix {
            Some(prefix) => format!("{prefix}.{key}"),
            None => key.to_string(),
        };
        let lower = key.to_ascii_lowercase();
        if lower.contains("token") || lower.contains("secret") || lower.contains("api_key") {
            return Some(path);
        }
        if let Some(field) = find_secret_like_field(value, Some(&path)) {
            return Some(field);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn config_missing_file_returns_defaults() {
        let home = unique_home("missing");
        let config = TentgentConfig::load(&home).expect("load missing config");

        assert_eq!(config.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(config.tui.last_section, "status");
        assert!(!config.tui.auto_start_daemon);
        assert!(config.daemon.url.is_none());
    }

    #[test]
    fn config_save_writes_schema_daemon_and_tui_sections() {
        let home = unique_home("save");
        let config = TentgentConfig {
            daemon: DaemonConfig {
                url: Some("http://127.0.0.1:8790".to_string()),
            },
            ..TentgentConfig::default()
        };

        config.save(&home).expect("save config");
        let body = fs::read_to_string(config_path(&home)).expect("read config");

        assert!(body.contains("schema_version = 1"));
        assert!(body.contains("[tui]"));
        assert!(body.contains("[daemon]"));
        assert!(body.contains("url = \"http://127.0.0.1:8790\""));
    }

    #[test]
    fn config_unknown_fields_are_tolerated() {
        let home = unique_home("unknown");
        fs::create_dir_all(&home).expect("home");
        fs::write(
            config_path(&home),
            r#"
schema_version = 1
future = "ok"

[tui]
last_section = "settings"
auto_start_daemon = false
unknown = "ok"
"#,
        )
        .expect("write config");

        let config = TentgentConfig::load(&home).expect("load config");
        assert_eq!(config.tui.last_section, "settings");
    }

    #[test]
    fn config_rejects_secret_like_fields() {
        let home = unique_home("secret");
        fs::create_dir_all(&home).expect("home");
        fs::write(
            config_path(&home),
            r#"
schema_version = 1

[daemon]
token = "nope"
"#,
        )
        .expect("write config");

        let error = TentgentConfig::load(&home).expect_err("secret should fail");
        assert!(matches!(error, ConfigError::SecretField { .. }));
    }

    #[test]
    fn invalid_config_daemon_url_falls_back_to_metadata_or_default() {
        let metadata = DaemonProcessMetadata {
            pid: 1,
            host: "127.0.0.1".to_string(),
            port: 9000,
            started_at: "2026-05-02T00:00:00Z".to_string(),
        };
        let with_metadata = resolve_daemon_url(DaemonUrlInputs {
            flag_url: None,
            env_url: None,
            config_url: Some("not-a-url"),
            metadata: Some(&metadata),
        });
        assert_eq!(with_metadata.url, "http://127.0.0.1:9000");
        assert_eq!(with_metadata.source, DaemonUrlSource::Metadata);
        assert!(with_metadata.config_error.is_some());

        let defaulted = resolve_daemon_url(DaemonUrlInputs {
            flag_url: None,
            env_url: None,
            config_url: Some("not-a-url"),
            metadata: None,
        });
        assert_eq!(defaulted.url, DEFAULT_DAEMON_URL);
        assert_eq!(defaulted.source, DaemonUrlSource::Default);
        assert!(defaulted.config_error.is_some());
    }

    #[test]
    fn daemon_url_resolution_precedence_is_flag_env_config_metadata_default() {
        let metadata = DaemonProcessMetadata {
            pid: 1,
            host: "127.0.0.1".to_string(),
            port: 9000,
            started_at: "2026-05-02T00:00:00Z".to_string(),
        };
        let all = resolve_daemon_url(DaemonUrlInputs {
            flag_url: Some("http://flag:1"),
            env_url: Some("http://env:2"),
            config_url: Some("http://config:3"),
            metadata: Some(&metadata),
        });
        assert_eq!(all.source, DaemonUrlSource::Flag);
        assert_eq!(all.url, "http://flag:1");

        let env = resolve_daemon_url(DaemonUrlInputs {
            flag_url: None,
            env_url: Some("http://env:2"),
            config_url: Some("http://config:3"),
            metadata: Some(&metadata),
        });
        assert_eq!(env.source, DaemonUrlSource::Env);

        let config = resolve_daemon_url(DaemonUrlInputs {
            flag_url: None,
            env_url: None,
            config_url: Some("http://config:3"),
            metadata: Some(&metadata),
        });
        assert_eq!(config.source, DaemonUrlSource::Config);

        let metadata_only = resolve_daemon_url(DaemonUrlInputs {
            flag_url: None,
            env_url: None,
            config_url: None,
            metadata: Some(&metadata),
        });
        assert_eq!(metadata_only.source, DaemonUrlSource::Metadata);

        let defaulted = resolve_daemon_url(DaemonUrlInputs {
            flag_url: None,
            env_url: None,
            config_url: None,
            metadata: None,
        });
        assert_eq!(defaulted.source, DaemonUrlSource::Default);
    }

    #[test]
    fn token_resolution_precedence_is_flag_env_none() {
        let flag = resolve_daemon_token(Some(" flag "), Some("env"));
        assert_eq!(flag.source, DaemonTokenSource::Flag);
        assert_eq!(flag.token.as_deref(), Some("flag"));

        let env = resolve_daemon_token(None, Some(" env "));
        assert_eq!(env.source, DaemonTokenSource::Env);
        assert_eq!(env.token.as_deref(), Some("env"));

        let none = resolve_daemon_token(Some(" "), Some(""));
        assert_eq!(none.source, DaemonTokenSource::None);
        assert!(none.token.is_none());
    }

    fn unique_home(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("tentgent-config-{label}-{nanos}"))
    }
}
