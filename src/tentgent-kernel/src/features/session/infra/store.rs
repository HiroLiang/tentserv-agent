use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::Path,
};

use crate::features::session::domain::{
    SessionCompactionOutcome, SessionFilePaths, SessionInspection, SessionMessage, SessionMessages,
    SessionMetadata, SessionRef, SessionRefSelector, SessionRemovalOutcome, SessionStorageLocation,
    SessionStoreConfig, SessionStoreLayout, SessionSummary, SessionWarning, StoredSessionMessage,
    SESSION_MESSAGE_SCHEMA, SESSION_SCHEMA,
};
use crate::features::session::ports::{
    SessionAppendMutation, SessionCreateRecord, SessionStore, SessionTranscriptRewrite,
};
use crate::foundation::error::KernelResult;

use super::error::{path_error, session_store_error};

const MESSAGES_MISSING_WARNING: &str = "messages_missing";
const MESSAGE_COUNT_MISMATCH_WARNING: &str = "message_count_mismatch";

/// Filesystem-backed session store using `session.toml` and `messages.jsonl`.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileSessionStore;

impl SessionStore for FileSessionStore {
    fn ensure_session_store(&self, store: &SessionStoreConfig) -> KernelResult<()> {
        let layout = file_layout(store)?;
        fs::create_dir_all(&layout.sessions_dir)
            .map_err(|err| path_error("create session store directory", &layout.sessions_dir, err))
    }

    fn list_sessions(&self, store: &SessionStoreConfig) -> KernelResult<Vec<SessionSummary>> {
        let layout = file_layout(store)?;
        let mut sessions = Vec::new();
        if !layout.sessions_dir.exists() {
            return Ok(sessions);
        }

        for entry in fs::read_dir(&layout.sessions_dir)
            .map_err(|err| path_error("read session store directory", &layout.sessions_dir, err))?
        {
            let entry = entry.map_err(|err| {
                session_store_error(format!(
                    "read entry in session store `{}` failed: {err}",
                    layout.sessions_dir.display()
                ))
            })?;
            let file_type = entry.file_type().map_err(|err| {
                path_error("read session store entry type", entry.path().as_path(), err)
            })?;
            if !file_type.is_dir() {
                continue;
            }

            let session_ref = parse_session_dir_name(entry.path().as_path())?;
            let metadata = self.load_session_metadata(store, &session_ref)?;
            sessions.push(SessionSummary {
                location: layout.file_location(&metadata.session_ref),
                metadata,
            });
        }

        sessions.sort_by(|left, right| {
            right
                .metadata
                .updated_at
                .cmp(&left.metadata.updated_at)
                .then_with(|| right.metadata.created_at.cmp(&left.metadata.created_at))
                .then_with(|| left.metadata.session_ref.cmp(&right.metadata.session_ref))
        });
        Ok(sessions)
    }

    fn inspect_session(
        &self,
        store: &SessionStoreConfig,
        selector: &SessionRefSelector,
    ) -> KernelResult<SessionInspection> {
        let layout = file_layout(store)?;
        let metadata = resolve_metadata(self, store, selector)?;
        let mut warnings = Vec::new();
        if !layout.messages_path(&metadata.session_ref).exists() {
            warnings.push(messages_missing_warning());
        }

        Ok(SessionInspection {
            location: layout.file_location(&metadata.session_ref),
            metadata,
            warnings,
        })
    }

    fn load_session_metadata(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
    ) -> KernelResult<SessionMetadata> {
        let layout = file_layout(store)?;
        let path = layout.metadata_path(session_ref);
        let metadata = read_session_metadata(&path)?;
        validate_metadata(&path, session_ref, &metadata)?;
        Ok(metadata)
    }

    fn read_all_messages(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
    ) -> KernelResult<Vec<SessionMessage>> {
        let layout = file_layout(store)?;
        let path = layout.messages_path(session_ref);
        if !path.exists() {
            return Ok(Vec::new());
        }
        read_messages(&path)
    }

