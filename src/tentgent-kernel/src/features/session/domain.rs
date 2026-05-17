//! Session identity, metadata, transcript, and context domain types.

use std::path::PathBuf;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

pub const SESSION_REF_HEX_LENGTH: usize = 64;
pub const MIN_SESSION_REF_LENGTH: usize = 12;
pub const SHORT_SESSION_REF_LENGTH: usize = 12;

pub const SESSION_DIRNAME: &str = "sessions";
pub const SESSION_METADATA_FILENAME: &str = "session.toml";
pub const SESSION_MESSAGES_FILENAME: &str = "messages.jsonl";
pub const SESSION_LOCK_FILENAME: &str = "session.lock";
pub const SESSION_CREATE_LOCK_FILENAME: &str = ".sessions.lock";

pub const SESSION_SCHEMA: &str = "tentgent.session.v1";
pub const SESSION_MESSAGE_SCHEMA: &str = "tentgent.session.message.v1";

pub const MAX_MESSAGES_PER_APPEND: usize = 100;
pub const MAX_MESSAGE_CONTENT_BYTES: usize = 1024 * 1024;
pub const MAX_MESSAGE_METADATA_BYTES: usize = 64 * 1024;
pub const DEFAULT_SESSION_CONTEXT_MESSAGES: usize = 50;
pub const MAX_SESSION_CONTEXT_MESSAGES: usize = 1000;
pub const MAX_SESSION_CONTEXT_BYTES: usize = 1024 * 1024;
pub const SESSION_MESSAGE_CAP: usize = 50;
pub const MAX_COMPACT_INSTRUCTIONS_BYTES: usize = 16 * 1024;
pub const ROLLING_CONTEXT_HIGH_WATER_MESSAGES: usize = 20;
pub const ROLLING_CONTEXT_LOW_WATER_RECENT_MESSAGES: usize = 10;
pub const ROLLING_CONTEXT_HIGH_WATER_BYTES: usize = 128 * 1024;
pub const ROLLING_CONTEXT_LOW_WATER_BYTES: usize = 64 * 1024;
pub const ROLLING_CONTEXT_MAX_SUMMARY_BYTES: usize = 32 * 1024;
pub const MAX_SESSION_TAGS: usize = 32;
pub const MAX_SESSION_TAG_CHARS: usize = 64;
pub const SESSION_SUMMARY_METADATA_KIND: &str = "session_summary";
pub const ROLLING_CONTEXT_SUMMARY_SCOPE: &str = "rolling_context";
pub const ROLLING_CONTEXT_SUMMARY_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionRef(String);

impl SessionRef {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, SessionRefParseError> {
        let normalized = normalize_hex_ref(value.as_ref())?;
        if normalized.len() < MIN_SESSION_REF_LENGTH {
            return Err(SessionRefParseError::TooShort {
                actual: normalized.len(),
            });
        }
        if normalized.len() > SESSION_REF_HEX_LENGTH {
            return Err(SessionRefParseError::TooLong {
                actual: normalized.len(),
            });
        }

        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn short_ref(&self) -> &str {
        &self.0[..SHORT_SESSION_REF_LENGTH]
    }

