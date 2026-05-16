//! Provider auth domain types and pure access policy rules.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub const AUTH_SERVICE: &str = "com.tentserv.tentgent.auth";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
    HuggingFace,
    OpenAI,
    Anthropic,
}

impl Provider {
    pub const ALL: [Self; 3] = [Self::HuggingFace, Self::OpenAI, Self::Anthropic];

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::HuggingFace => "Hugging Face",
            Self::OpenAI => "OpenAI",
            Self::Anthropic => "Anthropic",
        }
    }

    pub const fn cli_name(self) -> &'static str {
        match self {
            Self::HuggingFace => "hf",
            Self::OpenAI => "openai",
            Self::Anthropic => "anthropic",
        }
    }

    pub const fn env_var(self) -> &'static str {
        match self {
            Self::HuggingFace => "HF_TOKEN",
            Self::OpenAI => "OPENAI_API_KEY",
            Self::Anthropic => "ANTHROPIC_API_KEY",
        }
    }

    pub const fn keychain_account(self) -> &'static str {
        match self {
            Self::HuggingFace => "huggingface",
            Self::OpenAI => "openai",
            Self::Anthropic => "anthropic",
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
}

impl std::fmt::Display for AuthSecretSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Env => formatter.write_str(".env/env"),
            Self::Keychain => formatter.write_str("keychain"),
        }
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
pub enum KeychainPromptPreference {
    SystemDefault,
    PreferBiometric,
    PasswordAllowed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeychainPromptPlan {
    pub requested: KeychainPromptPreference,
    pub effective: KeychainPromptPreference,
    pub biometric_support: KeychainBiometricSupport,
}

impl KeychainPromptPlan {
    pub const fn honors_biometric_preference(&self) -> bool {
        matches!(self.biometric_support, KeychainBiometricSupport::Supported)
            && matches!(self.effective, KeychainPromptPreference::PreferBiometric)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeychainBiometricSupport {
    Supported,
    BackendUnsupported { reason: String },
    PlatformUnsupported { reason: String },
    Unknown { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthSecretCacheScope {
    None,
    ProcessSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthSecretAccessPolicy {
    pub read_intent: AuthSecretReadIntent,
    pub keychain_prompt: KeychainPromptPreference,
    pub cache_scope: AuthSecretCacheScope,
}

impl AuthSecretAccessPolicy {
    pub const fn non_prompting_status() -> Self {
        Self {
            read_intent: AuthSecretReadIntent::StatusOnly,
            keychain_prompt: KeychainPromptPreference::SystemDefault,
            cache_scope: AuthSecretCacheScope::None,
        }
    }

    pub const fn cli_secret_use() -> Self {
        Self {
            read_intent: AuthSecretReadIntent::ResolveForUse,
            keychain_prompt: KeychainPromptPreference::PreferBiometric,
            cache_scope: AuthSecretCacheScope::ProcessSession,
        }
    }

    pub const fn cli_secret_validation() -> Self {
        Self {
            read_intent: AuthSecretReadIntent::ResolveAndValidate,
            keychain_prompt: KeychainPromptPreference::PreferBiometric,
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
            access_policy: AuthSecretAccessPolicy::cli_secret_use(),
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
