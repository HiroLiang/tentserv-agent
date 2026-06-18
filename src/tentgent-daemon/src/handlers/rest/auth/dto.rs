use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct AuthProvidersResponse {
    pub providers: Vec<AuthProviderItem>,
}

#[derive(Debug, Serialize)]
pub struct AuthProviderResponse {
    pub provider: AuthProviderItem,
}

#[derive(Debug, Serialize)]
pub struct AuthProviderItem {
    pub provider: String,
    pub display_name: String,
    pub source_mode: String,
    pub env_present: bool,
    pub keychain_present: bool,
    pub effective_source: Option<String>,
    pub validation: AuthValidationItem,
}

#[derive(Debug, Serialize)]
pub struct AuthValidationItem {
    pub state: String,
    pub summary: String,
    pub detail: Option<String>,
}