    fn read_tail_messages(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
        tail: usize,
    ) -> KernelResult<SessionMessages> {
        let layout = file_layout(store)?;
        let metadata = self.load_session_metadata(store, session_ref)?;
        let path = layout.messages_path(session_ref);
        let mut warnings = Vec::new();
        if !path.exists() {
            warnings.push(messages_missing_warning());
            return Ok(SessionMessages {
                session_ref: metadata.session_ref,
                short_ref: metadata.short_ref,
                messages: Vec::new(),
                tail,
                total_messages: 0,
                truncated: false,
                warnings,
            });
        }

        let messages = read_messages(&path)?;
        let total_messages = messages.len();
        if metadata.message_count != total_messages {
            warnings.push(message_count_mismatch_warning(
                metadata.message_count,
                total_messages,
            ));
        }
        let truncated = total_messages > tail;
        let messages = if truncated {
            messages[total_messages - tail..].to_vec()
        } else {
            messages
        };

        Ok(SessionMessages {
            session_ref: metadata.session_ref,
            short_ref: metadata.short_ref,
            messages,
            tail,
            total_messages,
            truncated,
            warnings,
        })
    }

    fn create_session(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
        record: SessionCreateRecord,
    ) -> KernelResult<SessionInspection> {
        let layout = file_layout(store)?;
        validate_record_ref("create session", session_ref, &record.metadata)?;
        fs::create_dir_all(&layout.sessions_dir).map_err(|err| {
            path_error("create session store directory", &layout.sessions_dir, err)
        })?;
        let session_dir = layout.session_dir(session_ref);
        fs::create_dir(&session_dir)
            .map_err(|err| path_error("create session directory", &session_dir, err))?;
        write_session_metadata_atomic(&layout.metadata_path(session_ref), &record.metadata)?;
        if !record.initial_messages.is_empty() {
            write_messages_atomic(&layout.messages_path(session_ref), &record.initial_messages)?;
        }
        self.inspect_session(
            store,
            &SessionRefSelector::parse(session_ref.as_str())
                .map_err(|err| session_store_error(err.to_string()))?,
        )
    }

    fn update_session_metadata(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
        metadata: SessionMetadata,
    ) -> KernelResult<SessionInspection> {
        let layout = file_layout(store)?;
        validate_record_ref("update session metadata", session_ref, &metadata)?;
        write_session_metadata_atomic(&layout.metadata_path(session_ref), &metadata)?;
        self.inspect_session(
            store,
            &SessionRefSelector::parse(session_ref.as_str())
                .map_err(|err| session_store_error(err.to_string()))?,
        )
    }

    fn append_session_messages(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
        mutation: SessionAppendMutation,
    ) -> KernelResult<crate::features::session::domain::SessionAppendOutcome> {
        let layout = file_layout(store)?;
        validate_record_ref("append session messages", session_ref, &mutation.metadata)?;
        append_stored_messages(&layout.messages_path(session_ref), &mutation.messages)?;
        write_session_metadata_atomic(&layout.metadata_path(session_ref), &mutation.metadata)?;

        Ok(crate::features::session::domain::SessionAppendOutcome {
            metadata: mutation.metadata,
            location: layout.file_location(session_ref),
            appended: mutation.appended,
        })
    }

    fn rewrite_session_transcript(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
        rewrite: SessionTranscriptRewrite,
    ) -> KernelResult<SessionCompactionOutcome> {
        let layout = file_layout(store)?;
        validate_record_ref("rewrite session transcript", session_ref, &rewrite.metadata)?;
        write_messages_atomic(&layout.messages_path(session_ref), &rewrite.replacement)?;
        write_session_metadata_atomic(&layout.metadata_path(session_ref), &rewrite.metadata)?;

        Ok(SessionCompactionOutcome {
            metadata: rewrite.metadata,
            location: layout.file_location(session_ref),
            compacted: rewrite.compacted,
            source_message_count: rewrite.source_message_count,
            replaced_message_count: rewrite.replaced_message_count,
            kept_recent_messages: rewrite.kept_recent_messages,
            summary_index: rewrite.summary_index,
        })
    }

