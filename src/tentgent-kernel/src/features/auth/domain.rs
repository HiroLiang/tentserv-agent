//! Provider auth domain types and pure access policy rules.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

pub const AUTH_SERVICE: &str = "com.tentserv.tentgent.auth";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
    HuggingFace,
    OpenAI,
    Anthropic,
    Gemini,
}

impl Provider {
    pub const ALL: [Self; 4] = [
        Self::HuggingFace,
        Self::OpenAI,
        Self::Anthropic,
        Self::Gemini,
    ];

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::HuggingFace => "Hugging Face",
            Self::OpenAI => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::Gemini => "Gemini",
        }
    }

    pub const fn cli_name(self) -> &'static str {
        match self {
            Self::HuggingFace => "hf",
            Self::OpenAI => "openai",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
        }
    }

    pub const fn env_var(self) -> &'static str {
        match self {
            Self::HuggingFace => "HF_TOKEN",
            Self::OpenAI => "OPENAI_API_KEY",
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::Gemini => "GEMINI_API_KEY",
        }
    }

    pub const fn keychain_account(self) -> &'static str {
        match self {
            Self::HuggingFace => "huggingface",
            Self::OpenAI => "openai",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
        }
    }
}

impl std::fmt::Display for Provider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.display_name())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthSecretSource {
    Env,
    Keychain,
    Prompt,
    Request,
}

impl std::fmt::Display for AuthSecretSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Env => formatter.write_str(".env/env"),
            Self::Keychain => formatter.write_str("keychain"),
            Self::Prompt => formatter.write_str("prompt"),
            Self::Request => formatter.write_str("request"),
        }
    }
}

pub(crate) fn normalize_secret_value(mut secret: String) -> Option<String> {
    let trimmed = secret.trim();
    if trimmed.is_empty() {
        secret.zeroize();
        None
    } else if trimmed.len() == secret.len() {
        Some(secret)
    } else {
        let trimmed = trimmed.to_string();
        secret.zeroize();
        Some(trimmed)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct AuthSecretMaterial {
    pub provider: Provider,
    pub source: AuthSecretSource,
    pub secret: Zeroizing<String>,
}

impl std::fmt::Debug for AuthSecretMaterial {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthSecretMaterial")
            .field("provider", &self.provider)
            .field("source", &self.source)
            .field("secret", &"<redacted>")
            .finish()
    }
}

impl AuthSecretMaterial {
    pub fn new(provider: Provider, source: AuthSecretSource, secret: impl Into<String>) -> Self {
        Self {
            provider,
            source,
            secret: Zeroizing::new(secret.into()),
        }
    }

    pub fn normalized(
        provider: Provider,
        source: AuthSecretSource,
        secret: impl Into<String>,
    ) -> Option<Self> {
        normalize_secret_value(secret.into()).map(|secret| Self::new(provider, source, secret))
    }

    pub fn secret(&self) -> &str {
        self.secret.as_str()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct AuthEnvSecretMaterial {
    pub provider: Provider,
    pub env_var: String,
    pub origin: AuthEnvSecretOrigin,
    pub secret: Zeroizing<String>,
}

impl std::fmt::Debug for AuthEnvSecretMaterial {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthEnvSecretMaterial")
            .field("provider", &self.provider)
            .field("env_var", &self.env_var)
            .field("origin", &self.origin)
            .field("secret", &"<redacted>")
            .finish()
    }
}

impl AuthEnvSecretMaterial {
    pub fn new(
        provider: Provider,
        env_var: impl Into<String>,
        origin: AuthEnvSecretOrigin,
        secret: impl Into<String>,
    ) -> Self {
        Self {
            provider,
            env_var: env_var.into(),
            origin,
            secret: Zeroizing::new(secret.into()),
        }
    }

    pub fn secret(&self) -> &str {
        self.secret.as_str()
    }

    pub fn into_secret_material(self) -> AuthSecretMaterial {
        AuthSecretMaterial {
            provider: self.provider,
            source: AuthSecretSource::Env,
            secret: self.secret,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthEnvSecretOrigin {
    ProcessEnv,
    DotenvFile { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthEnvLoadPolicy {
    ProcessOnly,
    CwdDotenvOverride,
    ExplicitDotenvOverride { path: PathBuf },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeychainPresence {
    Present,
    Absent,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthKeyStatus {
    pub provider: Provider,
    pub env_present: bool,
    pub keychain_presence: KeychainPresence,
    pub effective_source: Option<AuthSecretSource>,
    pub validation: AuthValidationState,
}

impl AuthKeyStatus {
    pub fn local(
        provider: Provider,
        env_present: bool,
        keychain_presence: KeychainPresence,
    ) -> Self {
        Self {
            provider,
            env_present,
            keychain_presence,
            effective_source: effective_source(env_present, keychain_presence),
            validation: AuthValidationState::NotChecked,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthValidationState {
    Missing,
    NotChecked,
    Verified,
    Invalid { reason: String },
    Unknown { reason: String },
}

impl AuthValidationState {
    pub const fn summary(&self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::NotChecked => "not checked",
            Self::Verified => "verified",
            Self::Invalid { .. } => "invalid",
            Self::Unknown { .. } => "unknown",
        }
    }

    pub fn detail(&self) -> Option<&str> {
        match self {
            Self::Invalid { reason } | Self::Unknown { reason } => Some(reason.as_str()),
            Self::Missing | Self::NotChecked | Self::Verified => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthSecretReadIntent {
    StatusOnly,
    ResolveForUse,
    ResolveAndValidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthSecretCacheScope {
    None,
    ProcessSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthSecretAccessPolicy {
    pub read_intent: AuthSecretReadIntent,
    pub cache_scope: AuthSecretCacheScope,
}

impl AuthSecretAccessPolicy {
    pub const fn status_only() -> Self {
        Self {
            read_intent: AuthSecretReadIntent::StatusOnly,
            cache_scope: AuthSecretCacheScope::None,
        }
    }

    pub const fn resolve_for_use() -> Self {
        Self {
            read_intent: AuthSecretReadIntent::ResolveForUse,
            cache_scope: AuthSecretCacheScope::ProcessSession,
        }
    }

    pub const fn resolve_and_validate() -> Self {
        Self {
            read_intent: AuthSecretReadIntent::ResolveAndValidate,
            cache_scope: AuthSecretCacheScope::ProcessSession,
        }
    }

    pub const fn may_read_keychain_secret(self) -> bool {
        matches!(
            self.read_intent,
            AuthSecretReadIntent::ResolveForUse | AuthSecretReadIntent::ResolveAndValidate
        )
    }

    pub const fn should_cache_for_process(self) -> bool {
        matches!(self.cache_scope, AuthSecretCacheScope::ProcessSession)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthProviderPreference {
    pub provider: Provider,
    pub enabled: bool,
    pub access_policy: AuthSecretAccessPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthProviderMetadata {
    pub provider: Provider,
    pub keychain_presence: KeychainPresence,
    pub last_updated_at: Option<String>,
    pub last_validated_at: Option<String>,
    pub validation: AuthValidationState,
}

impl AuthProviderPreference {
    pub const fn default_for(provider: Provider) -> Self {
        Self {
            provider,
            enabled: true,
            access_policy: AuthSecretAccessPolicy::resolve_for_use(),
        }
    }
}

pub fn effective_source(
    env_present: bool,
    keychain_presence: KeychainPresence,
) -> Option<AuthSecretSource> {
    if env_present {
        Some(AuthSecretSource::Env)
    } else if keychain_presence == KeychainPresence::Present {
        Some(AuthSecretSource::Keychain)
    } else {
        None
    }
}
