use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use serde::Deserialize;
use serde_json::Value;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::{
    error::SessionError,
    store::{
        read_session_metadata, SessionMetadata, SessionStorePaths, SessionWarning,
        SESSION_MESSAGE_SCHEMA, SESSION_SCHEMA,
    },
};

const MESSAGES_MISSING_WARNING: &str = "messages_missing";
const MESSAGE_COUNT_MISMATCH_WARNING: &str = "message_count_mismatch";

#[derive(Debug, Clone)]
pub struct SessionManager {
    paths: SessionStorePaths,
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub metadata: SessionMetadata,
    pub store_path: PathBuf,
    pub messages_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct SessionInspection {
    pub metadata: SessionMetadata,
    pub store_path: PathBuf,
    pub metadata_path: PathBuf,
    pub messages_path: PathBuf,
    pub warnings: Vec<SessionWarning>,
}

#[derive(Debug, Clone)]
pub struct SessionMessage {
    pub index: usize,
    pub role: String,
    pub content: String,
    pub created_at: String,
    pub server_ref: Option<String>,
    pub adapter_ref: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone)]
pub struct SessionMessages {
    pub session_ref: String,
    pub short_ref: String,
    pub messages: Vec<SessionMessage>,
    pub tail: usize,
    pub total_messages: usize,
    pub truncated: bool,
    pub warnings: Vec<SessionWarning>,
}

#[derive(Debug, Clone)]
struct ResolvedSession {
    metadata: SessionMetadata,
    store_path: PathBuf,
    metadata_path: PathBuf,
    messages_path: PathBuf,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

#[derive(Debug, Deserialize)]
struct RawSessionMessage {
    schema: String,
    role: String,
    content: String,
    created_at: String,
    server_ref: Option<String>,
    adapter_ref: Option<String>,
    metadata: Option<Value>,
}

impl SessionManager {
    pub fn open_readonly(home_override: Option<&Path>) -> Result<Self, SessionError> {
        Ok(Self {
            paths: SessionStorePaths::resolve(home_override)?,
        })
    }

    pub fn list(&self) -> Result<Vec<SessionSummary>, SessionError> {
        let mut sessions = self.load_all_sessions()?;
        sessions.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| right.created_at.cmp(&left.created_at))
                .then_with(|| left.metadata.session_ref.cmp(&right.metadata.session_ref))
        });

        Ok(sessions
            .into_iter()
            .map(|resolved| SessionSummary {
                metadata: resolved.metadata,
                store_path: resolved.store_path,
                messages_path: resolved.messages_path,
            })
            .collect())
    }

    pub fn inspect(&self, reference: &str) -> Result<SessionInspection, SessionError> {
        let resolved = self.resolve_reference(reference)?;
        let mut warnings = Vec::new();
        if !resolved.messages_path.exists() {
            warnings.push(messages_missing_warning());
        }

        Ok(SessionInspection {
            metadata: resolved.metadata,
            store_path: resolved.store_path,
            metadata_path: resolved.metadata_path,
            messages_path: resolved.messages_path,
            warnings,
        })
    }

    pub fn messages(&self, reference: &str, tail: usize) -> Result<SessionMessages, SessionError> {
        let resolved = self.resolve_reference(reference)?;
        let mut warnings = Vec::new();
        if !resolved.messages_path.exists() {
            warnings.push(messages_missing_warning());
            return Ok(SessionMessages {
                session_ref: resolved.metadata.session_ref,
                short_ref: resolved.metadata.short_ref,
                messages: Vec::new(),
                tail,
                total_messages: 0,
                truncated: false,
                warnings,
            });
        }

        let (messages, total_messages) = read_tail_messages(&resolved.messages_path, tail)?;
        if resolved.metadata.message_count != total_messages {
            warnings.push(message_count_mismatch_warning(
                resolved.metadata.message_count,
                total_messages,
            ));
        }

        Ok(SessionMessages {
            session_ref: resolved.metadata.session_ref,
            short_ref: resolved.metadata.short_ref,
            messages,
            tail,
            total_messages,
            truncated: total_messages > tail,
            warnings,
        })
    }

    fn resolve_reference(&self, reference: &str) -> Result<ResolvedSession, SessionError> {
        if reference.is_empty() {
            return Err(SessionError::NotFound(reference.to_string()));
        }

        let mut matches = Vec::new();
        for resolved in self.load_all_sessions()? {
            if resolved.metadata.session_ref.starts_with(reference)
                || resolved.metadata.short_ref.starts_with(reference)
            {
                matches.push(resolved);
            }
        }

        match matches.len() {
            0 => Err(SessionError::NotFound(reference.to_string())),
            1 => Ok(matches.remove(0)),
            _ => Err(SessionError::AmbiguousRef(reference.to_string())),
        }
    }

    fn load_all_sessions(&self) -> Result<Vec<ResolvedSession>, SessionError> {
        if !self.paths.sessions_dir.exists() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();
        for entry in fs::read_dir(&self.paths.sessions_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            sessions.push(self.load_session_dir(&entry.path())?);
        }
        Ok(sessions)
    }

    fn load_session_dir(&self, store_path: &Path) -> Result<ResolvedSession, SessionError> {
        let session_ref = store_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| SessionError::InvalidMetadata {
                path: store_path.to_path_buf(),
                message: "session directory name must be valid UTF-8".to_string(),
            })?;
        let metadata_path = store_path.join("session.toml");
        let messages_path = store_path.join("messages.jsonl");
        let metadata = read_session_metadata(&metadata_path)?;
        validate_metadata(&metadata_path, session_ref, &metadata)?;
        let created_at = parse_metadata_time(&metadata_path, "created_at", &metadata.created_at)?;
        let updated_at = parse_metadata_time(&metadata_path, "updated_at", &metadata.updated_at)?;

        Ok(ResolvedSession {
            metadata,
            store_path: store_path.to_path_buf(),
            metadata_path,
            messages_path,
            created_at,
            updated_at,
        })
    }
}

