use std::path::Path;

use tentgent_core::{
    adapter::{AdapterManager, AdapterSummary},
    dataset::{DatasetManager, DatasetSummary},
    model::{ModelManager, ModelSummary},
    server::{
        ServerInspection, ServerManager, ServerPrepareOutcome, ServerProcessMetadata, ServerSummary,
    },
};

use crate::{
    app::DaemonHttpState,
    dto::{
        AdapterItem, AdaptersResponse, DatasetItem, DatasetSplitsItem, DatasetsResponse, ModelItem,
        ModelsResponse, ServerInspectionItem, ServerProcessItem, ServerResponse, ServerSummaryItem,
        ServersResponse,
    },
    http::HttpResponse,
    response::{json_response, manager_error_response, server_error_response},
};

pub(crate) fn list_models_response(state: &DaemonHttpState) -> HttpResponse {
    let manager = match ModelManager::open_readonly_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return manager_error_response("models", error),
    };
    match manager.list_models() {
        Ok(models) => json_response(
            200,
            ModelsResponse {
                models: models.into_iter().map(model_item).collect(),
            },
        ),
        Err(error) => manager_error_response("models", error),
    }
}

pub(crate) fn list_adapters_response(state: &DaemonHttpState) -> HttpResponse {
    let manager = match AdapterManager::open_readonly_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return manager_error_response("adapters", error),
    };
    match manager.list_adapters() {
        Ok(adapters) => json_response(
            200,
            AdaptersResponse {
                adapters: adapters.into_iter().map(adapter_item).collect(),
            },
        ),
        Err(error) => manager_error_response("adapters", error),
    }
}

pub(crate) fn list_datasets_response(state: &DaemonHttpState) -> HttpResponse {
    let manager = match DatasetManager::open_readonly_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return manager_error_response("datasets", error),
    };
    match manager.list_datasets() {
        Ok(datasets) => json_response(
            200,
            DatasetsResponse {
                datasets: datasets.into_iter().map(dataset_item).collect(),
            },
        ),
        Err(error) => manager_error_response("datasets", error),
    }
}

pub(crate) fn list_servers_response(state: &DaemonHttpState) -> HttpResponse {
    let manager = match ServerManager::open_readonly(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return server_error_response(error),
    };
    match manager.list() {
        Ok(servers) => json_response(
            200,
            ServersResponse {
                servers: servers.into_iter().map(server_summary_item).collect(),
            },
        ),
        Err(error) => server_error_response(error),
    }
}

pub(crate) fn inspect_server_response(state: &DaemonHttpState, reference: &str) -> HttpResponse {
    let manager = match ServerManager::open_readonly(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return server_error_response(error),
    };
    match manager.inspect(reference) {
        Ok(server) => json_response(
            200,
            ServerResponse {
                server: server_inspection_item(server),
            },
        ),
        Err(error) => server_error_response(error),
    }
}

fn model_item(summary: ModelSummary) -> ModelItem {
    let metadata = summary.metadata;
    ModelItem {
        model_ref: metadata.model_ref,
        short_ref: metadata.short_ref,
        store_path: path_string(&summary.store_path),
        file_count: metadata.file_count,
        total_bytes: metadata.total_bytes,
        imported_at: metadata.imported_at,
        format: metadata.primary_format.to_string(),
        detected_formats: metadata
            .detected_formats
            .into_iter()
            .map(|format| format.to_string())
            .collect(),
        source_kind: metadata.source_kind.to_string(),
        source_repo: metadata.source_repo,
        source_revision: metadata.source_revision,
        source_path: metadata.source_path,
    }
}

