use std::sync::OnceLock;

use super::provider::Provider;

static DOTENV_LOADED: OnceLock<()> = OnceLock::new();

pub(crate) fn ensure_env_loaded() {
    DOTENV_LOADED.get_or_init(|| {
        let _ = dotenvy::dotenv_override();
    });
}

pub(crate) fn read_provider_env(provider: Provider) -> Option<String> {
    ensure_env_loaded();

    let value = std::env::var(provider.env_var()).ok()?;
    let trimmed = value.trim().to_owned();

    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
