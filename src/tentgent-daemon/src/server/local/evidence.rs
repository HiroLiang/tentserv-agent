use axum::{http::StatusCode, response::Response};
use tentgent_kernel::{
    features::model::{
        domain::{ModelCapabilityProofStatus, ModelRefSelector, ModelStoreLayout},
        infra::{FileModelCapabilityProofStore, FileModelCatalogStore, SystemModelClock},
        ports::ModelCatalogStore,
        usecases::{
            ModelRuntimeExecutionEvidenceRecordRequest, ModelRuntimeExecutionEvidenceRecorder,
            StdModelRuntimeExecutionEvidenceRecorder,
        },
    },
    foundation::error::KernelResult,
};

use super::{error::LocalServerError, LocalServerState};

pub(super) fn record_runtime_execution_response(state: &LocalServerState, response: &Response) {
    if response.status().is_server_error() {
        record_failure(
            state,
            format!("model runtime returned HTTP {}", response.status()),
        );
    }
}

pub(super) fn record_runtime_execution_error(state: &LocalServerState, error: &LocalServerError) {
    if records_runtime_failure(error.status) {
        record_failure(state, error.message.clone());
    }
}

pub(super) fn record_runtime_execution_result(
    state: &LocalServerState,
    result: &Result<Response, LocalServerError>,
) {
    match result {
        Ok(response) => record_runtime_execution_response(state, response),
        Err(error) => record_runtime_execution_error(state, error),
    }
}

fn records_runtime_failure(status: StatusCode) -> bool {
    status.is_server_error() || status == StatusCode::BAD_GATEWAY
}

fn record_failure(state: &LocalServerState, error: String) {
    let _ = try_record_failure(state, error);
}

fn try_record_failure(state: &LocalServerState, error: String) -> KernelResult<()> {
    let model_store = ModelStoreLayout::from_models_dir(state.layout.models_dir.clone());
    let selector = ModelRefSelector::parse(&state.config.model_ref).map_err(|err| {
        tentgent_kernel::foundation::error::KernelError::ModelStoreUnavailable(format!(
            "invalid model ref in local server config: {err}"
        ))
    })?;
    let catalog = FileModelCatalogStore;
    let model = catalog.inspect_model(&model_store, &selector)?;
    let proofs = FileModelCapabilityProofStore;
    let clock = SystemModelClock;
    let recorder = StdModelRuntimeExecutionEvidenceRecorder::new(&proofs, &clock);
    let (runtime_profile, runtime_profile_version) =
        runtime_profile_parts(state.config.runtime_profile.as_deref());

    recorder.record_runtime_execution_evidence(ModelRuntimeExecutionEvidenceRecordRequest {
        layout: state.layout.clone(),
        metadata: model.metadata,
        capability: state.config.capability.required_model_capability(),
        status: ModelCapabilityProofStatus::Failed,
        server_ref: Some(state.config.server_ref.clone()),
        runtime_profile,
        runtime_profile_version,
        error: Some(error),
    })?;
    Ok(())
}

