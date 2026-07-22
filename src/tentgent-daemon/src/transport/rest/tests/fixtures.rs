use super::*;

pub(super) async fn json_body(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    serde_json::from_slice(&bytes).expect("json")
}

pub(super) async fn sse_body(response: axum::response::Response) -> String {
    String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body")
            .to_vec(),
    )
    .expect("utf8")
}

pub(super) fn rest_state(label: &str) -> RestState {
    let home = unique_home(label);
    let state = rest_state_for_home(home.clone());
    let _ = fs::remove_dir_all(home);
    state
}

pub(super) fn rest_state_for_home(home: PathBuf) -> RestState {
    rest_state_for_home_with_security(home, DaemonSecurityConfig::disabled())
}

pub(super) fn rest_state_for_home_with_security(
    home: PathBuf,
    security: DaemonSecurityConfig,
) -> RestState {
    let config = DaemonBootstrapConfig {
        home: Some(home.clone()),
        logging: LoggingConfig {
            enabled: false,
            env_filter: None,
        },
        rest: RestConfig::default(),
    };
    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: Some(home.clone()),
            data_root_dir: None,
        })
        .expect("layout");
    let services = DaemonServices::bootstrap(&config).expect("services");
    let state = DaemonAppState::new(
        services,
        LoggingRuntime::disabled(),
        layout,
        RestConfig::default(),
    );
    RestState::with_security(std::sync::Arc::new(state), security)
}

pub(super) fn unique_home(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "tentgent-daemon-rest-{label}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ))
}

pub(super) fn write_model_fixture(home: &std::path::Path, model_ref: &str) {
    write_model_fixture_with_capabilities(home, model_ref, &["chat", "embedding"]);
}

pub(super) fn write_cluster_fixture(home: &std::path::Path, cluster_ref: &str, model_ref: &str) {
    let cluster_dir = home.join("clusters").join(cluster_ref);
    fs::create_dir_all(&cluster_dir).expect("cluster dir");
    fs::write(
        cluster_dir.join("cluster.toml"),
        format!(
            r#"schema_version = 1
cluster_ref = "{cluster_ref}"

[routes.chat]
kind = "local-model"
model_ref = "{model_ref}"
"#
        ),
    )
    .expect("cluster definition");
}