fn read_tail_messages(
    path: &Path,
    tail: usize,
) -> Result<(Vec<SessionMessage>, usize), SessionError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut total_messages = 0_usize;
    let mut tail_messages = VecDeque::new();

    for (line_index, line) in reader.lines().enumerate() {
        let line_number = line_index + 1;
        let line = line?;
        let raw: RawSessionMessage =
            serde_json::from_str(&line).map_err(|error| SessionError::MessageParse {
                path: path.to_path_buf(),
                line: line_number,
                message: error.to_string(),
            })?;
        let message = parse_message(path, line_number, line_index, raw)?;
        total_messages += 1;
        tail_messages.push_back(message);
        if tail_messages.len() > tail {
            tail_messages.pop_front();
        }
    }

    Ok((tail_messages.into_iter().collect(), total_messages))
}

fn parse_message(
    path: &Path,
    line: usize,
    index: usize,
    raw: RawSessionMessage,
) -> Result<SessionMessage, SessionError> {
    if raw.schema != SESSION_MESSAGE_SCHEMA {
        return Err(SessionError::MessageParse {
            path: path.to_path_buf(),
            line,
            message: format!(
                "schema must be `{SESSION_MESSAGE_SCHEMA}`, got `{}`",
                raw.schema
            ),
        });
    }
    if !matches!(raw.role.as_str(), "system" | "user" | "assistant" | "tool") {
        return Err(SessionError::MessageParse {
            path: path.to_path_buf(),
            line,
            message: format!("unknown message role `{}`", raw.role),
        });
    }
    parse_message_time(path, line, &raw.created_at)?;

    let metadata = match raw.metadata {
        Some(value @ Value::Object(_)) => value,
        Some(_) => {
            return Err(SessionError::MessageParse {
                path: path.to_path_buf(),
                line,
                message: "`metadata` must be an object when present".to_string(),
            })
        }
        None => Value::Object(Default::default()),
    };

    Ok(SessionMessage {
        index,
        role: raw.role,
        content: raw.content,
        created_at: raw.created_at,
        server_ref: raw.server_ref,
        adapter_ref: raw.adapter_ref,
        metadata,
    })
}

fn validate_metadata(
    path: &Path,
    dir_session_ref: &str,
    metadata: &SessionMetadata,
) -> Result<(), SessionError> {
    if metadata.schema != SESSION_SCHEMA {
        return Err(SessionError::InvalidMetadata {
            path: path.to_path_buf(),
            message: format!(
                "schema must be `{SESSION_SCHEMA}`, got `{}`",
                metadata.schema
            ),
        });
    }
    if metadata.session_ref != dir_session_ref {
        return Err(SessionError::InvalidMetadata {
            path: path.to_path_buf(),
            message: "session_ref must match the session directory name".to_string(),
        });
    }
    if !valid_ref(&metadata.session_ref) {
        return Err(SessionError::InvalidMetadata {
            path: path.to_path_buf(),
            message: "session_ref must be lowercase hexadecimal and at least 12 characters"
                .to_string(),
        });
    }
    let expected_short_ref = metadata.session_ref.chars().take(12).collect::<String>();
    if metadata.short_ref != expected_short_ref {
        return Err(SessionError::InvalidMetadata {
            path: path.to_path_buf(),
            message: "short_ref must be the first 12 characters of session_ref".to_string(),
        });
    }
    Ok(())
}

