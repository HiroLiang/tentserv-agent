pub(crate) mod auth;
pub(crate) mod chat;
pub(crate) mod daemon_control;
pub(crate) mod dataset_cloud;
pub(crate) mod dataset_tools;
pub(crate) mod diagnostics;
pub(crate) mod doctor;
pub(crate) mod lifecycle;
pub(crate) mod openai;
pub(crate) mod session;
pub(crate) mod status;
pub(crate) mod store;
pub(crate) mod store_mutation;
pub(crate) mod train;

use crate::{
    app::DaemonHttpState,
    dto::ErrorResponse,
    http::{HttpRequest, HttpResponse},
    response::{json_response, method_not_allowed, not_found_response, unauthorized_response},
};

pub(crate) async fn route_request(request: &HttpRequest, state: &DaemonHttpState) -> HttpResponse {
    if let Some(error) = &request.parse_error {
        return json_response(
            error.status_code,
            ErrorResponse {
                error: "bad_request",
                message: error.message.clone(),
            },
        );
    }

    if request.version != "HTTP/1.1" && request.version != "HTTP/1.0" {
        return json_response(
            400,
            ErrorResponse {
                error: "bad_request",
                message: "unsupported HTTP version".to_string(),
            },
        );
    }

    if requires_daemon_auth(request)
        && state
            .security()
            .authorize_header(request.header("authorization"))
            .is_err()
    {
        return unauthorized_response();
    }

    match request.method.as_str() {
        "GET" => route_get(request, state).await,
        "POST" => route_post(request, state).await,
        "PATCH" => route_patch(request, state).await,
        "DELETE" => route_delete(request, state).await,
        _ => method_not_allowed(request),
    }
}

fn requires_daemon_auth(request: &HttpRequest) -> bool {
    request.path == "/v1" || request.path.starts_with("/v1/")
}