pub(super) fn write_model_fixture_with_capabilities(
    home: &std::path::Path,
    model_ref: &str,
    capabilities: &[&str],
) {
    let store_dir = home.join("models/store").join(model_ref);
    fs::create_dir_all(store_dir.join("variants/mlx/source")).expect("model source dir");
    fs::write(store_dir.join("manifest.json"), "{}").expect("manifest");
    write_model_variant_fixture(&store_dir, "mlx", capabilities);
    let capabilities = capabilities
        .iter()
        .map(|capability| format!(r#""{capability}""#))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        store_dir.join("model.toml"),
        format!(
            r#"model_ref = "{model_ref}"
short_ref = "{}"
source_kind = "local"
source_path = "{}"
primary_format = "mlx"
detected_formats = ["mlx"]
model_capabilities = [{capabilities}]
model_capability_source = "explicit-user"
file_count = 1
total_bytes = 10
imported_at = "2026-05-01T00:00:00Z"
"#,
            &model_ref[..12],
            path_string(home.join("fixtures/model"))
        ),
    )
    .expect("model metadata");
}

pub(super) fn write_safetensors_model_fixture_with_capabilities(
    home: &std::path::Path,
    model_ref: &str,
    capabilities: &[&str],
) {
    let store_dir = home.join("models/store").join(model_ref);
    fs::create_dir_all(store_dir.join("variants/safetensors/source")).expect("model source dir");
    fs::write(store_dir.join("manifest.json"), "{}").expect("manifest");
    write_model_variant_fixture(&store_dir, "safetensors", capabilities);
    let capabilities = capabilities
        .iter()
        .map(|capability| format!(r#""{capability}""#))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        store_dir.join("model.toml"),
        format!(
            r#"model_ref = "{model_ref}"
short_ref = "{}"
source_kind = "local"
source_path = "{}"
primary_format = "safetensors"
detected_formats = ["safetensors"]
model_capabilities = [{capabilities}]
model_capability_source = "explicit-user"
file_count = 1
total_bytes = 10
imported_at = "2026-05-01T00:00:00Z"
"#,
            &model_ref[..12],
            path_string(home.join("fixtures/model"))
        ),
    )
    .expect("model metadata");
}

pub(super) fn write_model_capability_proof(
    home: &std::path::Path,
    model_ref: &str,
    capability: ModelCapability,
    status: ModelCapabilityProofStatus,
    backend: &str,
    runtime_profile: Option<(&str, u32)>,
    error: Option<&str>,
) {
    let store = FileModelCapabilityProofStore;
    let layout = tentgent_kernel::features::model::domain::ModelStoreLayout::from_models_dir(
        home.join("models"),
    );
    store
        .save_capability_proof(
            &layout,
            &ModelCapabilityProof {
                model_ref: ModelRef::parse(model_ref).expect("model ref"),
                capability,
                status,
                source: ModelCapabilityProofSource::ManualProbe,
                primary_format: ModelFormat::Safetensors,
                mlx_runtime_family: None,
                backend: backend.to_string(),
                runtime_version: None,
                runtime_profile: runtime_profile.map(|(profile_id, _)| profile_id.to_string()),
                runtime_profile_version: runtime_profile.map(|(_, version)| version),
                server_ref: None,
                checked_at: "2026-07-08T00:00:00Z".to_string(),
                error: error.map(str::to_string),
            },
        )
        .expect("save capability proof");
}

pub(super) fn write_model_variant_fixture(
    store_dir: &std::path::Path,
    format: &str,
    capabilities: &[&str],
) {
    let variant_dir = store_dir.join("variants").join(format);
    let source_dir = variant_dir.join("source");
    fs::create_dir_all(&source_dir).expect("model source dir");
    fs::write(
        variant_dir.join("variant.toml"),
        format!(
            r#"format = "{format}"
status = "imported"
import_method = "add"
relative_source_path = "source"
"#
        ),
    )
    .expect("variant metadata");
    fs::write(source_dir.join("config.json"), "{}").expect("config");
    if capabilities
        .iter()
        .any(|capability| matches!(*capability, "chat" | "embedding" | "rerank"))
    {
        fs::write(source_dir.join("tokenizer.json"), "{}").expect("tokenizer");
    }
    if capabilities.iter().any(|capability| {
        matches!(
            *capability,
            "audio-transcription" | "audio-speech" | "vision-chat" | "video-understanding"
        )
    }) {
        fs::write(source_dir.join("processor_config.json"), "{}").expect("processor");
    }
}

pub(super) fn write_adapter_fixture(
    home: &std::path::Path,
    adapter_ref: &str,
    base_model_ref: &str,
) {
    let store_dir = home.join("adapters/store").join(adapter_ref);
    fs::create_dir_all(store_dir.join("source")).expect("adapter source dir");
    fs::write(store_dir.join("manifest.json"), "{}").expect("manifest");
    fs::write(
        store_dir.join("adapter.toml"),
        format!(
            r#"adapter_ref = "{adapter_ref}"
short_ref = "{}"
adapter_format = "mlx"
adapter_type = "lora"
base_model_ref = "{base_model_ref}"
base_model_source_repo = "mlx-community/base-model"
base_model_source_revision = "resolved-sha"
model_family = "qwen"
backend_support = ["mlx"]
source_kind = "local"
source_path = "{}"
training_dataset_ref = "dataset-ref"
training_run_ref = "run-ref"
training_config_ref = "config-ref"
file_count = 1
total_bytes = 10
imported_at = "2026-05-01T00:00:00Z"
"#,
            &adapter_ref[..12],
            path_string(home.join("fixtures/adapter"))
        ),
    )
    .expect("adapter metadata");
}

pub(super) fn write_dataset_fixture(home: &std::path::Path, dataset_ref: &str) {
    let store_dir = home.join("datasets/store").join(dataset_ref);
    fs::create_dir_all(store_dir.join("source")).expect("dataset source dir");
    fs::write(store_dir.join("manifest.json"), "{}").expect("manifest");
    fs::write(
        store_dir.join("dataset.toml"),
        format!(
            r#"dataset_ref = "{dataset_ref}"
short_ref = "{}"
source_kind = "local"
source_path = "{}"
dataset_format = "directory"
file_count = 2
total_bytes = 20
imported_at = "2026-05-01T00:00:00Z"

[package]
tuning_ready = true
warnings = ["small training set"]

[package.splits]
train = "train.jsonl"
validation = "valid.jsonl"
test = "test.jsonl"
eval_cases = "eval_cases.jsonl"
source_manifest = "manifest.json"
"#,
            &dataset_ref[..12],
            path_string(home.join("fixtures/dataset"))
        ),
    )
    .expect("dataset metadata");
}

pub(super) fn write_server_fixture(home: &std::path::Path, server_ref: &str, model_ref: &str) {
    write_server_fixture_with_capability(home, server_ref, model_ref, None);
}

pub(super) fn write_server_fixture_with_capability(
    home: &std::path::Path,
    server_ref: &str,
    model_ref: &str,
    capability: Option<&str>,
) {
    let server_dir = home.join("servers").join(server_ref);
    fs::create_dir_all(&server_dir).expect("server dir");
    let capability = capability
        .map(|capability| format!("capability = \"{capability}\"\n"))
        .unwrap_or_default();
    fs::write(
        server_dir.join("server.toml"),
        format!(
            r#"server_ref = "{server_ref}"
short_ref = "{}"
runtime_kind = "local"
{capability}model_ref = "{model_ref}"
host = "127.0.0.1"
port = 8999
lazy_load = false
idle_seconds = 60
created_at = "2026-05-01T00:00:00Z"

[runtime_profile]
profile_id = "local-chat-mlx"
profile_version = 1
"#,
            &server_ref[..12]
        ),
    )
    .expect("server spec");
}

pub(super) fn write_server_process_fixture(home: &std::path::Path, server_ref: &str, pid: u32) {
    fs::write(
        home.join("servers").join(server_ref).join("process.toml"),
        format!(
            r#"pid = {pid}
launch_mode = "background"
started_at = "2026-05-01T00:00:00Z"
"#
        ),
    )
    .expect("server process metadata");
}

pub(super) fn write_session_fixture(
    home: &std::path::Path,
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
default_server_ref = "server-ref"
adapter_ref = "adapter-ref"
tags = ["smoke", "rest"]
"#,
            &session_ref[..12]
        ),
    )
    .expect("session metadata");
    if let Some(messages) = messages {
        fs::write(
            session_dir.join("messages.jsonl"),
            messages.join("\n") + "\n",
        )
        .expect("session messages");
    }
}

