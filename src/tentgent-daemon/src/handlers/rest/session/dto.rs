use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use tentgent_kernel::features::session::domain::{
    SessionFilePaths, SessionInspection, SessionMessage, SessionMessages, SessionStorageLocation,
    SessionSummary, SessionWarning,
};

#[derive(Debug, Serialize)]
pub struct SessionsResponse {
    pub sessions: Vec<SessionSummaryItem>,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub session: SessionInspectionItem,
}

#[derive(Debug, Serialize)]
pub struct SessionMessagesResponse {
    pub session: SessionRefItem,
    pub messages: Vec<SessionMessageItem>,
    pub tail: usize,
    pub total_messages: usize,
    pub truncated: bool,
    pub warnings: Vec<SessionWarningItem>,
}

#[derive(Debug, Serialize)]
pub struct SessionSummaryItem {
    pub session_ref: String,
    pub short_ref: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    pub default_server_ref: Option<String>,
    pub adapter_ref: Option<String>,
    pub tags: Vec<String>,
    pub store_path: String,
}

#[derive(Debug, Serialize)]
pub struct SessionInspectionItem {
    pub session_ref: String,
    pub short_ref: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    pub default_server_ref: Option<String>,
    pub adapter_ref: Option<String>,
    pub tags: Vec<String>,
    pub store_path: String,
    pub messages_path: String,
    pub warnings: Vec<SessionWarningItem>,
}

#[derive(Debug, Serialize)]
pub struct SessionRefItem {
    pub session_ref: String,
    pub short_ref: String,
}

#[derive(Debug, Serialize)]
pub struct SessionMessageItem {
    pub index: usize,
    pub role: String,
    pub content: String,
    pub created_at: String,
    pub server_ref: Option<String>,
    pub adapter_ref: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Serialize)]
pub struct SessionWarningItem {
    pub code: String,
    pub message: String,
}

pub fn session_summary_item(summary: SessionSummary) -> SessionSummaryItem {
    let metadata = summary.metadata;
    SessionSummaryItem {
        session_ref: metadata.session_ref.into_string(),
        short_ref: metadata.short_ref,
        title: metadata.title,
        created_at: metadata.created_at,
        updated_at: metadata.updated_at,
        message_count: metadata.message_count,
        default_server_ref: metadata.default_server_ref,
        adapter_ref: metadata.adapter_ref,
        tags: metadata.tags,
        store_path: store_path_string(summary.location),
    }
}

pub fn session_inspection_item(inspection: SessionInspection) -> SessionInspectionItem {
    let metadata = inspection.metadata;
    let messages_path = messages_path_string(&inspection.location);
    SessionInspectionItem {
        session_ref: metadata.session_ref.into_string(),
        short_ref: metadata.short_ref,
        title: metadata.title,
        created_at: metadata.created_at,
        updated_at: metadata.updated_at,
        message_count: metadata.message_count,
        default_server_ref: metadata.default_server_ref,
        adapter_ref: metadata.adapter_ref,
        tags: metadata.tags,
        store_path: store_path_string(inspection.location),
        messages_path,
        warnings: inspection
            .warnings
            .into_iter()
            .map(session_warning_item)
            .collect(),
    }
}

pub fn session_messages_item(messages: SessionMessages) -> SessionMessagesResponse {
    SessionMessagesResponse {
        session: SessionRefItem {
            session_ref: messages.session_ref.into_string(),
            short_ref: messages.short_ref,
        },
        messages: messages
            .messages
            .into_iter()
            .map(session_message_item)
            .collect(),
        tail: messages.tail,
        total_messages: messages.total_messages,
        truncated: messages.truncated,
        warnings: messages
            .warnings
            .into_iter()
            .map(session_warning_item)
            .collect(),
    }
}

fn session_message_item(message: SessionMessage) -> SessionMessageItem {
    SessionMessageItem {
        index: message.index,
        role: message.role.to_string(),
        content: message.content,
        created_at: message.created_at,
        server_ref: message.server_ref,
        adapter_ref: message.adapter_ref,
        metadata: message.metadata,
    }
}

fn session_warning_item(warning: SessionWarning) -> SessionWarningItem {
    SessionWarningItem {
        code: warning.code,
        message: warning.message,
    }
}

fn store_path_string(location: SessionStorageLocation) -> String {
    match location {
        SessionStorageLocation::File(SessionFilePaths { store_path, .. }) => {
            path_string(store_path)
        }
        SessionStorageLocation::External { backend, locator } => {
            locator.unwrap_or_else(|| format!("external:{backend}"))
        }
    }
}

fn messages_path_string(location: &SessionStorageLocation) -> String {
    match location {
        SessionStorageLocation::File(SessionFilePaths { messages_path, .. }) => {
            path_string(messages_path)
        }
        SessionStorageLocation::External { backend, locator } => locator
            .clone()
            .unwrap_or_else(|| format!("external:{backend}:messages")),
    }
}

fn path_string(path: impl AsRef<Path>) -> String {
    path.as_ref().display().to_string()
}
