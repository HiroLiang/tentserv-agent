use std::path::PathBuf;

use tentgent_kernel::features::daemon::domain::{DEFAULT_DAEMON_HOST, DEFAULT_DAEMON_PORT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonBootstrapConfig {
    pub home: Option<PathBuf>,
    pub logging: LoggingConfig,
    pub rest: RestConfig,
}

impl Default for DaemonBootstrapConfig {
    fn default() -> Self {
        Self {
            home: None,
            logging: LoggingConfig::default(),
            rest: RestConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggingConfig {
    pub enabled: bool,
    pub env_filter: Option<String>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            env_filter: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub allow_unsafe_bind: bool,
}

impl RestConfig {
    pub fn from_parts(enabled: bool, host: Option<String>, port: Option<u16>) -> Self {
        Self {
            enabled,
            host: host.unwrap_or_else(|| DEFAULT_DAEMON_HOST.to_string()),
            port: port.unwrap_or(DEFAULT_DAEMON_PORT),
            allow_unsafe_bind: false,
        }
    }

    pub fn with_allow_unsafe_bind(mut self, allow_unsafe_bind: bool) -> Self {
        self.allow_unsafe_bind = allow_unsafe_bind;
        self
    }
}

impl Default for RestConfig {
    fn default() -> Self {
        Self::from_parts(true, None, None)
    }
}
