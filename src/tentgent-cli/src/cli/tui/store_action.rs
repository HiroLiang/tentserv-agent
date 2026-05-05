use std::{
    path::Path,
    time::{Duration, Instant},
};

use reqwest::{header, Method, StatusCode};
use serde_json::{Map, Value};

use super::{
    daemon_client::TuiTokenSource,
    jobs::{parse_job_response, TuiJobItem},
    navigator::{percent_encode_path_segment, NavigatorListKind, NavigatorRow},
};

const ACTION_CONNECT_TIMEOUT: Duration = Duration::from_millis(700);

#[cfg(test)]
pub(super) const ACTION_ALLOWED_ROUTES: [&str; 22] = [
    "POST /v1/models/pull",
    "POST /v1/models/pull/jobs",
    "POST /v1/models/import",
    "POST /v1/models/import/jobs",
    "DELETE /v1/models/{ref}",
    "POST /v1/adapters/pull",
    "POST /v1/adapters/pull/jobs",
    "POST /v1/adapters/import",
    "POST /v1/adapters/import/jobs",
    "POST /v1/adapters/{ref}/bind",
    "DELETE /v1/adapters/{ref}",
    "POST /v1/datasets/import",
    "POST /v1/datasets/import/jobs",
    "POST /v1/datasets/validate",
    "POST /v1/datasets/template",
    "POST /v1/datasets/{ref}/export",
    "POST /v1/datasets/{ref}/diff",
    "POST /v1/datasets/synth",
    "POST /v1/datasets/synth/jobs",
    "POST /v1/datasets/eval",
    "POST /v1/datasets/eval/jobs",
    "DELETE /v1/datasets/{ref}",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StoreActionKind {
    ModelPull,
    ModelImport,
    ModelRemove,
    AdapterPull,
    AdapterImport,
    AdapterBind,
    AdapterRemove,
    DatasetImport,
    DatasetValidateSelected,
    DatasetValidatePath,
    DatasetTemplate,
    DatasetExport,
    DatasetDiffRef,
    DatasetDiffPath,
    DatasetSynthPromptBrief,
    DatasetSynthBrief,
    DatasetSynthSpecPath,
    DatasetEvalSelected,
    DatasetEvalPath,
    DatasetEvalContent,
    DatasetRemove,
}

impl StoreActionKind {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::ModelPull => "Pull model",
            Self::ModelImport => "Import model path",
            Self::ModelRemove => "Remove model",
            Self::AdapterPull => "Pull adapter",
            Self::AdapterImport => "Import adapter path",
            Self::AdapterBind => "Bind adapter to model",
            Self::AdapterRemove => "Remove adapter",
            Self::DatasetImport => "Import dataset path",
            Self::DatasetValidateSelected => "Validate selected dataset",
            Self::DatasetValidatePath => "Validate dataset path",
            Self::DatasetTemplate => "Generate dataset template",
            Self::DatasetExport => "Export selected dataset",
            Self::DatasetDiffRef => "Diff selected vs dataset ref",
            Self::DatasetDiffPath => "Diff selected vs path",
            Self::DatasetSynthPromptBrief => "Preview synth prompt",
            Self::DatasetSynthBrief => "Synth dataset from brief",
            Self::DatasetSynthSpecPath => "Synth dataset from spec path",
            Self::DatasetEvalSelected => "Eval selected dataset",
            Self::DatasetEvalPath => "Eval dataset path",
            Self::DatasetEvalContent => "Eval pasted jsonl",
            Self::DatasetRemove => "Remove dataset",
        }
    }

    pub(super) fn section(self) -> NavigatorListKind {
        match self {
            Self::ModelPull | Self::ModelImport | Self::ModelRemove => NavigatorListKind::Models,
            Self::AdapterPull | Self::AdapterImport | Self::AdapterBind | Self::AdapterRemove => {
                NavigatorListKind::Adapters
            }
            Self::DatasetImport
            | Self::DatasetValidateSelected
            | Self::DatasetValidatePath
            | Self::DatasetTemplate
            | Self::DatasetExport
            | Self::DatasetDiffRef
            | Self::DatasetDiffPath
            | Self::DatasetSynthPromptBrief
            | Self::DatasetSynthBrief
            | Self::DatasetSynthSpecPath
            | Self::DatasetEvalSelected
            | Self::DatasetEvalPath
            | Self::DatasetEvalContent
            | Self::DatasetRemove => NavigatorListKind::Datasets,
        }
    }

    pub(super) fn detail(self) -> &'static str {
        match self {
            Self::ModelPull => "download/import a Hugging Face model repo",
            Self::ModelImport => "add an absolute local model path",
            Self::ModelRemove => "delete one managed model entry",
            Self::AdapterPull => "download/import a Hugging Face adapter repo",
            Self::AdapterImport => "add an absolute local adapter path",
            Self::AdapterBind => "mutate adapter metadata base_model_ref",
            Self::AdapterRemove => "delete one managed adapter entry",
            Self::DatasetImport => "add an absolute local dataset path",
            Self::DatasetValidateSelected => "validate selected managed dataset",
            Self::DatasetValidatePath => "validate an absolute dataset path",
            Self::DatasetTemplate => "render deterministic dataset prompt text",
            Self::DatasetExport => "write selected dataset to an output path",
            Self::DatasetDiffRef => "compare selected dataset with another ref",
            Self::DatasetDiffPath => "compare selected dataset with path",
            Self::DatasetSynthPromptBrief => "preview provider prompt without network",
            Self::DatasetSynthBrief => "network/provider-credit dataset generation",
            Self::DatasetSynthSpecPath => "network/provider-credit generation from spec file",
            Self::DatasetEvalSelected => "network/provider-credit dataset evaluation",
            Self::DatasetEvalPath => "network/provider-credit eval for path",
            Self::DatasetEvalContent => "network/provider-credit eval for pasted JSONL",
            Self::DatasetRemove => "delete one managed dataset entry",
        }
    }

    pub(super) fn destructive(self) -> bool {
        matches!(
            self,
            Self::ModelRemove | Self::AdapterRemove | Self::DatasetRemove
        )
    }

    pub(super) fn long_running(self) -> bool {
        matches!(
            self,
            Self::ModelPull
                | Self::ModelImport
                | Self::AdapterPull
                | Self::AdapterImport
                | Self::DatasetImport
                | Self::DatasetSynthBrief
                | Self::DatasetSynthSpecPath
                | Self::DatasetEvalSelected
                | Self::DatasetEvalPath
                | Self::DatasetEvalContent
        )
    }

    pub(super) fn requires_provider_confirmation(self) -> bool {
        matches!(
            self,
            Self::DatasetSynthBrief
                | Self::DatasetSynthSpecPath
                | Self::DatasetEvalSelected
                | Self::DatasetEvalPath
                | Self::DatasetEvalContent
        )
    }

    pub(super) fn requires_selection(self) -> bool {
        matches!(
            self,
            Self::ModelRemove
                | Self::AdapterBind
                | Self::AdapterRemove
                | Self::DatasetValidateSelected
                | Self::DatasetExport
                | Self::DatasetDiffRef
                | Self::DatasetDiffPath
                | Self::DatasetEvalSelected
                | Self::DatasetRemove
        )
    }

    pub(super) fn refresh_targets(self) -> &'static [NavigatorListKind] {
        match self {
            Self::ModelPull | Self::ModelImport | Self::ModelRemove => &[NavigatorListKind::Models],
            Self::AdapterPull | Self::AdapterImport | Self::AdapterBind | Self::AdapterRemove => {
                &[NavigatorListKind::Adapters]
            }
            Self::DatasetImport
            | Self::DatasetSynthBrief
            | Self::DatasetSynthSpecPath
            | Self::DatasetRemove => &[NavigatorListKind::Datasets],
            _ => &[],
        }
    }

    pub(super) fn fields(self) -> Vec<ActionFieldSpec> {
        match self {
            Self::ModelPull => vec![
                ActionFieldSpec::required("repo_id", FieldKind::Text),
                ActionFieldSpec::optional("revision", FieldKind::Text),
            ],
            Self::ModelImport | Self::DatasetImport | Self::DatasetValidatePath => {
                vec![ActionFieldSpec::required("path", FieldKind::AbsolutePath)]
            }
            Self::AdapterPull => vec![
                ActionFieldSpec::required("repo_id", FieldKind::Text),
                ActionFieldSpec::optional("revision", FieldKind::Text),
                ActionFieldSpec::optional("base_model_ref", FieldKind::Ref),
            ],
            Self::AdapterImport => vec![
                ActionFieldSpec::required("path", FieldKind::AbsolutePath),
                ActionFieldSpec::optional("base_model_ref", FieldKind::Ref),
            ],
            Self::AdapterBind => vec![ActionFieldSpec::required("base_model_ref", FieldKind::Ref)],
            Self::DatasetTemplate => vec![
                ActionFieldSpec::optional("task", FieldKind::Text),
                ActionFieldSpec::optional("language", FieldKind::Text),
            ],
            Self::DatasetExport => {
                vec![ActionFieldSpec::required(
                    "output_path",
                    FieldKind::AbsolutePath,
                )]
            }
            Self::DatasetDiffRef => {
                vec![ActionFieldSpec::required(
                    "right_dataset_ref",
                    FieldKind::Ref,
                )]
            }
            Self::DatasetDiffPath => {
                vec![ActionFieldSpec::required(
                    "right_path",
                    FieldKind::AbsolutePath,
                )]
            }
            Self::DatasetSynthPromptBrief => vec![
                ActionFieldSpec::required("brief", FieldKind::Text),
                ActionFieldSpec::required("split", FieldKind::Text),
                ActionFieldSpec::required("count", FieldKind::PositiveInteger),
            ],
            Self::DatasetSynthBrief => vec![
                ActionFieldSpec::required("brief", FieldKind::Text),
                ActionFieldSpec::required("provider", FieldKind::Text),
                ActionFieldSpec::required("model", FieldKind::Text),
                ActionFieldSpec::required("output_path", FieldKind::AbsolutePath),
                ActionFieldSpec::required("split", FieldKind::Text),
                ActionFieldSpec::required("count", FieldKind::PositiveInteger),
                ActionFieldSpec::optional("max_tokens", FieldKind::PositiveInteger),
                ActionFieldSpec::optional("temperature", FieldKind::Number),
                ActionFieldSpec::optional("timeout_seconds", FieldKind::Number),
                ActionFieldSpec::optional("retries", FieldKind::PositiveInteger),
            ],
            Self::DatasetSynthSpecPath => vec![
                ActionFieldSpec::required("spec_path", FieldKind::AbsolutePath),
                ActionFieldSpec::required("provider", FieldKind::Text),
                ActionFieldSpec::required("model", FieldKind::Text),
                ActionFieldSpec::required("output_path", FieldKind::AbsolutePath),
                ActionFieldSpec::required("split", FieldKind::Text),
                ActionFieldSpec::required("count", FieldKind::PositiveInteger),
                ActionFieldSpec::optional("max_tokens", FieldKind::PositiveInteger),
                ActionFieldSpec::optional("temperature", FieldKind::Number),
                ActionFieldSpec::optional("timeout_seconds", FieldKind::Number),
                ActionFieldSpec::optional("retries", FieldKind::PositiveInteger),
            ],
            Self::DatasetEvalSelected => vec![
                ActionFieldSpec::required("provider", FieldKind::Text),
                ActionFieldSpec::required("model", FieldKind::Text),
                ActionFieldSpec::required("output_path", FieldKind::AbsolutePath),
                ActionFieldSpec::optional("split", FieldKind::Text),
                ActionFieldSpec::optional("max_records", FieldKind::PositiveInteger),
                ActionFieldSpec::optional("criteria", FieldKind::Text),
                ActionFieldSpec::optional("max_tokens", FieldKind::PositiveInteger),
                ActionFieldSpec::optional("temperature", FieldKind::Number),
                ActionFieldSpec::optional("timeout_seconds", FieldKind::Number),
            ],
            Self::DatasetEvalPath => vec![
                ActionFieldSpec::required("input_path", FieldKind::AbsolutePath),
                ActionFieldSpec::required("provider", FieldKind::Text),
                ActionFieldSpec::required("model", FieldKind::Text),
                ActionFieldSpec::required("output_path", FieldKind::AbsolutePath),
                ActionFieldSpec::optional("split", FieldKind::Text),
                ActionFieldSpec::optional("max_records", FieldKind::PositiveInteger),
                ActionFieldSpec::optional("criteria", FieldKind::Text),
                ActionFieldSpec::optional("max_tokens", FieldKind::PositiveInteger),
                ActionFieldSpec::optional("temperature", FieldKind::Number),
                ActionFieldSpec::optional("timeout_seconds", FieldKind::Number),
            ],
            Self::DatasetEvalContent => vec![
                ActionFieldSpec::required("input_content", FieldKind::Text),
                ActionFieldSpec::optional("input_format", FieldKind::Text),
                ActionFieldSpec::required("provider", FieldKind::Text),
                ActionFieldSpec::required("model", FieldKind::Text),
                ActionFieldSpec::required("output_path", FieldKind::AbsolutePath),
                ActionFieldSpec::optional("split", FieldKind::Text),
                ActionFieldSpec::optional("max_records", FieldKind::PositiveInteger),
                ActionFieldSpec::optional("criteria", FieldKind::Text),
                ActionFieldSpec::optional("max_tokens", FieldKind::PositiveInteger),
                ActionFieldSpec::optional("temperature", FieldKind::Number),
                ActionFieldSpec::optional("timeout_seconds", FieldKind::Number),
            ],
            Self::ModelRemove
            | Self::AdapterRemove
            | Self::DatasetValidateSelected
            | Self::DatasetRemove => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FieldKind {
    Text,
    Ref,
    AbsolutePath,
    PositiveInteger,
    Number,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ActionFieldSpec {
    pub(super) name: &'static str,
    pub(super) kind: FieldKind,
    pub(super) required: bool,
}

impl ActionFieldSpec {
    const fn required(name: &'static str, kind: FieldKind) -> Self {
        Self {
            name,
            kind,
            required: true,
        }
    }

    const fn optional(name: &'static str, kind: FieldKind) -> Self {
        Self {
            name,
            kind,
            required: false,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct StoreActionForm {
    pub(super) action: StoreActionKind,
    pub(super) fields: Vec<ActionFieldValue>,
    pub(super) selected_field: usize,
}

impl StoreActionForm {
    pub(super) fn new(action: StoreActionKind) -> Self {
        let fields = action
            .fields()
            .iter()
            .map(|spec| ActionFieldValue {
                spec: *spec,
                value: default_field_value(spec.name),
                cursor: default_field_value(spec.name).chars().count(),
            })
            .collect();
        Self {
            action,
            fields,
            selected_field: 0,
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub(super) fn selected_field_mut(&mut self) -> Option<&mut ActionFieldValue> {
        self.fields.get_mut(self.selected_field)
    }

    pub(super) fn move_field(&mut self, delta: isize) {
        self.selected_field = move_index(self.selected_field, self.fields.len(), delta);
    }

    pub(super) fn values(&self) -> Vec<(&'static str, String)> {
        self.fields
            .iter()
            .map(|field| (field.spec.name, field.value.trim().to_string()))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub(super) struct ActionFieldValue {
    pub(super) spec: ActionFieldSpec,
    pub(super) value: String,
    pub(super) cursor: usize,
}

#[derive(Debug, Clone)]
pub(super) enum ActionState {
    Idle,
    SelectingAction {
        kind: NavigatorListKind,
        actions: Vec<StoreActionKind>,
        selected: usize,
    },
    EditingForm {
        kind: NavigatorListKind,
        selected: Option<NavigatorRow>,
        form: StoreActionForm,
        error: Option<String>,
    },
    Confirming {
        request: StoreActionRequest,
        typed: String,
        cursor: usize,
        message: String,
    },
    Running {
        request_id: u64,
        generation: u64,
        request: StoreActionRequest,
        started_at: Instant,
    },
    Result(StoreActionResult),
    Error {
        action: StoreActionKind,
        message: String,
        recoverable: bool,
    },
}

impl Default for ActionState {
    fn default() -> Self {
        Self::Idle
    }
}

impl ActionState {
    pub(super) fn is_active(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    pub(super) fn in_flight(&self) -> Option<u64> {
        match self {
            Self::Running { request_id, .. } => Some(*request_id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct StoreActionRequest {
    pub(super) action: StoreActionKind,
    pub(super) method: Method,
    pub(super) path: String,
    pub(super) body: Option<Value>,
    pub(super) selected_ref: Option<String>,
    pub(super) selected_short_ref: Option<String>,
    pub(super) form_values: Vec<(&'static str, String)>,
    pub(super) confirmation_token: Option<String>,
    pub(super) provider_confirmation: bool,
    pub(super) refresh_targets: Vec<NavigatorListKind>,
    pub(super) long_running: bool,
    pub(super) cli_hint: Option<String>,
}

impl StoreActionRequest {
    pub(super) fn confirmation_matches(&self, typed: &str) -> bool {
        if self.provider_confirmation {
            return typed.trim() == "RUN";
        }
        let typed = typed.trim();
        self.confirmation_token
            .as_deref()
            .is_some_and(|token| typed == token)
            || self
                .selected_ref
                .as_deref()
                .is_some_and(|full_ref| typed == full_ref)
    }

    pub(super) fn job_path(&self) -> Option<String> {
        let path = match self.action {
            StoreActionKind::ModelPull => "/v1/models/pull/jobs",
            StoreActionKind::ModelImport => "/v1/models/import/jobs",
            StoreActionKind::AdapterPull => "/v1/adapters/pull/jobs",
            StoreActionKind::AdapterImport => "/v1/adapters/import/jobs",
            StoreActionKind::DatasetImport => "/v1/datasets/import/jobs",
            StoreActionKind::DatasetSynthBrief | StoreActionKind::DatasetSynthSpecPath => {
                "/v1/datasets/synth/jobs"
            }
            StoreActionKind::DatasetEvalSelected
            | StoreActionKind::DatasetEvalPath
            | StoreActionKind::DatasetEvalContent => "/v1/datasets/eval/jobs",
            _ => return None,
        };
        Some(path.to_string())
    }
}

#[derive(Debug, Clone)]
pub(super) struct StoreActionResult {
    pub(super) action: StoreActionKind,
    pub(super) status: u16,
    pub(super) lines: Vec<(String, String)>,
    pub(super) raw_summary: String,
    pub(super) refresh_targets: Vec<NavigatorListKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum StoreActionError {
    AuthRequired(String),
    BadRequest(String),
    NotFound(String),
    Conflict(String),
    Timeout(String),
    Down(String),
    Protocol(String),
    Http { status: u16, message: String },
}

impl std::fmt::Display for StoreActionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AuthRequired(message)
            | Self::BadRequest(message)
            | Self::NotFound(message)
            | Self::Conflict(message)
            | Self::Timeout(message)
            | Self::Down(message)
            | Self::Protocol(message) => write!(formatter, "{message}"),
            Self::Http { status, message } => write!(formatter, "HTTP {status}: {message}"),
        }
    }
}

impl StoreActionError {
    fn from_status(status: StatusCode, path: &str, text: &str) -> Self {
        let message = error_message(text).unwrap_or_else(|| format!("{path} returned {status}"));
        match status {
            StatusCode::UNAUTHORIZED => Self::AuthRequired(message),
            StatusCode::BAD_REQUEST => Self::BadRequest(message),
            StatusCode::NOT_FOUND => Self::NotFound(message),
            StatusCode::CONFLICT => Self::Conflict(message),
            status if status.is_server_error() => Self::Http {
                status: status.as_u16(),
                message,
            },
            status => Self::Http {
                status: status.as_u16(),
                message,
            },
        }
    }
}

pub(super) struct StoreActionClient {
    base_url: String,
    token: Option<String>,
    client: reqwest::Client,
}

impl StoreActionClient {
    pub(super) fn new(
        base_url: String,
        token: Option<String>,
        _token_source: TuiTokenSource,
    ) -> miette::Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(ACTION_CONNECT_TIMEOUT)
            .build()
            .map_err(|error| miette::miette!("failed to build action client: {error}"))?;
        Ok(Self {
            base_url,
            token,
            client,
        })
    }

    pub(super) async fn execute(
        &self,
        request: StoreActionRequest,
    ) -> Result<StoreActionResult, StoreActionError> {
        let url = self.endpoint(&request.path);
        let mut builder = self.client.request(request.method.clone(), url);
        if let Some(token) = self.token.as_deref() {
            builder = builder.bearer_auth(token);
        }
        if let Some(body) = &request.body {
            builder = builder
                .header(header::CONTENT_TYPE, "application/json")
                .json(body);
        }
        let response = builder.send().await.map_err(|error| {
            if error.is_timeout() {
                StoreActionError::Timeout(format!("{} timed out: {error}", request.path))
            } else {
                StoreActionError::Down(format!("{} failed: {error}", request.path))
            }
        })?;
        let status = response.status();
        let text = response.text().await.map_err(|error| {
            StoreActionError::Protocol(format!("failed to read response: {error}"))
        })?;
        if !status.is_success() {
            return Err(StoreActionError::from_status(status, &request.path, &text));
        }
        let value: Value = serde_json::from_str(&text).map_err(|error| {
            StoreActionError::Protocol(format!("invalid {} JSON: {error}", request.path))
        })?;
        Ok(StoreActionResult {
            action: request.action,
            status: status.as_u16(),
            lines: summarize_action_value(request.action, &value),
            raw_summary: bounded_value_summary(&value),
            refresh_targets: request.refresh_targets,
        })
    }

    pub(super) async fn start_job(
        &self,
        request: StoreActionRequest,
    ) -> Result<TuiJobItem, StoreActionError> {
        let Some(path) = request.job_path() else {
            return Err(StoreActionError::Protocol(format!(
                "{} does not have an async job route",
                request.action.label()
            )));
        };
        let url = self.endpoint(&path);
        let mut builder = self.client.post(url);
        if let Some(token) = self.token.as_deref() {
            builder = builder.bearer_auth(token);
        }
        if let Some(body) = &request.body {
            builder = builder
                .header(header::CONTENT_TYPE, "application/json")
                .json(body);
        }
        let response = builder.send().await.map_err(|error| {
            if error.is_timeout() {
                StoreActionError::Timeout(format!("{path} timed out: {error}"))
            } else {
                StoreActionError::Down(format!("{path} failed: {error}"))
            }
        })?;
        let status = response.status();
        let text = response.text().await.map_err(|error| {
            StoreActionError::Protocol(format!("failed to read job response: {error}"))
        })?;
        if !status.is_success() {
            return Err(StoreActionError::from_status(status, &path, &text));
        }
        parse_job_response(&text).map_err(|error| match error {
            super::jobs::JobError::Protocol(message) => StoreActionError::Protocol(message),
            super::jobs::JobError::AuthRequired(message) => StoreActionError::AuthRequired(message),
            super::jobs::JobError::Down(message) => StoreActionError::Down(message),
            super::jobs::JobError::Timeout(message) => StoreActionError::Timeout(message),
            super::jobs::JobError::Http { status, message } => {
                StoreActionError::Http { status, message }
            }
        })
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.base_url.trim_end_matches('/'), path)
    }
}

pub(super) fn actions_for(kind: NavigatorListKind) -> Vec<StoreActionKind> {
    match kind {
        NavigatorListKind::Models => vec![
            StoreActionKind::ModelPull,
            StoreActionKind::ModelImport,
            StoreActionKind::ModelRemove,
        ],
        NavigatorListKind::Adapters => vec![
            StoreActionKind::AdapterPull,
            StoreActionKind::AdapterImport,
            StoreActionKind::AdapterBind,
            StoreActionKind::AdapterRemove,
        ],
        NavigatorListKind::Datasets => vec![
            StoreActionKind::DatasetImport,
            StoreActionKind::DatasetValidateSelected,
            StoreActionKind::DatasetValidatePath,
            StoreActionKind::DatasetTemplate,
            StoreActionKind::DatasetExport,
            StoreActionKind::DatasetDiffRef,
            StoreActionKind::DatasetDiffPath,
            StoreActionKind::DatasetSynthPromptBrief,
            StoreActionKind::DatasetSynthBrief,
            StoreActionKind::DatasetSynthSpecPath,
            StoreActionKind::DatasetEvalSelected,
            StoreActionKind::DatasetEvalPath,
            StoreActionKind::DatasetEvalContent,
            StoreActionKind::DatasetRemove,
        ],
        _ => Vec::new(),
    }
}

pub(super) fn build_action_request(
    action: StoreActionKind,
    selected: Option<&NavigatorRow>,
    values: &[(&'static str, String)],
) -> Result<StoreActionRequest, String> {
    let selected_ref = selected.map(|row| row.item_ref.clone());
    let selected_short_ref = selected.map(|row| row.short_ref.clone());
    if action.requires_selection() && selected_ref.is_none() {
        return Err(format!("{} requires a selected row", action.label()));
    }
    validate_values(action, values)?;
    let mut body = Map::new();
    for (key, value) in values {
        if value.trim().is_empty() {
            continue;
        }
        insert_typed_value(&mut body, key, value)?;
    }
    let selected_ref_str = selected_ref.as_deref();
    let path = action_path(action, selected_ref_str)?;
    match action {
        StoreActionKind::DatasetValidateSelected => {
            body.insert(
                "dataset_ref".to_string(),
                Value::String(selected_ref_str.expect("checked").to_string()),
            );
        }
        StoreActionKind::DatasetSynthPromptBrief => {
            body.insert("print_prompt".to_string(), Value::Bool(true));
        }
        StoreActionKind::DatasetEvalSelected => {
            body.insert(
                "dataset_ref".to_string(),
                Value::String(selected_ref_str.expect("checked").to_string()),
            );
        }
        _ => {}
    }
    let method = if action.destructive() {
        Method::DELETE
    } else {
        Method::POST
    };
    let body = if method == Method::DELETE {
        None
    } else {
        Some(Value::Object(body))
    };
    let confirmation_token = action.destructive().then(|| {
        selected_short_ref
            .clone()
            .or_else(|| selected_ref.clone())
            .expect("destructive action requires selection")
    });
    Ok(StoreActionRequest {
        action,
        method,
        path,
        body,
        selected_ref,
        selected_short_ref,
        form_values: values.to_vec(),
        confirmation_token,
        provider_confirmation: action.requires_provider_confirmation(),
        refresh_targets: action.refresh_targets().to_vec(),
        long_running: action.long_running(),
        cli_hint: cli_hint(action, values, selected),
    })
}

fn action_path(action: StoreActionKind, selected_ref: Option<&str>) -> Result<String, String> {
    let selected = selected_ref.map(percent_encode_path_segment);
    Ok(match action {
        StoreActionKind::ModelPull => "/v1/models/pull".to_string(),
        StoreActionKind::ModelImport => "/v1/models/import".to_string(),
        StoreActionKind::ModelRemove => {
            format!("/v1/models/{}", selected.ok_or("missing model ref")?)
        }
        StoreActionKind::AdapterPull => "/v1/adapters/pull".to_string(),
        StoreActionKind::AdapterImport => "/v1/adapters/import".to_string(),
        StoreActionKind::AdapterBind => {
            format!(
                "/v1/adapters/{}/bind",
                selected.ok_or("missing adapter ref")?
            )
        }
        StoreActionKind::AdapterRemove => {
            format!("/v1/adapters/{}", selected.ok_or("missing adapter ref")?)
        }
        StoreActionKind::DatasetImport => "/v1/datasets/import".to_string(),
        StoreActionKind::DatasetValidateSelected | StoreActionKind::DatasetValidatePath => {
            "/v1/datasets/validate".to_string()
        }
        StoreActionKind::DatasetTemplate => "/v1/datasets/template".to_string(),
        StoreActionKind::DatasetExport => {
            format!(
                "/v1/datasets/{}/export",
                selected.ok_or("missing dataset ref")?
            )
        }
        StoreActionKind::DatasetDiffRef | StoreActionKind::DatasetDiffPath => {
            format!(
                "/v1/datasets/{}/diff",
                selected.ok_or("missing dataset ref")?
            )
        }
        StoreActionKind::DatasetSynthPromptBrief
        | StoreActionKind::DatasetSynthBrief
        | StoreActionKind::DatasetSynthSpecPath => "/v1/datasets/synth".to_string(),
        StoreActionKind::DatasetEvalSelected
        | StoreActionKind::DatasetEvalPath
        | StoreActionKind::DatasetEvalContent => "/v1/datasets/eval".to_string(),
        StoreActionKind::DatasetRemove => {
            format!("/v1/datasets/{}", selected.ok_or("missing dataset ref")?)
        }
    })
}

fn validate_values(
    action: StoreActionKind,
    values: &[(&'static str, String)],
) -> Result<(), String> {
    for spec in action.fields() {
        let value = field_value(values, spec.name);
        if spec.required && value.trim().is_empty() {
            return Err(format!("{} is required", spec.name));
        }
        if value.trim().is_empty() {
            continue;
        }
        match spec.kind {
            FieldKind::AbsolutePath => {
                if !Path::new(value.trim()).is_absolute() {
                    return Err(format!("{} must be an absolute path", spec.name));
                }
            }
            FieldKind::PositiveInteger => {
                let parsed = value
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| format!("{} must be a positive integer", spec.name))?;
                if parsed == 0 {
                    return Err(format!("{} must be greater than zero", spec.name));
                }
            }
            FieldKind::Number => {
                value
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| format!("{} must be a number", spec.name))?;
            }
            FieldKind::Ref => {
                if value.trim().contains('/') {
                    return Err(format!("{} must be a managed ref, not a path", spec.name));
                }
            }
            FieldKind::Text => {}
        }
    }
    Ok(())
}

fn insert_typed_value(
    body: &mut Map<String, Value>,
    key: &'static str,
    value: &str,
) -> Result<(), String> {
    let trimmed = value.trim();
    let typed = match key {
        "count"
        | "train_count"
        | "valid_count"
        | "test_count"
        | "eval_count"
        | "max_tokens"
        | "timeout_seconds_int"
        | "retries"
        | "max_records" => Value::Number(
            trimmed
                .parse::<u64>()
                .map_err(|_| format!("{key} must be an integer"))?
                .into(),
        ),
        "temperature" | "timeout_seconds" => {
            let number = trimmed
                .parse::<f64>()
                .map_err(|_| format!("{key} must be a number"))?;
            let number = serde_json::Number::from_f64(number)
                .ok_or_else(|| format!("{key} must be a finite number"))?;
            Value::Number(number)
        }
        _ => Value::String(trimmed.to_string()),
    };
    body.insert(key.to_string(), typed);
    Ok(())
}

fn summarize_action_value(action: StoreActionKind, value: &Value) -> Vec<(String, String)> {
    let mut lines = vec![("action".to_string(), action.label().to_string())];
    for path in [
        &["mutation", "kind"][..],
        &["model", "short_ref"],
        &["adapter", "short_ref"],
        &["dataset", "short_ref"],
        &["removed", "short_ref"],
        &["mutation", "store_path"],
        &["mutation", "source_index_path"],
        &["mutation", "base_model_ref"],
        &["export", "output_path"],
        &["export", "file_count"],
        &["diff", "summary", "added"],
        &["diff", "summary", "removed"],
        &["diff", "truncated"],
        &["valid"],
        &["records"],
        &["errors_count"],
        &["template_version"],
        &["prompt", "source_kind"],
        &["synth", "output_dir"],
        &["synth", "debug_dir"],
        &["eval", "output_dir"],
        &["eval", "debug_dir"],
    ] {
        if let Some(found) = value_at_path(value, path) {
            lines.push((path.join("."), display_json_scalar(found)));
        }
    }
    if let Some(content) = value_at_path(value, &["content"]) {
        lines.push((
            "content_preview".to_string(),
            truncate(&display_json_scalar(content), 600),
        ));
    }
    if let Some(content) = value_at_path(value, &["prompt", "content"]) {
        lines.push((
            "prompt_preview".to_string(),
            truncate(&display_json_scalar(content), 600),
        ));
    }
    if let Some(progress) = value.get("progress_events").and_then(Value::as_array) {
        lines.push(("progress_events".to_string(), progress.len().to_string()));
    }
    if lines.len() == 1 {
        lines.push(("response".to_string(), bounded_value_summary(value)));
    }
    lines
}

fn bounded_value_summary(value: &Value) -> String {
    truncate(&value.to_string(), 1200)
}

fn cli_hint(
    action: StoreActionKind,
    values: &[(&'static str, String)],
    selected: Option<&NavigatorRow>,
) -> Option<String> {
    let selected = selected.map(|row| row.short_ref.as_str());
    let value = |name| field_value(values, name).trim().to_string();
    match action {
        StoreActionKind::ModelPull => Some(format!(
            "tentgent model pull {}",
            shell_quote(&value("repo_id"))
        )),
        StoreActionKind::ModelImport => Some(format!(
            "tentgent model add {}",
            shell_quote(&value("path"))
        )),
        StoreActionKind::AdapterBind => selected.map(|selected| {
            format!(
                "tentgent adapter bind {} --base-model-ref {}",
                shell_quote(selected),
                shell_quote(&value("base_model_ref"))
            )
        }),
        StoreActionKind::DatasetImport => Some(format!(
            "tentgent dataset add {}",
            shell_quote(&value("path"))
        )),
        StoreActionKind::DatasetValidatePath => Some(format!(
            "tentgent dataset validate {}",
            shell_quote(&value("path"))
        )),
        StoreActionKind::DatasetExport => selected.map(|selected| {
            format!(
                "tentgent dataset export {} {}",
                shell_quote(selected),
                shell_quote(&value("output_path"))
            )
        }),
        StoreActionKind::DatasetDiffRef => selected.map(|selected| {
            format!(
                "tentgent dataset diff {} {}",
                shell_quote(selected),
                shell_quote(&value("right_dataset_ref"))
            )
        }),
        StoreActionKind::DatasetDiffPath => selected.map(|selected| {
            format!(
                "tentgent dataset diff {} --path {}",
                shell_quote(selected),
                shell_quote(&value("right_path"))
            )
        }),
        _ => None,
    }
}

fn field_value<'a>(values: &'a [(&'static str, String)], name: &str) -> &'a str {
    values
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.as_str())
        .unwrap_or("")
}

fn value_at_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn display_json_scalar(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(value) => format!("array({})", value.len()),
        Value::Object(value) => format!("object({})", value.len()),
    }
}

fn error_message(text: &str) -> Option<String> {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| value.get("error").and_then(Value::as_str))
                .map(ToOwned::to_owned)
        })
        .or_else(|| (!text.trim().is_empty()).then(|| text.trim().to_string()))
}

fn default_field_value(name: &str) -> String {
    match name {
        "split" => "train".to_string(),
        "count" => "20".to_string(),
        "input_format" => "jsonl".to_string(),
        _ => String::new(),
    }
}

fn move_index(current: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    let max = len - 1;
    if delta < 0 {
        current.saturating_sub(delta.unsigned_abs()).min(max)
    } else {
        current.saturating_add(delta as usize).min(max)
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn truncate(value: &str, max: usize) -> String {
    let mut chars = value.chars();
    let head: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{head}...")
    } else {
        head
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(item_ref: &str, short_ref: &str) -> NavigatorRow {
        NavigatorRow {
            item_ref: item_ref.to_string(),
            short_ref: short_ref.to_string(),
            columns: vec![],
            search_text: String::new(),
            summary: vec![],
            raw: json!({}),
        }
    }

    #[test]
    fn action_allowlist_excludes_auth_server_train_and_session_mutations() {
        let routes = ACTION_ALLOWED_ROUTES.join("\n");
        assert!(!routes.contains("/v1/auth"));
        assert!(!routes.contains("/v1/servers/{ref}/start"));
        assert!(!routes.contains("/v1/train/lora/plans"));
        assert!(!routes.contains("/v1/sessions/{ref}/compact"));
        assert!(!routes.contains("/v1/sessions/{ref}"));
    }

    #[test]
    fn ref_routes_are_percent_encoded() {
        let selected = row("provider:abc/def ghi", "abc");
        let request = build_action_request(
            StoreActionKind::AdapterBind,
            Some(&selected),
            &[("base_model_ref", "model".into())],
        )
        .expect("request");
        assert_eq!(request.path, "/v1/adapters/provider%3Aabc%2Fdef%20ghi/bind");
    }

    #[test]
    fn request_bodies_match_existing_dtos_and_delete_is_empty() {
        let pull = build_action_request(
            StoreActionKind::ModelPull,
            None,
            &[("repo_id", "org/model".into()), ("revision", "main".into())],
        )
        .expect("pull");
        assert_eq!(pull.method, Method::POST);
        assert_eq!(pull.body.as_ref().unwrap()["repo_id"], "org/model");
        assert_eq!(pull.body.as_ref().unwrap()["revision"], "main");
        assert_eq!(pull.path, "/v1/models/pull");
        assert_eq!(pull.job_path().as_deref(), Some("/v1/models/pull/jobs"));

        let selected = row("full-model-ref", "short-ref");
        let remove = build_action_request(StoreActionKind::ModelRemove, Some(&selected), &[])
            .expect("remove");
        assert_eq!(remove.method, Method::DELETE);
        assert!(remove.body.is_none());
        assert_eq!(remove.confirmation_token.as_deref(), Some("short-ref"));
        assert!(remove.confirmation_matches("short-ref"));
        assert!(remove.confirmation_matches("full-model-ref"));
    }

    #[test]
    fn long_running_actions_have_async_job_routes_without_mutating_sync_paths() {
        let import = build_action_request(
            StoreActionKind::DatasetImport,
            None,
            &[("path", "/tmp/dataset".into())],
        )
        .expect("import");
        assert_eq!(import.path, "/v1/datasets/import");
        assert_eq!(
            import.job_path().as_deref(),
            Some("/v1/datasets/import/jobs")
        );

        let prompt = build_action_request(
            StoreActionKind::DatasetSynthPromptBrief,
            None,
            &[
                ("brief", "make data".into()),
                ("split", "train".into()),
                ("count", "2".into()),
            ],
        )
        .expect("prompt");
        assert_eq!(prompt.path, "/v1/datasets/synth");
        assert!(prompt.job_path().is_none());
    }

    #[test]
    fn path_validation_rejects_blank_and_relative_paths() {
        let blank =
            build_action_request(StoreActionKind::ModelImport, None, &[("path", "".into())]);
        assert!(blank.unwrap_err().contains("path is required"));

        let relative = build_action_request(
            StoreActionKind::ModelImport,
            None,
            &[("path", "relative".into())],
        );
        assert!(relative.unwrap_err().contains("absolute path"));
    }

    #[test]
    fn synth_eval_require_provider_confirmation_without_auth_route() {
        let synth = build_action_request(
            StoreActionKind::DatasetSynthBrief,
            None,
            &[
                ("brief", "make examples".into()),
                ("provider", "openai".into()),
                ("model", "gpt-test".into()),
                ("output_path", "/tmp/out".into()),
                ("split", "train".into()),
                ("count", "2".into()),
            ],
        )
        .expect("synth");
        assert!(synth.provider_confirmation);
        assert!(synth.confirmation_matches("RUN"));
        assert_eq!(synth.path, "/v1/datasets/synth");
        assert!(!synth.path.contains("/v1/auth"));
    }

    #[test]
    fn raw_provider_output_is_not_summarized() {
        let value = json!({
            "synth": {"output_dir": "/tmp/out", "debug_dir": "/tmp/out/_debug"},
            "progress_events": [{"message": "ok"}],
            "provider_output": "SECRET RAW"
        });
        let lines = summarize_action_value(StoreActionKind::DatasetSynthBrief, &value);
        let rendered = format!("{lines:?}");
        assert!(rendered.contains("/tmp/out"));
        assert!(!rendered.contains("SECRET RAW"));
    }
}