async fn route_get(request: &HttpRequest, state: &DaemonHttpState) -> HttpResponse {
    match request.path.as_str() {
        "/healthz" => status::healthz_response(),
        "/v1/status" => status::status_response(state),
        "/v1/auth" => auth::auth_providers_response(),
        "/v1/doctor" => doctor::doctor_response(state),
        "/v1/models" => store::list_models_response(state),
        "/v1/adapters" => store::list_adapters_response(state),
        "/v1/datasets" => store::list_datasets_response(state),
        "/v1/servers" => store::list_servers_response(state),
        "/v1/sessions" => session::list_sessions_response(state),
        "/v1/train/lora/plans" => train::list_train_plans_response(state),
        "/v1/train/lora/runs" => train::list_train_runs_response(state),
        "/v1/daemon/logs" => diagnostics::daemon_logs_metadata_response(state),
        "/v1/daemon/shutdown" => method_not_allowed(request),
        "/v1/daemon/logs/stdout" => {
            diagnostics::daemon_log_content_response(state, request, "stdout")
        }
        "/v1/daemon/logs/stderr" => {
            diagnostics::daemon_log_content_response(state, request, "stderr")
        }
        path if path.starts_with("/v1/daemon/logs/") => not_found_response(&request.path),
        path if server_action_path(path).is_some() => method_not_allowed(request),
        path if diagnostics::is_server_logs_path(path) => {
            diagnostics::server_logs_response(state, request)
        }
        path if session::session_messages_path(path).is_some() => {
            let reference = session::session_messages_path(path).expect("checked path");
            session::session_messages_response(state, request, reference)
        }
        path if train_plan_static_path(path) => method_not_allowed(request),
        path if train_plan_runs_path(path).is_some() => {
            let reference = train_plan_runs_path(path).expect("checked path");
            train::list_plan_runs_response(state, reference)
        }
        path if train_plan_ref_path(path).is_some() => {
            let reference = train_plan_ref_path(path).expect("checked path");
            train::inspect_train_plan_response(state, reference)
        }
        path if path.starts_with("/v1/train/lora/plans/") => not_found_response(&request.path),
        path if train_run_metrics_path(path).is_some() => {
            let reference = train_run_metrics_path(path).expect("checked path");
            train::train_run_metrics_response(state, request, reference)
        }
        path if train_run_raw_log_path(path).is_some() => {
            let reference = train_run_raw_log_path(path).expect("checked path");
            train::train_run_raw_log_response(state, request, reference)
        }
        path if train_run_logs_path(path).is_some() => {
            let reference = train_run_logs_path(path).expect("checked path");
            train::train_run_logs_response(state, reference)
        }
        path if train_run_ref_path(path).is_some() => {
            let reference = train_run_ref_path(path).expect("checked path");
            train::inspect_train_run_response(state, reference)
        }
        path if path.starts_with("/v1/train/lora/runs/") => not_found_response(&request.path),
        path if auth_provider_path(path).is_some() => {
            let provider = auth_provider_path(path).expect("checked path");
            auth::auth_provider_response(provider)
        }
        path if path.starts_with("/v1/auth/") => not_found_response(&request.path),
        path if dataset_tool_path(path) => method_not_allowed(request),
        path if store_mutation_path(path) => method_not_allowed(request),
        path if session::session_ref_path(path).is_some() => {
            let reference = session::session_ref_path(path).expect("checked path");
            session::inspect_session_response(state, reference)
        }
        path if path.starts_with("/v1/sessions/") => not_found_response(&request.path),
        path if store_ref_path(path, "/v1/models/").is_some() => {
            let reference = store_ref_path(path, "/v1/models/").expect("checked path");
            store::inspect_model_response(state, reference)
        }
        path if path.starts_with("/v1/models/") => not_found_response(&request.path),
        path if store_ref_path(path, "/v1/adapters/").is_some() => {
            let reference = store_ref_path(path, "/v1/adapters/").expect("checked path");
            store::inspect_adapter_response(state, reference)
        }
        path if path.starts_with("/v1/adapters/") => not_found_response(&request.path),
        path if store_ref_path(path, "/v1/datasets/").is_some() => {
            let reference = store_ref_path(path, "/v1/datasets/").expect("checked path");
            store::inspect_dataset_response(state, reference)
        }
        path if path.starts_with("/v1/datasets/") => not_found_response(&request.path),
        path if server_health_path(path).is_some() => match server_health_path(path) {
            Some(reference) => lifecycle::health_server_response(state, reference).await,
            None => not_found_response(&request.path),
        },
        path if path.starts_with("/v1/servers/") => {
            let reference = path.trim_start_matches("/v1/servers/");
            if reference.is_empty() {
                not_found_response(&request.path)
            } else {
                store::inspect_server_response(state, reference)
            }
        }
        _ => not_found_response(&request.path),
    }
}

async fn route_delete(request: &HttpRequest, state: &DaemonHttpState) -> HttpResponse {
    match request.path.as_str() {
        "/v1/auth" | "/v1/doctor" | "/v1/daemon/shutdown" => method_not_allowed(request),
        path if path.starts_with("/v1/auth/") => method_not_allowed(request),
        path if path == "/v1/daemon/logs" || path.starts_with("/v1/daemon/logs/") => {
            method_not_allowed(request)
        }
        path if dataset_tool_path(path) => method_not_allowed(request),
        path if store_mutation_path(path) => method_not_allowed(request),
        path if store_ref_path(path, "/v1/models/").is_some() => {
            let reference = store_ref_path(path, "/v1/models/").expect("checked path");
            store::remove_model_response(state, request, reference)
        }
        path if path.starts_with("/v1/models/") => not_found_response(&request.path),
        path if store_ref_path(path, "/v1/adapters/").is_some() => {
            let reference = store_ref_path(path, "/v1/adapters/").expect("checked path");
            store::remove_adapter_response(state, request, reference)
        }
        path if path.starts_with("/v1/adapters/") => not_found_response(&request.path),
        path if store_ref_path(path, "/v1/datasets/").is_some() => {
            let reference = store_ref_path(path, "/v1/datasets/").expect("checked path");
            store::remove_dataset_response(state, request, reference)
        }
        path if path.starts_with("/v1/datasets/") => not_found_response(&request.path),
        path if store_ref_path(path, "/v1/servers/").is_some() => {
            let reference = store_ref_path(path, "/v1/servers/").expect("checked path");
            store::remove_server_response(state, request, reference)
        }
        path if path.starts_with("/v1/servers/") => not_found_response(&request.path),
        path if session::session_ref_path(path).is_some() => {
            let reference = session::session_ref_path(path).expect("checked path");
            session::remove_session_response(state, request, reference)
        }
        path if session::is_session_route(path) => not_found_response(&request.path),
        path if train_plan_ref_path(path).is_some() => {
            let reference = train_plan_ref_path(path).expect("checked path");
            train::remove_train_plan_response(state, request, reference)
        }
        path if path.starts_with("/v1/train/lora/plans") => method_not_allowed(request),
        _ => not_found_response(&request.path),
    }
}