fn runtime_profile_parts(label: Option<&str>) -> (Option<String>, Option<u32>) {
    let Some(label) = label else {
        return (None, None);
    };
    match label.rsplit_once("-v") {
        Some((profile_id, version)) => match version.parse::<u32>() {
            Ok(version) => (Some(profile_id.to_string()), Some(version)),
            Err(_) => (Some(label.to_string()), None),
        },
        None => (Some(label.to_string()), None),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use axum::{body::Body, http::StatusCode};
    use tentgent_kernel::{
        features::{
            model::{
                domain::{
                    default_model_capability_source, ModelCapability, ModelCapabilityProofSource,
                    ModelFormat, ModelMetadata, ModelRef, ModelSourceKind, ModelStoreLayout,
                },
                infra::{FileModelCapabilityProofStore, FileModelCatalogStore},
                ports::{ModelCapabilityProofStore, ModelCatalogStore},
            },
            runtime::{
                domain::{PythonRuntimeLayout, PythonRuntimeSource},
                infra::{ModelRuntimeDaemonLaunchPolicy, ModelRuntimeDaemonSupervisor},
            },
            server::domain::ServerCapability,
        },
        foundation::layout::RuntimeLayout,
    };

    use super::{record_runtime_execution_response, runtime_profile_parts};
    use crate::server::local::{LocalServerRuntimeConfig, LocalServerState};

    #[test]
    fn parses_runtime_profile_label() {
        assert_eq!(
            runtime_profile_parts(Some("local-chat-mlx-v1")),
            (Some("local-chat-mlx".to_string()), Some(1))
        );
        assert_eq!(
            runtime_profile_parts(Some("legacy-profile")),
            (Some("legacy-profile".to_string()), None)
        );
        assert_eq!(runtime_profile_parts(None), (None, None));
    }

    #[test]
    fn records_runtime_execution_failure_for_server_error_response() {
        let home = unique_home("local-server-runtime-evidence");
        let layout = runtime_layout(&home);
        let model_ref = model_ref();
        let model_store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        let catalog = FileModelCatalogStore;
        catalog
            .save_model_metadata(&model_store, &model_metadata(&model_ref))
            .expect("save model metadata");
        let state = local_server_state(layout.clone(), &model_ref);
        let response = axum::response::Response::builder()
            .status(StatusCode::BAD_GATEWAY)
            .body(Body::empty())
            .expect("response");

        record_runtime_execution_response(&state, &response);

        let proofs = FileModelCapabilityProofStore
            .list_capability_proofs(&model_store, &model_ref)
            .expect("proofs");
        assert_eq!(proofs.len(), 1);
        assert_eq!(proofs[0].capability, ModelCapability::Chat);
        assert_eq!(
            proofs[0].source,
            ModelCapabilityProofSource::RuntimeExecution
        );
        assert_eq!(proofs[0].server_ref.as_deref(), Some("server-ref"));
        assert_eq!(proofs[0].runtime_profile.as_deref(), Some("local-chat-mlx"));
        assert_eq!(proofs[0].runtime_profile_version, Some(1));
        assert!(proofs[0]
            .error
            .as_deref()
            .is_some_and(|error| error.contains("HTTP 502 Bad Gateway")));
    }

    fn local_server_state(layout: RuntimeLayout, model_ref: &ModelRef) -> LocalServerState {
        LocalServerState {
            config: LocalServerRuntimeConfig {
                server_ref: "server-ref".to_string(),
                capability: ServerCapability::Chat,
                model_ref: model_ref.to_string(),
                runtime_profile: Some("local-chat-mlx-v1".to_string()),
                host: "127.0.0.1".to_string(),
                port: 0,
                runtime_home: Some(layout.home_dir.clone()),
                idle_seconds: None,
            },
            runtime: PythonRuntimeLayout {
                project_dir: layout.runtime_dir.join("project"),
                env_dir: layout.python_env_dir.clone(),
                source: PythonRuntimeSource::DevelopmentSource,
            },
            layout,
            executable_resolver:
                tentgent_kernel::features::runtime::infra::StdRuntimeExecutableResolver,
            supervisor: ModelRuntimeDaemonSupervisor::new(),
            client: reqwest::Client::new(),
            launch_policy: ModelRuntimeDaemonLaunchPolicy::default(),
        }
    }

    fn model_metadata(model_ref: &ModelRef) -> ModelMetadata {
        ModelMetadata {
            model_ref: model_ref.clone(),
            short_ref: model_ref.short_ref().to_string(),
            source_kind: ModelSourceKind::Local,
            source_repo: None,
            source_revision: None,
            source_path: Some("/tmp/model".to_string()),
            primary_format: ModelFormat::Safetensors,
            detected_formats: vec![ModelFormat::Safetensors],
            mlx_runtime_family: None,
            model_capabilities: vec![ModelCapability::Chat],
            model_capability_source: default_model_capability_source(),
            file_count: 1,
            total_bytes: 1,
            imported_at: "2026-06-12T00:00:00Z".to_string(),
        }
    }

    fn model_ref() -> ModelRef {
        ModelRef::parse("7".repeat(64)).expect("model ref")
    }

    fn runtime_layout(home: &std::path::Path) -> RuntimeLayout {
        RuntimeLayout {
            home_dir: home.to_path_buf(),
            data_root_dir: home.join("data"),
            config_path: home.join("config.toml"),
            models_dir: home.join("models"),
            adapters_dir: home.join("adapters"),
            datasets_dir: home.join("datasets"),
            sessions_dir: home.join("sessions"),
            servers_dir: home.join("servers"),
            train_dir: home.join("train"),
            cache_dir: home.join("cache"),
            runtime_dir: home.join("runtime"),
            logs_dir: home.join("logs"),
            locks_dir: home.join("locks"),
            python_env_dir: home.join("runtime/python"),
            bootstrap_dir: home.join("bootstrap"),
            bootstrap_uv_dir: home.join("bootstrap/uv"),
            bootstrap_uv_cache_dir: home.join("bootstrap/uv-cache"),
            capabilities_path: home.join("runtime/capabilities.toml"),
            auth_metadata_path: home.join("runtime/auth.toml"),
        }
    }

    fn unique_home(prefix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("tentgent-{prefix}-{nanos}"))
    }
}