fn valid_ref(value: &str) -> bool {
    value.len() >= 12
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        && value == value.to_ascii_lowercase()
}

fn parse_metadata_time(
    path: &Path,
    field: &str,
    value: &str,
) -> Result<OffsetDateTime, SessionError> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|error| SessionError::InvalidMetadata {
        path: path.to_path_buf(),
        message: format!("{field} must be RFC3339: {error}"),
    })
}

fn parse_message_time(path: &Path, line: usize, value: &str) -> Result<(), SessionError> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|error| SessionError::MessageParse {
        path: path.to_path_buf(),
        line,
        message: format!("created_at must be RFC3339: {error}"),
    })?;
    Ok(())
}

fn messages_missing_warning() -> SessionWarning {
    SessionWarning {
        code: MESSAGES_MISSING_WARNING.to_string(),
        message: "messages.jsonl was not found".to_string(),
    }
}

fn message_count_mismatch_warning(expected: usize, actual: usize) -> SessionWarning {
    SessionWarning {
        code: MESSAGE_COUNT_MISMATCH_WARNING.to_string(),
        message: format!("session metadata records message_count={expected}, but messages.jsonl contains {actual} message(s)"),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    #[test]
    fn empty_isolated_home_lists_zero_sessions() {
        let home = unique_home("empty");
        let manager = SessionManager::open_readonly(Some(&home)).expect("manager");

        assert!(manager.list().expect("list").is_empty());
    }

    #[test]
    fn fixture_sessions_list_by_updated_at_descending() {
        let home = unique_home("list-order");
        write_session(
            &home,
            "aaaaaaaaaaaa000000000000",
            "Older",
            "2026-05-01T00:00:00Z",
            "2026-05-01T00:10:00Z",
            0,
            None,
        );
        write_session(
            &home,
            "bbbbbbbbbbbb000000000000",
            "Newer",
            "2026-05-01T00:00:00Z",
            "2026-05-01T00:20:00Z",
            0,
            None,
        );

        let manager = SessionManager::open_readonly(Some(&home)).expect("manager");
        let sessions = manager.list().expect("list");

        assert_eq!(sessions[0].metadata.short_ref, "bbbbbbbbbbbb");
        assert_eq!(sessions[1].metadata.short_ref, "aaaaaaaaaaaa");
    }

    #[test]
    fn inspect_accepts_full_ref_and_unique_prefix() {
        let home = unique_home("inspect");
        write_session(
            &home,
            "abcdefabcdef000000000000",
            "Inspect",
            "2026-05-01T00:00:00Z",
            "2026-05-01T00:10:00Z",
            0,
            None,
        );
        let manager = SessionManager::open_readonly(Some(&home)).expect("manager");

        let by_full = manager
            .inspect("abcdefabcdef000000000000")
            .expect("full ref");
        let by_prefix = manager.inspect("abcdef").expect("prefix");

        assert_eq!(by_full.metadata.session_ref, by_prefix.metadata.session_ref);
        assert_eq!(by_prefix.warnings[0].code, "messages_missing");
    }

    #[test]
    fn missing_and_ambiguous_refs_return_core_errors() {
        let home = unique_home("refs");
        write_session(
            &home,
            "aaaaaaaaaaaa000000000000",
            "One",
            "2026-05-01T00:00:00Z",
            "2026-05-01T00:10:00Z",
            0,
            None,
        );
        write_session(
            &home,
            "aaaaaaaaaaab000000000000",
            "Two",
            "2026-05-01T00:00:00Z",
            "2026-05-01T00:20:00Z",
            0,
            None,
        );
        let manager = SessionManager::open_readonly(Some(&home)).expect("manager");

        assert!(matches!(
            manager.inspect("missing").expect_err("missing"),
            SessionError::NotFound(_)
        ));
        assert!(matches!(
            manager.inspect("").expect_err("empty"),
            SessionError::NotFound(_)
        ));
        assert!(matches!(
            manager.inspect("aaaa").expect_err("ambiguous"),
            SessionError::AmbiguousRef(_)
        ));
    }

    #[test]
    fn message_tail_reports_total_truncation_and_global_indexes() {
        let home = unique_home("messages-tail");
        write_session(
            &home,
            "cccccccccccc000000000000",
            "Tail",
            "2026-05-01T00:00:00Z",
            "2026-05-01T00:10:00Z",
            4,
            Some(&[
                message("user", "one"),
                message("assistant", "two"),
                message("user", "three"),
                message("assistant", "four"),
            ]),
        );
        let manager = SessionManager::open_readonly(Some(&home)).expect("manager");

        let messages = manager.messages("cccccccccccc", 2).expect("messages");

        assert_eq!(messages.total_messages, 4);
        assert!(messages.truncated);
        assert_eq!(messages.messages.len(), 2);
        assert_eq!(messages.messages[0].index, 2);
        assert_eq!(messages.messages[0].content, "three");
        assert_eq!(messages.messages[1].index, 3);
        assert!(messages.warnings.is_empty());
    }

    #[test]
    fn missing_messages_jsonl_returns_empty_messages_with_warning() {
        let home = unique_home("missing-messages");
        write_session(
            &home,
            "dddddddddddd000000000000",
            "Missing",
            "2026-05-01T00:00:00Z",
            "2026-05-01T00:10:00Z",
            3,
            None,
        );
        let manager = SessionManager::open_readonly(Some(&home)).expect("manager");

        let messages = manager.messages("dddddddddddd", 200).expect("messages");

        assert_eq!(messages.total_messages, 0);
        assert!(!messages.truncated);
        assert_eq!(messages.warnings[0].code, "messages_missing");
    }

    #[test]
    fn message_count_mismatch_returns_structured_warning() {
        let home = unique_home("count-mismatch");
        write_session(
            &home,
            "eeeeeeeeeeee000000000000",
            "Mismatch",
            "2026-05-01T00:00:00Z",
            "2026-05-01T00:10:00Z",
            99,
            Some(&[message("user", "one")]),
        );
        let manager = SessionManager::open_readonly(Some(&home)).expect("manager");

        let messages = manager.messages("eeeeeeeeeeee", 200).expect("messages");

        assert_eq!(messages.total_messages, 1);
        assert_eq!(messages.warnings[0].code, "message_count_mismatch");
    }

    #[test]
    fn malformed_metadata_messages_timestamps_and_roles_fail_safely() {
        let home = unique_home("malformed");
        let session_dir = home.join("sessions/ffffffffffff000000000000");
        fs::create_dir_all(&session_dir).expect("session dir");
        fs::write(
            session_dir.join("session.toml"),
            r#"schema = "tentgent.session.v1"
session_ref = "ffffffffffff000000000000"
short_ref = "ffffffffffff"
title = "Bad"
created_at = "not-a-date"
updated_at = "2026-05-01T00:00:00Z"
message_count = 0
tags = []
"#,
        )
        .expect("metadata");
        let manager = SessionManager::open_readonly(Some(&home)).expect("manager");
        assert!(matches!(
            manager.list().expect_err("invalid timestamp"),
            SessionError::InvalidMetadata { .. }
        ));

        let home = unique_home("bad-role");
        write_session(
            &home,
            "999999999999000000000000",
            "Bad role",
            "2026-05-01T00:00:00Z",
            "2026-05-01T00:10:00Z",
            1,
            Some(&[message("alien", "hello")]),
        );
        let manager = SessionManager::open_readonly(Some(&home)).expect("manager");
        assert!(matches!(
            manager.messages("999999999999", 10).expect_err("bad role"),
            SessionError::MessageParse { line: 1, .. }
        ));
    }

    fn write_session(
        home: &Path,
        session_ref: &str,
        title: &str,
        created_at: &str,
        updated_at: &str,
        message_count: usize,
        messages: Option<&[String]>,
    ) {
        let session_dir = home.join("sessions").join(session_ref);
        fs::create_dir_all(&session_dir).expect("session dir");
        fs::write(
            session_dir.join("session.toml"),
            format!(
                r#"schema = "tentgent.session.v1"
session_ref = "{session_ref}"
short_ref = "{}"
title = "{title}"
created_at = "{created_at}"
updated_at = "{updated_at}"
message_count = {message_count}
tags = []
"#,
                &session_ref[..12]
            ),
        )
        .expect("metadata");
        if let Some(messages) = messages {
            fs::write(
                session_dir.join("messages.jsonl"),
                messages.join("\n") + "\n",
            )
            .expect("messages");
        }
    }

    fn message(role: &str, content: &str) -> String {
        format!(
            r#"{{"schema":"tentgent.session.message.v1","role":"{role}","content":"{content}","created_at":"2026-05-01T00:00:00Z","metadata":{{}}}}"#
        )
    }

    fn unique_home(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("tentgent-session-{label}-{nanos}"))
    }
}
