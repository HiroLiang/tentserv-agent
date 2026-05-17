use std::path::{Path, PathBuf};

use miette::{miette, IntoDiagnostic, Result};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use super::LoggingConfig;

const DEFAULT_LOG_FILTER: &str = "tentgent_daemon=info,tentgent_kernel=info";
const DAEMON_LOG_FILE_PREFIX: &str = "daemon.log";

pub struct LoggingRuntime {
    log_dir: Option<PathBuf>,
    file_prefix: Option<String>,
    _guard: Option<WorkerGuard>,
}

impl LoggingRuntime {
    pub fn disabled() -> Self {
        Self {
            log_dir: None,
            file_prefix: None,
            _guard: None,
        }
    }

    pub fn log_dir(&self) -> Option<&Path> {
        self.log_dir.as_deref()
    }

    pub fn file_prefix(&self) -> Option<&str> {
        self.file_prefix.as_deref()
    }
}

pub fn init_logging(config: &LoggingConfig, log_dir: &Path) -> Result<LoggingRuntime> {
    if !config.enabled {
        return Ok(LoggingRuntime::disabled());
    }

    std::fs::create_dir_all(log_dir).into_diagnostic()?;
    let filter = config.env_filter.as_deref().unwrap_or(DEFAULT_LOG_FILTER);
    let file_appender = tracing_appender::rolling::daily(log_dir, DAEMON_LOG_FILE_PREFIX);
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
    let console_layer = fmt::layer().compact().with_target(true);
    let file_layer = fmt::layer()
        .compact()
        .with_ansi(false)
        .with_target(true)
        .with_writer(file_writer);

    tracing_subscriber::registry()
        .with(EnvFilter::new(filter))
        .with(console_layer)
        .with(file_layer)
        .try_init()
        .map_err(|err| miette!("failed to initialize daemon logging: {err}"))?;

    Ok(LoggingRuntime {
        log_dir: Some(log_dir.to_path_buf()),
        file_prefix: Some(DAEMON_LOG_FILE_PREFIX.to_string()),
        _guard: Some(guard),
    })
}
