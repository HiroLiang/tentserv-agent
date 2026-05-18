use std::path::PathBuf;

use crate::features::model::domain::{ModelRef, ModelRefSelector};

use super::domain::{
    parse_server_runtime_selection, CloudProvider, LaunchMode, ServerProcessMetadata, ServerRef,
    ServerRefParseError, ServerRefSelector, ServerRuntimeKind, ServerRuntimeSelection, ServerSpec,
    ServerStoreLayout, DEFAULT_SERVER_HOST, DEFAULT_SERVER_PORT, SERVER_PROCESS_FILENAME,
    SERVER_REF_HEX_LENGTH, SERVER_SPEC_FILENAME, SERVER_STDERR_LOG_FILENAME,
    SERVER_STDOUT_LOG_FILENAME, SHORT_SERVER_REF_LENGTH,
};

#[test]
fn server_ref_is_canonical_sha256_hex_and_derives_short_ref() {
    let uppercase = "A".repeat(SERVER_REF_HEX_LENGTH);
    let server_ref = ServerRef::parse(&uppercase).expect("server ref");

    assert_eq!(server_ref.as_str(), "a".repeat(SERVER_REF_HEX_LENGTH));
    assert_eq!(server_ref.short_ref(), "a".repeat(SHORT_SERVER_REF_LENGTH));
}

#[test]
fn server_ref_selector_accepts_short_or_full_hex_prefixes() {
    let short = ServerRefSelector::parse("ABC123").expect("short selector");
    assert_eq!(short.as_str(), "abc123");
    assert!(!short.is_full_ref());

    let full = ServerRefSelector::parse("b".repeat(SERVER_REF_HEX_LENGTH)).expect("full selector");
    assert!(full.is_full_ref());
}

#[test]
fn server_ref_validation_rejects_empty_wrong_length_and_non_hex_values() {
    assert_eq!(ServerRef::parse(""), Err(ServerRefParseError::Empty));
    assert_eq!(
        ServerRef::parse("abc"),
        Err(ServerRefParseError::InvalidFullLength { actual: 3 })
    );
    assert_eq!(
        ServerRefSelector::parse("../abc"),
        Err(ServerRefParseError::NonHex)
    );
}

#[test]
fn server_store_layout_matches_contract_paths() {
    let layout = ServerStoreLayout::from_home_and_servers_dir(
        "/tmp/tentgent-home",
        "/tmp/tentgent-home/servers",
    );
    let server_ref = "1".repeat(SERVER_REF_HEX_LENGTH);

    assert_eq!(
        layout.server_dir(&server_ref),
        PathBuf::from("/tmp/tentgent-home/servers").join(&server_ref)
    );
    assert_eq!(
        layout.server_spec_path(&server_ref),
        PathBuf::from("/tmp/tentgent-home/servers")
            .join(&server_ref)
            .join(SERVER_SPEC_FILENAME)
    );
    assert_eq!(
        layout.process_metadata_path(&server_ref),
        PathBuf::from("/tmp/tentgent-home/servers")
            .join(&server_ref)
            .join(SERVER_PROCESS_FILENAME)
    );
    assert_eq!(
        layout.stdout_log_path(&server_ref),
        PathBuf::from("/tmp/tentgent-home/servers")
            .join(&server_ref)
            .join(SERVER_STDOUT_LOG_FILENAME)
    );
    assert_eq!(
        layout.stderr_log_path(&server_ref),
        PathBuf::from("/tmp/tentgent-home/servers")
            .join(&server_ref)
            .join(SERVER_STDERR_LOG_FILENAME)
    );
}

#[test]
fn runtime_ref_parser_keeps_cloud_alias_and_local_model_selectors() {
    assert_eq!(
        parse_server_runtime_selection("openai:gpt-4.1-mini"),
        Ok(ServerRuntimeSelection::CloudProvider {
            provider: CloudProvider::OpenAI,
            provider_model: "gpt-4.1-mini".to_string(),
        })
    );
    assert_eq!(
        parse_server_runtime_selection("claude:claude-3-5-sonnet-latest"),
        Ok(ServerRuntimeSelection::CloudProvider {
            provider: CloudProvider::Anthropic,
            provider_model: "claude-3-5-sonnet-latest".to_string(),
        })
    );
    assert_eq!(
        parse_server_runtime_selection("abc123"),
        Ok(ServerRuntimeSelection::LocalModel {
            selector: ModelRefSelector::parse("abc123").expect("model selector"),
        })
    );
}

#[test]
fn server_spec_and_process_metadata_round_trip_existing_toml_shape() {
    let server_ref = ServerRef::parse("c".repeat(SERVER_REF_HEX_LENGTH)).expect("server ref");
    let model_ref = ModelRef::parse("d".repeat(64)).expect("model ref");
    let spec = ServerSpec {
        short_ref: server_ref.short_ref().to_string(),
        server_ref,
        runtime_kind: ServerRuntimeKind::Local,
        model_ref: Some(model_ref),
        provider: None,
        provider_model: None,
        host: DEFAULT_SERVER_HOST.to_string(),
        port: DEFAULT_SERVER_PORT,
        lazy_load: true,
        idle_seconds: Some(30),
        created_at: "2026-05-17T00:00:00Z".to_string(),
    };
    let process = ServerProcessMetadata {
        pid: 42,
        launch_mode: LaunchMode::Background,
        started_at: "2026-05-17T00:00:01Z".to_string(),
    };

    let spec_body = toml::to_string_pretty(&spec).expect("serialize spec");
    assert!(spec_body.contains("runtime_kind = \"local\""));
    assert!(spec_body.contains("lazy_load = true"));
    let parsed_spec: ServerSpec = toml::from_str(&spec_body).expect("parse spec");
    assert_eq!(parsed_spec, spec);

    let process_body = toml::to_string_pretty(&process).expect("serialize process");
    assert!(process_body.contains("launch_mode = \"background\""));
    let parsed_process: ServerProcessMetadata =
        toml::from_str(&process_body).expect("parse process");
    assert_eq!(parsed_process, process);
}
