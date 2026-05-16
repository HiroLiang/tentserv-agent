use super::domain::{
    default_daemon_url, resolve_daemon_token, resolve_daemon_url, DaemonConfig, DaemonEndpoint,
    DaemonTokenSource, DaemonUrlInputs, DaemonUrlSource, TentgentConfig, TuiConfig,
    CONFIG_SCHEMA_VERSION, DEFAULT_DAEMON_HOST, DEFAULT_DAEMON_PORT,
};

#[test]
fn config_defaults_match_user_surface_contract() {
    let config = TentgentConfig::default();

    assert_eq!(config.schema_version, CONFIG_SCHEMA_VERSION);
    assert_eq!(
        config.tui,
        TuiConfig {
            last_section: "status".to_string(),
            auto_start_daemon: false,
        }
    );
    assert_eq!(config.daemon, DaemonConfig { url: None });
    assert_eq!(default_daemon_url(), "http://127.0.0.1:8790");
}

#[test]
fn daemon_url_resolution_precedence_is_flag_env_config_metadata_default() {
    let endpoint = DaemonEndpoint {
        host: "127.0.0.1".to_string(),
        port: 9000,
    };
    let all = resolve_daemon_url(DaemonUrlInputs {
        flag_url: Some("http://flag:1"),
        env_url: Some("http://env:2"),
        config_url: Some("http://config:3"),
        metadata_endpoint: Some(&endpoint),
    });
    assert_eq!(all.source, DaemonUrlSource::Flag);
    assert_eq!(all.url, "http://flag:1");

    let env = resolve_daemon_url(DaemonUrlInputs {
        flag_url: None,
        env_url: Some("http://env:2"),
        config_url: Some("http://config:3"),
        metadata_endpoint: Some(&endpoint),
    });
    assert_eq!(env.source, DaemonUrlSource::Env);

    let config = resolve_daemon_url(DaemonUrlInputs {
        flag_url: None,
        env_url: None,
        config_url: Some("http://config:3"),
        metadata_endpoint: Some(&endpoint),
    });
    assert_eq!(config.source, DaemonUrlSource::Config);

    let metadata = resolve_daemon_url(DaemonUrlInputs {
        flag_url: None,
        env_url: None,
        config_url: None,
        metadata_endpoint: Some(&endpoint),
    });
    assert_eq!(metadata.source, DaemonUrlSource::Metadata);
    assert_eq!(metadata.url, "http://127.0.0.1:9000");

    let defaulted = resolve_daemon_url(DaemonUrlInputs {
        flag_url: None,
        env_url: None,
        config_url: None,
        metadata_endpoint: None,
    });
    assert_eq!(defaulted.source, DaemonUrlSource::Default);
    assert_eq!(defaulted.url, default_daemon_url());
}

#[test]
fn invalid_config_daemon_url_falls_back_with_error() {
    let endpoint = DaemonEndpoint {
        host: "127.0.0.1".to_string(),
        port: 9000,
    };
    let with_metadata = resolve_daemon_url(DaemonUrlInputs {
        flag_url: None,
        env_url: None,
        config_url: Some("not-a-url"),
        metadata_endpoint: Some(&endpoint),
    });

    assert_eq!(with_metadata.source, DaemonUrlSource::Metadata);
    assert_eq!(with_metadata.url, "http://127.0.0.1:9000");
    assert_eq!(
        with_metadata
            .config_error
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("daemon URL `not-a-url` from config must be an absolute http or https URL")
    );
}

#[test]
fn token_resolution_precedence_is_flag_env_none() {
    let flag = resolve_daemon_token(Some(" flag "), Some("env"));
    assert_eq!(flag.source, DaemonTokenSource::Flag);
    assert_eq!(flag.token.as_deref(), Some("flag"));

    let env = resolve_daemon_token(None, Some(" env "));
    assert_eq!(env.source, DaemonTokenSource::Env);
    assert_eq!(env.token.as_deref(), Some("env"));

    let none = resolve_daemon_token(Some(" "), Some(""));
    assert_eq!(none.source, DaemonTokenSource::None);
    assert!(none.token.is_none());
}

#[test]
fn daemon_endpoint_formats_ipv4_ipv6_and_defaults() {
    assert_eq!(
        DaemonEndpoint::default(),
        DaemonEndpoint {
            host: DEFAULT_DAEMON_HOST.to_string(),
            port: DEFAULT_DAEMON_PORT,
        }
    );
    assert_eq!(super::domain::daemon_url("::1", 8790), "http://[::1]:8790");
    assert_eq!(
        super::domain::daemon_url("[::1]", 8790),
        "http://[::1]:8790"
    );
}

#[test]
fn secret_like_key_detection_matches_config_guardrail() {
    assert!(super::domain::is_secret_like_config_key("token"));
    assert!(super::domain::is_secret_like_config_key("api_key"));
    assert!(super::domain::is_secret_like_config_key("client_secret"));
    assert!(!super::domain::is_secret_like_config_key("daemon_url"));
}
