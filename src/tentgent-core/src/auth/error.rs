use crate::auth::Provider;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("failed to initialize secure storage entry for {provider}: {message}")]
    KeychainEntry { provider: Provider, message: String },
    #[error("failed to access secure storage for {provider}: {message}")]
    Keychain { provider: Provider, message: String },
    #[error("failed to build HTTP client for key validation: {0}")]
    HttpClient(String),
}
