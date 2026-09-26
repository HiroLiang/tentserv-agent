use super::*;
use clap::Parser;
use std::path::PathBuf;
use tentgent_kernel::features::model::domain::ModelRef;
use tentgent_kernel::features::server::domain::ServerRef;

#[tokio::test]
async fn server_run_rejects_each_cloud_lifecycle_flag_before_auth_or_launch() {
    for flags in [
        vec!["--lazy-load"],
        vec!["--runtime-idle-seconds", "300"],
        vec!["--idle-seconds", "0"],
        vec!["--model-idle-seconds", "0"],
    ] {
        let home = std::env::temp_dir().join(format!(
            "tentgent-cli-cloud-options-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut args = vec![
            "tentgent",
            "server",
            "run",
            "openai:gpt-4.1-mini",
            "--home",
            home.to_str().unwrap(),
        ];
        args.extend(flags);
        let cli = crate::cli::app::Cli::try_parse_from(args).unwrap();
        let crate::cli::commands::Commands::Server { action } = cli.command else {
            panic!("server command");
        };
        let error = handle_server_command(action).await.unwrap_err().to_string();
        assert!(
            error.contains("Cloud lifecycle options are not applicable"),
            "{error}"
        );
        let _ = std::fs::remove_dir_all(home);
    }
}

#[test]
fn hidden_cloud_worker_rejects_removed_lifecycle_flags() {
    let base = [
        "tentgent",
        "__cloud-server-runtime",
        "--server-ref",
        "server",
        "--provider",
        "openai",
        "--provider-model",
        "test",
        "--host",
        "127.0.0.1",
        "--port",
        "8780",
    ];
    assert!(crate::cli::app::Cli::try_parse_from(base).is_ok());
    for flags in [
        vec!["--lazy-load"],
        vec!["--idle-seconds", "30"],
        vec!["--runtime-idle-seconds", "30"],
        vec!["--model-idle-seconds", "0"],
    ] {
        let mut args = base.to_vec();
        args.extend(flags);
        assert!(crate::cli::app::Cli::try_parse_from(args).is_err());
    }
}

#[tokio::test]
async fn hidden_local_worker_rejects_image_eager_before_runtime_work() {
    let error = handle_local_server_runtime(LocalServerRuntimeCommand {
        server_ref: "server".into(),
        capability: "image-generation".into(),
        model_ref: "missing".into(),
        runtime_profile: None,
        host: "127.0.0.1".into(),
        port: 0,
        home: None,
        lazy_load: false,
        runtime_idle_seconds: 300,
        model_idle_seconds: 0,
    })
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("requires --lazy-load"), "{error}");
}

#[test]
fn cloud_inspect_marks_legacy_options_without_hiding_raw_values() {
    let mut spec = cloud_server_spec();
    spec.lazy_load = true;
    spec.idle_seconds = Some(42);
    let inspection = ServerInspection {
        spec,
        home_dir: PathBuf::new(),
        server_dir: PathBuf::new(),
        spec_path: PathBuf::new(),
        process_path: PathBuf::new(),
        stdout_log_path: PathBuf::new(),
        stderr_log_path: PathBuf::new(),
        running: false,
        process: None,
    };
    let output = render_server_table(&inspection).to_string();
    assert!(output.contains("legacy stored values ignored"));
    assert!(output.contains("lazy_load") && output.contains("true"));
    assert!(output.contains("idle_seconds") && output.contains("42"));
}

#[test]
fn server_list_target_label_shortens_local_model_refs() {
    let spec = local_server_spec();

    assert_eq!(server_list_target_label(&spec), "abcdefabcdef");
}

#[test]
fn server_list_target_label_keeps_cloud_provider_model_names() {
    let spec = cloud_server_spec();

    assert_eq!(server_list_target_label(&spec), "gpt-4o-mini");
}

#[test]
fn server_list_target_label_uses_cluster_ref_for_cluster_targets() {
    let spec = cluster_server_spec();

    assert_eq!(server_list_target_label(&spec), "local-assistant");
}

#[test]
fn runtime_idle_alias_accepts_matching_values_and_rejects_conflicts() {
    assert_eq!(resolve_runtime_idle_alias(None, None).unwrap(), None);
    assert_eq!(
        resolve_runtime_idle_alias(Some(30), None).unwrap(),
        Some(30)
    );
    assert_eq!(
        resolve_runtime_idle_alias(None, Some(30)).unwrap(),
        Some(30)
    );
    assert_eq!(
        resolve_runtime_idle_alias(Some(30), Some(30)).unwrap(),
        Some(30)
    );

    let error = resolve_runtime_idle_alias(Some(30), Some(31)).expect_err("conflict");
    assert!(error.to_string().contains("must match"));
}

fn local_server_spec() -> ServerSpec {
    let model_ref =
        ModelRef::parse("abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd")
            .expect("model ref");

    ServerSpec {
        server_ref: server_ref(),
        short_ref: "0123456789ab".to_string(),
        runtime_kind: ServerRuntimeKind::Local,
        capability: Some(ServerCapability::Chat),
        model_ref: Some(model_ref),
        provider: None,
        provider_model: None,
        cluster_ref: None,
        runtime_profile: None,
        host: "127.0.0.1".to_string(),
        port: 8780,
        port_auto: false,
        lazy_load: false,
        idle_seconds: None,
        model_idle_seconds: None,
        created_at: "2026-06-15T00:00:00Z".to_string(),
    }
}

fn cloud_server_spec() -> ServerSpec {
    ServerSpec {
        server_ref: server_ref(),
        short_ref: "0123456789ab".to_string(),
        runtime_kind: ServerRuntimeKind::Cloud,
        capability: Some(ServerCapability::Chat),
        model_ref: None,
        provider: Some(CloudProvider::OpenAI),
        provider_model: Some("gpt-4o-mini".to_string()),
        cluster_ref: None,
        runtime_profile: None,
        host: "127.0.0.1".to_string(),
        port: 8780,
        port_auto: false,
        lazy_load: false,
        idle_seconds: None,
        model_idle_seconds: None,
        created_at: "2026-06-15T00:00:00Z".to_string(),
    }
}

fn cluster_server_spec() -> ServerSpec {
    ServerSpec {
        server_ref: server_ref(),
        short_ref: "0123456789ab".to_string(),
        runtime_kind: ServerRuntimeKind::Cluster,
        capability: None,
        model_ref: None,
        provider: None,
        provider_model: None,
        cluster_ref: Some(ClusterRef::parse("local-assistant").expect("cluster ref")),
        runtime_profile: None,
        host: "127.0.0.1".to_string(),
        port: 8780,
        port_auto: false,
        lazy_load: false,
        idle_seconds: None,
        model_idle_seconds: None,
        created_at: "2026-07-12T00:00:00Z".to_string(),
    }
}

fn server_ref() -> ServerRef {
    ServerRef::parse("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        .expect("server ref")
}
