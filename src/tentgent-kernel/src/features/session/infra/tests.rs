use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::features::session::domain::{
    SessionAppendedMessage, SessionMessageRole, SessionMetadata, SessionRef, SessionStoreConfig,
    StoredSessionMessage, SESSION_REF_HEX_LENGTH,
};
use crate::features::session::infra::{
    FileSessionLockManager, FileSessionStore, StdSessionIdentityGenerator, SystemSessionClock,
};
use crate::features::session::ports::{
    SessionAppendMutation, SessionClock, SessionCreateRecord, SessionIdentityGenerator,
    SessionLockManager, SessionStore, SessionTranscriptRewrite,
};

#[test]
fn file_session_store_preserves_legacy_file_layout_behind_store_port() {
    let root = temp_home("file-store");
    let config = SessionStoreConfig::file_from_home_dir(&root);
    let layout = config.file_layout().expect("file layout");
    let store = FileSessionStore;
    let session_ref = SessionRef::parse("a".repeat(SESSION_REF_HEX_LENGTH)).expect("session ref");
    let created_at = "2026-05-17T00:00:00Z";
    let updated_at = "2026-05-17T00:00:01Z";
    let mut metadata = SessionMetadata::new(session_ref.clone(), created_at, updated_at);
    metadata.title = Some("Support review".to_string());
    metadata.message_count = 1;
    let initial = StoredSessionMessage::new(SessionMessageRole::User, "你好", created_at);

    store.ensure_session_store(&config).expect("ensure store");
    let inspection = store
        .create_session(
            &config,
            &session_ref,
            SessionCreateRecord {
                metadata: metadata.clone(),
                initial_messages: vec![initial],
            },
        )
        .expect("create session");

    assert_eq!(inspection.metadata.title.as_deref(), Some("Support review"));
    assert_eq!(inspection.location, layout.file_location(&session_ref));
    assert!(layout.metadata_path(&session_ref).is_file());
    assert!(layout.messages_path(&session_ref).is_file());

    let listed = store.list_sessions(&config).expect("list sessions");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].location, layout.file_location(&session_ref));

    let messages = store
        .read_tail_messages(&config, &session_ref, 10)
        .expect("read tail");
    assert_eq!(messages.total_messages, 1);
    assert_eq!(messages.messages[0].role, SessionMessageRole::User);

    metadata.message_count = 2;
    metadata.updated_at = "2026-05-17T00:00:02Z".to_string();
    let assistant = StoredSessionMessage::new(
        SessionMessageRole::Assistant,
        "您好，已收到。",
        "2026-05-17T00:00:02Z",
    );
    let append = store
        .append_session_messages(
            &config,
            &session_ref,
            SessionAppendMutation {
                metadata: metadata.clone(),
                messages: vec![assistant],
                appended: vec![SessionAppendedMessage {
                    index: 1,
                    role: SessionMessageRole::Assistant,
                    created_at: "2026-05-17T00:00:02Z".to_string(),
                }],
            },
        )
        .expect("append");
    assert_eq!(append.location, layout.file_location(&session_ref));
    assert_eq!(append.appended[0].index, 1);

    metadata.message_count = 1;
    metadata.updated_at = "2026-05-17T00:00:03Z".to_string();
    let summary = StoredSessionMessage::new(
        SessionMessageRole::System,
        "摘要：使用者需要繁中客服資料。",
        "2026-05-17T00:00:03Z",
    );
    let compaction = store
        .rewrite_session_transcript(
            &config,
            &session_ref,
            SessionTranscriptRewrite {
                metadata: metadata.clone(),
                replacement: vec![summary],
                compacted: true,
                source_message_count: 2,
                replaced_message_count: 2,
                kept_recent_messages: 0,
                summary_index: Some(0),
            },
        )
        .expect("rewrite");
    assert!(compaction.compacted);
    assert_eq!(compaction.location, layout.file_location(&session_ref));
    assert_eq!(
        store
            .read_all_messages(&config, &session_ref)
            .expect("read all")
            .len(),
        1
    );

    let removal = store
        .remove_session(&config, &session_ref)
        .expect("remove session");
    assert_eq!(
        removal.inspection.location,
        layout.file_location(&session_ref)
    );
    assert!(!layout.session_dir(&session_ref).exists());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn file_session_infra_generates_refs_timestamps_and_locks() {
    let root = temp_home("identity-lock");
    let config = SessionStoreConfig::file_from_home_dir(&root);
    let layout = config.file_layout().expect("file layout");
    let identity = StdSessionIdentityGenerator;
    let clock = SystemSessionClock;
    let locks = FileSessionLockManager::default();

    fs::create_dir_all(&layout.sessions_dir).expect("create sessions dir");
    let session_ref = identity
        .generate_session_ref(&config)
        .expect("generate session ref");
    assert!(session_ref.is_generated_length());
    assert!(!clock.now_rfc3339().expect("timestamp").is_empty());

    let create_lock = locks.acquire_create_lock(&config).expect("create lock");
    assert!(layout.create_lock_path().exists());
    drop(create_lock);
    assert!(!layout.create_lock_path().exists());

    fs::create_dir_all(layout.session_dir(&session_ref)).expect("create session dir");
    let session_lock = locks
        .acquire_session_lock(&config, &session_ref)
        .expect("session lock");
    assert!(layout.session_lock_path(&session_ref).exists());
    drop(session_lock);
    assert!(!layout.session_lock_path(&session_ref).exists());

    let _ = fs::remove_dir_all(root);
}

fn temp_home(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "tentgent-session-{name}-{}-{nanos}",
        std::process::id()
    ))
}