pub(super) fn session_message(role: &str, content: &str) -> String {
    format!(
        r#"{{"schema":"tentgent.session.message.v1","role":"{role}","content":"{content}","created_at":"2026-05-01T00:00:00Z","metadata":{{}}}}"#
    )
}

pub(super) fn sample_dataset_record() -> &'static str {
    r#"{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hi"}]}"#
}

pub(super) struct MultipartPart {
    pub(super) name: String,
    pub(super) filename: Option<String>,
    pub(super) content_type: Option<String>,
    pub(super) body: Vec<u8>,
}

impl MultipartPart {
    pub(super) fn text(name: impl Into<String>, body: impl AsRef<str>) -> Self {
        Self {
            name: name.into(),
            filename: None,
            content_type: None,
            body: body.as_ref().as_bytes().to_vec(),
        }
    }

    pub(super) fn file(
        name: impl Into<String>,
        filename: impl Into<String>,
        content_type: impl Into<String>,
        body: impl AsRef<[u8]>,
    ) -> Self {
        Self {
            name: name.into(),
            filename: Some(filename.into()),
            content_type: Some(content_type.into()),
            body: body.as_ref().to_vec(),
        }
    }
}

pub(super) fn multipart_body(boundary: &str, parts: &[MultipartPart]) -> Vec<u8> {
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        match part.filename.as_deref() {
            Some(filename) => body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                    part.name, filename
                )
                .as_bytes(),
            ),
            None => body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{}\"\r\n", part.name).as_bytes(),
            ),
        }
        if let Some(content_type) = part.content_type.as_deref() {
            body.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
        }
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(&part.body);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    body
}

pub(super) fn path_string(path: impl AsRef<std::path::Path>) -> String {
    path.as_ref().display().to_string()
}