fn adapter_item(summary: AdapterSummary) -> AdapterItem {
    let metadata = summary.metadata;
    AdapterItem {
        adapter_ref: metadata.adapter_ref,
        short_ref: metadata.short_ref,
        store_path: path_string(&summary.store_path),
        file_count: metadata.file_count,
        total_bytes: metadata.total_bytes,
        imported_at: metadata.imported_at,
        format: metadata.adapter_format.to_string(),
        adapter_type: metadata.adapter_type.to_string(),
        base_model_ref: metadata.base_model_ref,
        base_model_source_repo: metadata.base_model_source_repo,
        base_model_source_revision: metadata.base_model_source_revision,
        model_family: metadata.model_family,
        backend_support: metadata.backend_support,
        source_kind: metadata.source_kind.to_string(),
        source_repo: metadata.source_repo,
        source_revision: metadata.source_revision,
        source_path: metadata.source_path,
        training_dataset_ref: metadata.training_dataset_ref,
        training_run_ref: metadata.training_run_ref,
        training_config_ref: metadata.training_config_ref,
    }
}

fn dataset_item(summary: DatasetSummary) -> DatasetItem {
    let metadata = summary.metadata;
    let package = metadata.package;
    DatasetItem {
        dataset_ref: metadata.dataset_ref,
        short_ref: metadata.short_ref,
        store_path: path_string(&summary.store_path),
        file_count: metadata.file_count,
        total_bytes: metadata.total_bytes,
        imported_at: metadata.imported_at,
        format: metadata.dataset_format.to_string(),
        source_kind: metadata.source_kind.to_string(),
        source_path: metadata.source_path,
        source_repo: metadata.source_repo,
        source_revision: metadata.source_revision,
        tuning_ready: package.tuning_ready,
        splits: DatasetSplitsItem {
            train: package.splits.train,
            validation: package.splits.validation,
            test: package.splits.test,
            eval_cases: package.splits.eval_cases,
            source_manifest: package.splits.source_manifest,
        },
        warnings: package.warnings,
    }
}

fn server_summary_item(summary: ServerSummary) -> ServerSummaryItem {
    let spec = summary.spec;
    ServerSummaryItem {
        server_ref: spec.server_ref,
        short_ref: spec.short_ref,
        runtime_kind: spec.runtime_kind.to_string(),
        model_ref: spec.model_ref,
        provider: spec.provider.map(|provider| provider.to_string()),
        provider_model: spec.provider_model,
        host: spec.host,
        port: spec.port,
        lazy_load: spec.lazy_load,
        idle_seconds: spec.idle_seconds,
        created_at: spec.created_at,
        running: summary.running,
        process: summary.process.map(server_process_item),
    }
}

pub(crate) fn server_inspection_item(inspection: ServerInspection) -> ServerInspectionItem {
    let spec = inspection.spec;
    ServerInspectionItem {
        server_ref: spec.server_ref,
        short_ref: spec.short_ref,
        runtime_kind: spec.runtime_kind.to_string(),
        model_ref: spec.model_ref,
        provider: spec.provider.map(|provider| provider.to_string()),
        provider_model: spec.provider_model,
        host: spec.host,
        port: spec.port,
        lazy_load: spec.lazy_load,
        idle_seconds: spec.idle_seconds,
        created_at: spec.created_at,
        running: inspection.running,
        process: inspection.process.map(server_process_item),
        home_dir: path_string(&inspection.home_dir),
        server_dir: path_string(&inspection.server_dir),
        spec_path: path_string(&inspection.spec_path),
        process_path: path_string(&inspection.process_path),
        stdout_log: path_string(&inspection.stdout_log_path),
        stderr_log: path_string(&inspection.stderr_log_path),
    }
}

pub(crate) fn server_prepare_item(outcome: ServerPrepareOutcome) -> ServerInspectionItem {
    server_inspection_item(ServerInspection {
        spec: outcome.spec,
        home_dir: outcome.home_dir,
        server_dir: outcome.server_dir,
        spec_path: outcome.spec_path,
        process_path: outcome.process_path,
        stdout_log_path: outcome.stdout_log_path,
        stderr_log_path: outcome.stderr_log_path,
        running: false,
        process: None,
    })
}

fn server_process_item(process: ServerProcessMetadata) -> ServerProcessItem {
    ServerProcessItem {
        pid: process.pid,
        launch_mode: process.launch_mode.to_string(),
        started_at: process.started_at,
    }
}

pub(crate) fn path_string(path: &Path) -> String {
    path.display().to_string()
}