async fn route_post(request: &HttpRequest, state: &DaemonHttpState) -> HttpResponse {
    match request.path.as_str() {
        "/v1/daemon/shutdown" => daemon_control::shutdown_response(state, request),
        "/v1/auth" | "/v1/doctor" => method_not_allowed(request),
        path if path == "/v1/daemon/logs" || path.starts_with("/v1/daemon/logs/") => {
            method_not_allowed(request)
        }
        "/v1/chat/completions" => openai::chat_completions_response(state, request).await,
        "/v1/chat" => chat::proxy_chat_response(state, request).await,
        "/v1/servers" => lifecycle::create_server_response(state, request),
        "/v1/sessions" => session::create_session_response(state, request),
        "/v1/train/lora/plans/preview" => train::preview_train_plan_response(state, request),
        "/v1/train/lora/plans" => train::create_train_plan_response(state, request),
        "/v1/models/import" => store_mutation::import_model_response(state, request).await,
        "/v1/models/pull" => store_mutation::pull_model_response(state, request).await,
        "/v1/adapters/import" => store_mutation::import_adapter_response(state, request).await,
        "/v1/adapters/pull" => store_mutation::pull_adapter_response(state, request).await,
        "/v1/datasets/import" => store_mutation::import_dataset_response(state, request).await,
        "/v1/datasets/validate" => dataset_tools::validate_dataset_response(state, request).await,
        "/v1/datasets/template" => dataset_tools::dataset_template_response(request),
        "/v1/datasets/synth" => dataset_cloud::synth_dataset_response(state, request).await,
        "/v1/datasets/eval" => dataset_cloud::eval_dataset_response(state, request).await,
        path if adapter_bind_path(path).is_some() => {
            let reference = adapter_bind_path(path).expect("checked path");
            store_mutation::bind_adapter_response(state, request, reference).await
        }
        path if dataset_export_path(path).is_some() => {
            let reference = dataset_export_path(path).expect("checked path");
            dataset_tools::export_dataset_response(state, request, reference).await
        }
        path if dataset_diff_path(path).is_some() => {
            let reference = dataset_diff_path(path).expect("checked path");
            dataset_tools::diff_dataset_response(state, request, reference).await
        }
        path if path.starts_with("/v1/models/")
            || path.starts_with("/v1/adapters/")
            || path.starts_with("/v1/datasets/") =>
        {
            method_not_allowed(request)
        }
        path if train_plan_runs_path(path).is_some() => {
            let reference = train_plan_runs_path(path).expect("checked path");
            train::start_train_run_response(state, request, reference).await
        }
        path if path.starts_with("/v1/train/lora/plans") => method_not_allowed(request),
        path if path.starts_with("/v1/train/lora/runs") => method_not_allowed(request),
        path if path.starts_with("/v1/auth/") => method_not_allowed(request),
        path if path.starts_with("/v1/servers/") => match server_action_path(path) {
            Some((reference, ServerAction::Start)) => {
                lifecycle::start_server_response(state, reference, request).await
            }
            Some((reference, ServerAction::Stop)) => {
                lifecycle::stop_server_response(state, reference)
            }
            None => not_found_response(&request.path),
        },
        path if session::session_messages_path(path).is_some() => {
            let reference = session::session_messages_path(path).expect("checked path");
            session::append_session_messages_response(state, request, reference)
        }
        path if session::is_session_route(path) => method_not_allowed(request),
        _ => not_found_response(&request.path),
    }
}