    pub fn is_generated_length(&self) -> bool {
        self.0.len() == SESSION_REF_HEX_LENGTH
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for SessionRef {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for SessionRef {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<&str> for SessionRef {
    type Error = SessionRefParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl Serialize for SessionRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SessionRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionRefSelector(String);

impl SessionRefSelector {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, SessionRefParseError> {
        let normalized = normalize_hex_ref(value.as_ref())?;
        if normalized.len() > SESSION_REF_HEX_LENGTH {
            return Err(SessionRefParseError::TooLong {
                actual: normalized.len(),
            });
        }

        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_full_ref(&self) -> bool {
        self.0.len() == SESSION_REF_HEX_LENGTH
    }
}

impl AsRef<str> for SessionRefSelector {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for SessionRefSelector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for SessionRefSelector {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SessionRefSelector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SessionRefParseError {
    #[error("session reference is empty")]
    Empty,
    #[error("session reference must contain at least 12 hexadecimal characters; got {actual}")]
    TooShort { actual: usize },
    #[error("session reference must contain at most 64 hexadecimal characters; got {actual}")]
    TooLong { actual: usize },
    #[error("session reference must contain only hexadecimal characters")]
    NonHex,
}

fn normalize_hex_ref(value: &str) -> Result<String, SessionRefParseError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(SessionRefParseError::Empty);
    }

    if !trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(SessionRefParseError::NonHex);
    }

    Ok(trimmed.to_ascii_lowercase())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

impl SessionMessageRole {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, SessionMessageRoleParseError> {
        let normalized = value.as_ref().trim().to_ascii_lowercase();
        match normalized.as_str() {
            "" => Err(SessionMessageRoleParseError::Empty),
            "system" => Ok(Self::System),
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            "tool" => Ok(Self::Tool),
            _ => Err(SessionMessageRoleParseError::Unsupported { value: normalized }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }

    pub const fn is_chat_context_supported(self) -> bool {
        !matches!(self, Self::Tool)
    }
}

impl std::fmt::Display for SessionMessageRole {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SessionMessageRoleParseError {
    #[error("session message role is empty")]
    Empty,
    #[error("unsupported session message role `{value}`")]
    Unsupported { value: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub schema: String,
    pub session_ref: SessionRef,
    pub short_ref: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    pub default_server_ref: Option<String>,
    pub adapter_ref: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl SessionMetadata {
    pub fn new(
        session_ref: SessionRef,
        created_at: impl Into<String>,
        updated_at: impl Into<String>,
    ) -> Self {
        let short_ref = session_ref.short_ref().to_string();
        Self {
            schema: SESSION_SCHEMA.to_string(),
            session_ref,
            short_ref,
            title: None,
            created_at: created_at.into(),
            updated_at: updated_at.into(),
            message_count: 0,
            default_server_ref: None,
            adapter_ref: None,
            tags: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub metadata: SessionMetadata,
    pub store_path: PathBuf,
    pub messages_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInspection {
    pub metadata: SessionMetadata,
    pub store_path: PathBuf,
    pub metadata_path: PathBuf,
    pub messages_path: PathBuf,
    pub warnings: Vec<SessionWarning>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMessage {
    pub index: usize,
    pub role: SessionMessageRole,
    pub content: String,
    pub created_at: String,
    pub server_ref: Option<String>,
    pub adapter_ref: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredSessionMessage {
    pub schema: String,
    pub role: SessionMessageRole,
    pub content: String,
    pub created_at: String,
    pub server_ref: Option<String>,
    pub adapter_ref: Option<String>,
    pub metadata: Value,
}

impl StoredSessionMessage {
    pub fn new(
        role: SessionMessageRole,
        content: impl Into<String>,
        created_at: impl Into<String>,
    ) -> Self {
        Self {
            schema: SESSION_MESSAGE_SCHEMA.to_string(),
            role,
            content: content.into(),
            created_at: created_at.into(),
            server_ref: None,
            adapter_ref: None,
            metadata: Value::Object(Default::default()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionMessages {
    pub session_ref: SessionRef,
    pub short_ref: String,
    pub messages: Vec<SessionMessage>,
    pub tail: usize,
    pub total_messages: usize,
    pub truncated: bool,
    pub warnings: Vec<SessionWarning>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionCreateRequest {
    pub title: Option<String>,
    pub default_server_ref: Option<String>,
    pub adapter_ref: Option<String>,
    pub tags: Vec<String>,
    pub messages: Vec<SessionMessageInput>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionUpdateRequest {
    pub title: SessionOptionalStringPatch,
    pub default_server_ref: SessionOptionalStringPatch,
    pub adapter_ref: SessionOptionalStringPatch,
    pub tags: Option<Vec<String>>,
}

impl Default for SessionUpdateRequest {
    fn default() -> Self {
        Self {
            title: SessionOptionalStringPatch::Unchanged,
            default_server_ref: SessionOptionalStringPatch::Unchanged,
            adapter_ref: SessionOptionalStringPatch::Unchanged,
            tags: None,
        }
    }
}

impl SessionUpdateRequest {
    pub fn is_empty(&self) -> bool {
        matches!(self.title, SessionOptionalStringPatch::Unchanged)
            && matches!(
                self.default_server_ref,
                SessionOptionalStringPatch::Unchanged
            )
            && matches!(self.adapter_ref, SessionOptionalStringPatch::Unchanged)
            && self.tags.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionOptionalStringPatch {
    Unchanged,
    Clear,
    Set(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionMessageInput {
    pub role: SessionMessageRole,
    pub content: String,
    pub server_ref: Option<String>,
    pub adapter_ref: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionAppendedMessage {
    pub index: usize,
    pub role: SessionMessageRole,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionAppendOutcome {
    pub metadata: SessionMetadata,
    pub store_path: PathBuf,
    pub appended: Vec<SessionAppendedMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRemovalOutcome {
    pub inspection: SessionInspection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionChatContextMessage {
    pub role: SessionMessageRole,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCompactionInput {
    pub prompt_messages: Vec<SessionChatContextMessage>,
    pub source_message_count: usize,
    pub replaced_message_count: usize,
    pub source_start_index: usize,
    pub source_end_index: usize,
    pub kept_recent_messages: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRequestContextSummaryInput {
    pub prompt_messages: Vec<SessionChatContextMessage>,
    pub source_message_count: usize,
    pub summarized_message_count: usize,
    pub kept_recent_messages: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCompactionSummary {
    pub content: String,
    pub server_ref: Option<String>,
    pub model_ref: Option<String>,
    pub provider_model: Option<String>,
    pub adapter_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCompactionOutcome {
    pub metadata: SessionMetadata,
    pub store_path: PathBuf,
    pub compacted: bool,
    pub source_message_count: usize,
    pub replaced_message_count: usize,
    pub kept_recent_messages: usize,
    pub summary_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStoreLayout {
    pub home_dir: PathBuf,
    pub sessions_dir: PathBuf,
}

impl SessionStoreLayout {
    pub fn from_home_dir(home_dir: impl Into<PathBuf>) -> Self {
        let home_dir = home_dir.into();
        Self {
            sessions_dir: home_dir.join(SESSION_DIRNAME),
            home_dir,
        }
    }

    pub fn from_sessions_dir(sessions_dir: impl Into<PathBuf>) -> Self {
        let sessions_dir = sessions_dir.into();
        let home_dir = sessions_dir
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        Self {
            home_dir,
            sessions_dir,
        }
    }

    pub fn session_dir(&self, session_ref: &SessionRef) -> PathBuf {
        self.sessions_dir.join(session_ref.as_str())
    }

    pub fn metadata_path(&self, session_ref: &SessionRef) -> PathBuf {
        self.session_dir(session_ref)
            .join(SESSION_METADATA_FILENAME)
    }

    pub fn messages_path(&self, session_ref: &SessionRef) -> PathBuf {
        self.session_dir(session_ref)
            .join(SESSION_MESSAGES_FILENAME)
    }

    pub fn create_lock_path(&self) -> PathBuf {
        self.sessions_dir.join(SESSION_CREATE_LOCK_FILENAME)
    }

    pub fn session_lock_path(&self, session_ref: &SessionRef) -> PathBuf {
        self.session_dir(session_ref).join(SESSION_LOCK_FILENAME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ref_accepts_generated_and_legacy_lengths() {
        let generated = SessionRef::parse("A".repeat(SESSION_REF_HEX_LENGTH)).expect("generated");
        assert_eq!(generated.as_str(), "a".repeat(SESSION_REF_HEX_LENGTH));
        assert_eq!(generated.short_ref(), "aaaaaaaaaaaa");
        assert!(generated.is_generated_length());

        let legacy = SessionRef::parse("b".repeat(MIN_SESSION_REF_LENGTH)).expect("legacy");
        assert_eq!(legacy.short_ref(), "bbbbbbbbbbbb");
        assert!(!legacy.is_generated_length());
    }

    #[test]
    fn session_ref_rejects_invalid_values() {
        assert_eq!(SessionRef::parse(""), Err(SessionRefParseError::Empty));
        assert_eq!(
            SessionRef::parse("a".repeat(MIN_SESSION_REF_LENGTH - 1)),
            Err(SessionRefParseError::TooShort {
                actual: MIN_SESSION_REF_LENGTH - 1
            })
        );
        assert_eq!(
            SessionRef::parse("a".repeat(SESSION_REF_HEX_LENGTH + 1)),
            Err(SessionRefParseError::TooLong {
                actual: SESSION_REF_HEX_LENGTH + 1
            })
        );
        assert_eq!(
            SessionRef::parse("not-a-session-ref"),
            Err(SessionRefParseError::NonHex)
        );
    }

    #[test]
    fn session_ref_selector_accepts_prefixes() {
        let selector = SessionRefSelector::parse("ABC123").expect("selector");
        assert_eq!(selector.as_str(), "abc123");
        assert!(!selector.is_full_ref());
    }

    #[test]
    fn session_message_role_parses_known_roles() {
        assert_eq!(
            SessionMessageRole::parse("assistant"),
            Ok(SessionMessageRole::Assistant)
        );
        assert_eq!(
            SessionMessageRole::parse("tool"),
            Ok(SessionMessageRole::Tool)
        );
        assert!(!SessionMessageRole::Tool.is_chat_context_supported());
    }

    #[test]
    fn session_update_request_default_is_empty() {
        assert!(SessionUpdateRequest::default().is_empty());
        assert!(!SessionUpdateRequest {
            title: SessionOptionalStringPatch::Set("Planning".to_string()),
            ..SessionUpdateRequest::default()
        }
        .is_empty());
    }

    #[test]
    fn session_store_layout_derives_standard_paths() {
        let session_ref = SessionRef::parse("c".repeat(SESSION_REF_HEX_LENGTH)).expect("ref");
        let layout = SessionStoreLayout::from_home_dir("/tmp/tentgent-home");

        assert_eq!(
            layout.session_dir(&session_ref),
            PathBuf::from("/tmp/tentgent-home/sessions").join(session_ref.as_str())
        );
        assert_eq!(
            layout.metadata_path(&session_ref),
            PathBuf::from("/tmp/tentgent-home/sessions")
                .join(session_ref.as_str())
                .join(SESSION_METADATA_FILENAME)
        );
        assert_eq!(
            layout.messages_path(&session_ref),
            PathBuf::from("/tmp/tentgent-home/sessions")
                .join(session_ref.as_str())
                .join(SESSION_MESSAGES_FILENAME)
        );
    }
}