    fn remove_session(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
    ) -> KernelResult<SessionRemovalOutcome> {
        let inspection = self.inspect_session(
            store,
            &SessionRefSelector::parse(session_ref.as_str())
                .map_err(|err| session_store_error(err.to_string()))?,
        )?;
        let SessionStorageLocation::File(SessionFilePaths { store_path, .. }) =
            &inspection.location
        else {
            return Err(session_store_error(
                "file session store cannot remove a non-file session location",
            ));
        };
        fs::remove_dir_all(store_path)
            .map_err(|err| path_error("remove session directory", store_path, err))?;
        Ok(SessionRemovalOutcome { inspection })
    }
}

fn file_layout(store: &SessionStoreConfig) -> KernelResult<&SessionStoreLayout> {
    store
        .file_layout()
        .ok_or_else(|| session_store_error("file session store requires a file session config"))
}

fn resolve_metadata(
    store: &FileSessionStore,
    config: &SessionStoreConfig,
    selector: &SessionRefSelector,
) -> KernelResult<SessionMetadata> {
    let mut matches = Vec::new();
    for summary in store.list_sessions(config)? {
        if summary
            .metadata
            .session_ref
            .as_str()
            .starts_with(selector.as_str())
            || summary.metadata.short_ref.starts_with(selector.as_str())
        {
            matches.push(summary.metadata);
        }
    }

    match matches.len() {
        0 => Err(session_store_error(format!(
            "session `{}` was not found",
            selector.as_str()
        ))),
        1 => Ok(matches.remove(0)),
        _ => Err(session_store_error(format!(
            "session reference `{}` is ambiguous",
            selector.as_str()
        ))),
    }
}

fn parse_session_dir_name(path: &Path) -> KernelResult<SessionRef> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            session_store_error(format!(
                "session directory `{}` must have a valid UTF-8 name",
                path.display()
            ))
        })?;
    SessionRef::parse(name).map_err(|err| {
        session_store_error(format!("invalid session directory name `{name}`: {err}"))
    })
}

fn read_session_metadata(path: &Path) -> KernelResult<SessionMetadata> {
    let body =
        fs::read_to_string(path).map_err(|err| path_error("read session metadata", path, err))?;
    toml::from_str(&body).map_err(|err| {
        session_store_error(format!(
            "parse session metadata `{}` failed: {err}",
            path.display()
        ))
    })
}

fn validate_metadata(
    path: &Path,
    session_ref: &SessionRef,
    metadata: &SessionMetadata,
) -> KernelResult<()> {
    if metadata.schema != SESSION_SCHEMA {
        return Err(session_store_error(format!(
            "session metadata `{}` has unsupported schema `{}`",
            path.display(),
            metadata.schema
        )));
    }
    if &metadata.session_ref != session_ref {
        return Err(session_store_error(format!(
            "session metadata `{}` session_ref `{}` does not match directory `{}`",
            path.display(),
            metadata.session_ref,
            session_ref
        )));
    }
    if metadata.short_ref != session_ref.short_ref() {
        return Err(session_store_error(format!(
            "session metadata `{}` short_ref `{}` does not match session_ref `{}`",
            path.display(),
            metadata.short_ref,
            session_ref
        )));
    }
    Ok(())
}

fn validate_record_ref(
    action: &str,
    session_ref: &SessionRef,
    metadata: &SessionMetadata,
) -> KernelResult<()> {
    if &metadata.session_ref != session_ref {
        return Err(session_store_error(format!(
            "{action} metadata session_ref `{}` does not match `{}`",
            metadata.session_ref, session_ref
        )));
    }
    Ok(())
}