async fn route_patch(request: &HttpRequest, state: &DaemonHttpState) -> HttpResponse {
    match request.path.as_str() {
        path if session::session_ref_path(path).is_some() => {
            let reference = session::session_ref_path(path).expect("checked path");
            session::update_session_response(state, request, reference)
        }
        path if session::is_session_route(path) => method_not_allowed(request),
        path if path.starts_with("/v1/") => method_not_allowed(request),
        _ => not_found_response(&request.path),
    }
}

fn store_ref_path<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    let reference = path.strip_prefix(prefix)?;
    if reference.is_empty() || reference.contains('/') {
        None
    } else {
        Some(reference)
    }
}

fn auth_provider_path(path: &str) -> Option<&str> {
    let provider = path.strip_prefix("/v1/auth/")?;
    if provider.is_empty() || provider.contains('/') {
        None
    } else {
        Some(provider)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServerAction {
    Start,
    Stop,
}

fn server_action_path(path: &str) -> Option<(&str, ServerAction)> {
    let rest = path.strip_prefix("/v1/servers/")?;
    let (reference, action) = rest.rsplit_once('/')?;
    if reference.is_empty() {
        return None;
    }

    match action {
        "start" => Some((reference, ServerAction::Start)),
        "stop" => Some((reference, ServerAction::Stop)),
        _ => None,
    }
}

fn server_health_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/v1/servers/")?;
    let reference = rest.strip_suffix("/health")?;
    if reference.is_empty() || reference.contains('/') {
        return None;
    }
    Some(reference)
}

fn adapter_bind_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/v1/adapters/")?;
    let reference = rest.strip_suffix("/bind")?;
    if reference.is_empty() || reference.contains('/') {
        return None;
    }
    Some(reference)
}

fn dataset_export_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/v1/datasets/")?;
    let reference = rest.strip_suffix("/export")?;
    if reference.is_empty() || reference.contains('/') {
        return None;
    }
    Some(reference)
}

fn dataset_diff_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/v1/datasets/")?;
    let reference = rest.strip_suffix("/diff")?;
    if reference.is_empty() || reference.contains('/') {
        return None;
    }
    Some(reference)
}

fn train_plan_ref_path(path: &str) -> Option<&str> {
    let reference = path.strip_prefix("/v1/train/lora/plans/")?;
    if reference.is_empty() || reference == "preview" || reference.contains('/') {
        None
    } else {
        Some(reference)
    }
}

fn train_plan_runs_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/v1/train/lora/plans/")?;
    let reference = rest.strip_suffix("/runs")?;
    if reference.is_empty() || reference.contains('/') || reference == "preview" {
        None
    } else {
        Some(reference)
    }
}

fn train_run_ref_path(path: &str) -> Option<&str> {
    let reference = path.strip_prefix("/v1/train/lora/runs/")?;
    if reference.is_empty() || reference.contains('/') {
        None
    } else {
        Some(reference)
    }
}

fn train_run_metrics_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/v1/train/lora/runs/")?;
    let reference = rest.strip_suffix("/metrics")?;
    if reference.is_empty() || reference.contains('/') {
        None
    } else {
        Some(reference)
    }
}

fn train_run_logs_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/v1/train/lora/runs/")?;
    let reference = rest.strip_suffix("/logs")?;
    if reference.is_empty() || reference.contains('/') {
        None
    } else {
        Some(reference)
    }
}

fn train_run_raw_log_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/v1/train/lora/runs/")?;
    let reference = rest.strip_suffix("/logs/raw")?;
    if reference.is_empty() || reference.contains('/') {
        None
    } else {
        Some(reference)
    }
}

fn train_plan_static_path(path: &str) -> bool {
    matches!(path, "/v1/train/lora/plans/preview")
}

fn dataset_tool_path(path: &str) -> bool {
    matches!(
        path,
        "/v1/datasets/validate"
            | "/v1/datasets/template"
            | "/v1/datasets/synth"
            | "/v1/datasets/eval"
    ) || dataset_export_path(path).is_some()
        || dataset_diff_path(path).is_some()
}

fn store_mutation_path(path: &str) -> bool {
    matches!(
        path,
        "/v1/models/import"
            | "/v1/models/pull"
            | "/v1/adapters/import"
            | "/v1/adapters/pull"
            | "/v1/datasets/import"
    ) || adapter_bind_path(path).is_some()
}

#[cfg(test)]
mod tests;
