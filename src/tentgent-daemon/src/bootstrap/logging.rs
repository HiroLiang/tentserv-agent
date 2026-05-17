use miette::Result;
use tracing_subscriber::EnvFilter;

use super::LoggingConfig;

const DEFAULT_LOG_FILTER: &str = "tentgent_daemon=info,tentgent_kernel=info";

pub fn init_logging(config: &LoggingConfig) -> Result<()> {
    if !config.enabled {
        return Ok(());
    }

    let filter = config.env_filter.as_deref().unwrap_or(DEFAULT_LOG_FILTER);
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(filter))
        .try_init();

    Ok(())
}