fn read_messages(path: &Path) -> KernelResult<Vec<SessionMessage>> {
    let file = File::open(path).map_err(|err| path_error("open session messages", path, err))?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();

    for (line_index, line) in reader.lines().enumerate() {
        let line_number = line_index + 1;
        let line = line.map_err(|err| path_error("read session message line", path, err))?;
        let raw: StoredSessionMessage = serde_json::from_str(&line).map_err(|err| {
            session_store_error(format!(
                "parse session message `{}` line {} failed: {err}",
                path.display(),
                line_number
            ))
        })?;
        messages.push(parse_message(path, line_number, line_index, raw)?);
    }

    Ok(messages)
}

fn parse_message(
    path: &Path,
    line_number: usize,
    index: usize,
    raw: StoredSessionMessage,
) -> KernelResult<SessionMessage> {
    if raw.schema != SESSION_MESSAGE_SCHEMA {
        return Err(session_store_error(format!(
            "session message `{}` line {} has unsupported schema `{}`",
            path.display(),
            line_number,
            raw.schema
        )));
    }

    Ok(SessionMessage {
        index,
        role: raw.role,
        content: raw.content,
        created_at: raw.created_at,
        server_ref: raw.server_ref,
        adapter_ref: raw.adapter_ref,
        metadata: raw.metadata,
    })
}

fn write_session_metadata_atomic(path: &Path, metadata: &SessionMetadata) -> KernelResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| path_error("create session metadata parent directory", parent, err))?;
    }
    let tmp_path = path.with_file_name("session.toml.tmp");
    let body = toml::to_string_pretty(metadata)
        .map_err(|err| session_store_error(format!("serialize session metadata failed: {err}")))?;
    fs::write(&tmp_path, body)
        .map_err(|err| path_error("write temporary session metadata", &tmp_path, err))?;
    fs::rename(&tmp_path, path).map_err(|err| path_error("replace session metadata", path, err))
}

fn write_messages_atomic(path: &Path, messages: &[StoredSessionMessage]) -> KernelResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| path_error("create session messages parent directory", parent, err))?;
    }
    let tmp_path = path.with_file_name("messages.jsonl.tmp");
    {
        let mut file = File::create(&tmp_path)
            .map_err(|err| path_error("create temporary session messages", &tmp_path, err))?;
        for message in messages {
            write_message_line(&mut file, path, message)?;
        }
        file.flush()
            .map_err(|err| path_error("flush temporary session messages", &tmp_path, err))?;
    }
    fs::rename(&tmp_path, path).map_err(|err| path_error("replace session messages", path, err))
}

fn append_stored_messages(path: &Path, messages: &[StoredSessionMessage]) -> KernelResult<()> {
    if messages.is_empty() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| path_error("create session messages parent directory", parent, err))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| path_error("open session messages for append", path, err))?;
    for message in messages {
        write_message_line(&mut file, path, message)?;
    }
    file.flush()
        .map_err(|err| path_error("flush session messages", path, err))
}

fn write_message_line(
    file: &mut File,
    path: &Path,
    message: &StoredSessionMessage,
) -> KernelResult<()> {
    let line = serde_json::to_string(message)
        .map_err(|err| session_store_error(format!("serialize session message failed: {err}")))?;
    file.write_all(line.as_bytes())
        .map_err(|err| path_error("write session message", path, err))?;
    file.write_all(b"\n")
        .map_err(|err| path_error("write session message newline", path, err))
}

fn messages_missing_warning() -> SessionWarning {
    SessionWarning {
        code: MESSAGES_MISSING_WARNING.to_string(),
        message: "messages.jsonl is missing; transcript is empty".to_string(),
    }
}

fn message_count_mismatch_warning(expected: usize, actual: usize) -> SessionWarning {
    SessionWarning {
        code: MESSAGE_COUNT_MISMATCH_WARNING.to_string(),
        message: format!("metadata message_count is {expected}, but transcript has {actual}"),
    }
}
