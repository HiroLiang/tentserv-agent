use std::{
    net::IpAddr,
    time::{Duration, Instant},
};

use reqwest::{header, Method, StatusCode};
use serde_json::{Map, Value};

use super::{
    daemon_client::TuiTokenSource,
    navigator::{percent_encode_path_segment, NavigatorListKind, NavigatorRow},
    runtime_wizard::RuntimeWizardState,
};

const RUNTIME_ACTION_CONNECT_TIMEOUT: Duration = Duration::from_millis(700);

#[cfg(test)]
pub(super) const RUNTIME_ACTION_ALLOWED_ROUTES: [&str; 9] = [
    "POST /v1/servers",
    "POST /v1/servers/{ref}/start",
    "POST /v1/servers/{ref}/stop",
    "DELETE /v1/servers/{ref}",
    "POST /v1/train/lora/plans/preview",
    "POST /v1/train/lora/plans",
    "DELETE /v1/train/lora/plans/{ref}",
    "POST /v1/train/lora/plans/{ref}/runs",
    "GET /v1/train/lora/runs/{ref}/metrics?tail=N",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimeActionKind {
    ServerCreate,
    ServerCreateFromModel,
    ServerStart,
    ServerStop,
    ServerRemove,
    TrainPlanPreview,
    TrainPlanCreate,
    TrainPlanCreateFromDataset,
    TrainPlanRemove,
    TrainRunStart,
}

impl RuntimeActionKind {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::ServerCreate => "Create server spec",
            Self::ServerCreateFromModel => "Create server from model",
            Self::ServerStart => "Start server",
            Self::ServerStop => "Stop server",
            Self::ServerRemove => "Remove server spec",
            Self::TrainPlanPreview => "Preview LoRA plan",
            Self::TrainPlanCreate => "Create LoRA plan",
            Self::TrainPlanCreateFromDataset => "Create LoRA plan from dataset",
            Self::TrainPlanRemove => "Remove LoRA plan",
            Self::TrainRunStart => "Start LoRA run",
        }
    }

    pub(super) fn detail(self) -> &'static str {
        match self {
            Self::ServerCreate => "store a server spec without starting it",
            Self::ServerCreateFromModel => "prefill runtime_ref from selected model",
            Self::ServerStart => "background start with bounded readiness wait",
            Self::ServerStop => "stop one running server process",
            Self::ServerRemove => "delete one stopped server spec",
            Self::TrainPlanPreview => "validate/render without writing plan.toml",
            Self::TrainPlanCreate => "persist a normalized LoRA plan",
            Self::TrainPlanCreateFromDataset => "prefill dataset_ref from selected dataset",
            Self::TrainPlanRemove => "delete one plan with zero runs",
            Self::TrainRunStart => "launch a detached LoRA run-worker",
        }
    }

    pub(super) fn primary_section(self) -> NavigatorListKind {
        match self {
            Self::ServerCreate
            | Self::ServerCreateFromModel
            | Self::ServerStart
            | Self::ServerStop
            | Self::ServerRemove => NavigatorListKind::Servers,
            Self::TrainPlanPreview
            | Self::TrainPlanCreate
            | Self::TrainPlanCreateFromDataset
            | Self::TrainPlanRemove
            | Self::TrainRunStart => NavigatorListKind::TrainPlans,
        }
    }

    pub(super) fn requires_selection(self) -> bool {
        matches!(
            self,
            Self::ServerCreateFromModel
                | Self::ServerStart
                | Self::ServerStop
                | Self::ServerRemove
                | Self::TrainPlanCreateFromDataset
                | Self::TrainPlanRemove
                | Self::TrainRunStart
        )
    }

    pub(super) fn destructive(self) -> bool {
        matches!(self, Self::ServerRemove | Self::TrainPlanRemove)
    }

    pub(super) fn resource_confirmation(self) -> bool {
        matches!(self, Self::TrainRunStart)
    }

    pub(super) fn fields(self) -> Vec<RuntimeActionFieldSpec> {
        match self {
            Self::ServerCreate | Self::ServerCreateFromModel => vec![
                RuntimeActionFieldSpec::required("runtime_ref", RuntimeFieldKind::Ref),
                RuntimeActionFieldSpec::required("host", RuntimeFieldKind::Text),
                RuntimeActionFieldSpec::required("port", RuntimeFieldKind::Port),
                RuntimeActionFieldSpec::optional("lazy_load", RuntimeFieldKind::Bool),
                RuntimeActionFieldSpec::optional("idle_seconds", RuntimeFieldKind::PositiveInteger),
            ],
            Self::TrainPlanPreview | Self::TrainPlanCreate | Self::TrainPlanCreateFromDataset => {
                vec![
                    RuntimeActionFieldSpec::required("model_ref", RuntimeFieldKind::Ref),
                    RuntimeActionFieldSpec::required("dataset_ref", RuntimeFieldKind::Ref),
                    RuntimeActionFieldSpec::optional("name", RuntimeFieldKind::Text),
                    RuntimeActionFieldSpec::optional("backend", RuntimeFieldKind::Text),
                    RuntimeActionFieldSpec::optional(
                        "max_seq_length",
                        RuntimeFieldKind::PositiveInteger,
                    ),
                    RuntimeActionFieldSpec::optional("rank", RuntimeFieldKind::PositiveInteger),
                    RuntimeActionFieldSpec::optional("learning_rate", RuntimeFieldKind::Number),
                    RuntimeActionFieldSpec::optional(
                        "batch_size",
                        RuntimeFieldKind::PositiveInteger,
                    ),
                    RuntimeActionFieldSpec::optional(
                        "gradient_accumulation_steps",
                        RuntimeFieldKind::PositiveInteger,
                    ),
                    RuntimeActionFieldSpec::optional(
                        "max_steps",
                        RuntimeFieldKind::PositiveInteger,
                    ),
                    RuntimeActionFieldSpec::optional("seed", RuntimeFieldKind::PositiveInteger),
                    RuntimeActionFieldSpec::optional("mask_prompt", RuntimeFieldKind::Bool),
                    RuntimeActionFieldSpec::optional(
                        "mlx_num_layers",
                        RuntimeFieldKind::PositiveInteger,
                    ),
                    RuntimeActionFieldSpec::optional("mlx_grad_checkpoint", RuntimeFieldKind::Bool),
                    RuntimeActionFieldSpec::optional("peft_load_in_4bit", RuntimeFieldKind::Bool),
                    RuntimeActionFieldSpec::optional("peft_load_in_8bit", RuntimeFieldKind::Bool),
                ]
            }
            Self::ServerStart
            | Self::ServerStop
            | Self::ServerRemove
            | Self::TrainPlanRemove
            | Self::TrainRunStart => Vec::new(),
        }
    }

    pub(super) fn refresh_targets(self) -> &'static [NavigatorListKind] {
        match self {
            Self::ServerCreate
            | Self::ServerCreateFromModel
            | Self::ServerStart
            | Self::ServerStop
            | Self::ServerRemove => &[NavigatorListKind::Servers],
            Self::TrainPlanPreview => &[],
            Self::TrainPlanCreate | Self::TrainPlanCreateFromDataset | Self::TrainPlanRemove => {
                &[NavigatorListKind::TrainPlans]
            }
            Self::TrainRunStart => &[NavigatorListKind::TrainRuns],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimeFieldKind {
    Text,
    Ref,
    Port,
    PositiveInteger,
    Number,
    Bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RuntimeActionFieldSpec {
    pub(super) name: &'static str,
    pub(super) kind: RuntimeFieldKind,
    pub(super) required: bool,
}

impl RuntimeActionFieldSpec {
    const fn required(name: &'static str, kind: RuntimeFieldKind) -> Self {
        Self {
            name,
            kind,
            required: true,
        }
    }

    const fn optional(name: &'static str, kind: RuntimeFieldKind) -> Self {
        Self {
            name,
            kind,
            required: false,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeActionForm {
    pub(super) action: RuntimeActionKind,
    pub(super) fields: Vec<RuntimeActionFieldValue>,
    pub(super) selected_field: usize,
}

impl RuntimeActionForm {
    pub(super) fn new(action: RuntimeActionKind, selected: Option<&NavigatorRow>) -> Self {
        let fields = action
            .fields()
            .iter()
            .map(|spec| {
                let value = default_field_value(action, spec.name, selected);
                let cursor = value.chars().count();
                RuntimeActionFieldValue {
                    spec: *spec,
                    value,
                    cursor,
                }
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

    pub(super) fn selected_field_mut(&mut self) -> Option<&mut RuntimeActionFieldValue> {
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
pub(super) struct RuntimeActionFieldValue {
    pub(super) spec: RuntimeActionFieldSpec,
    pub(super) value: String,
    pub(super) cursor: usize,
}

#[derive(Debug, Clone)]
pub(super) enum RuntimeActionState {
    Idle,
    SelectingAction {
        kind: NavigatorListKind,
        actions: Vec<RuntimeActionKind>,
        selected: usize,
    },
    EditingForm {
        kind: NavigatorListKind,
        selected: Option<NavigatorRow>,
        form: RuntimeActionForm,
        error: Option<String>,
    },
    Wizard(RuntimeWizardState),
    WizardPreviewRunning {
        request_id: u64,
        generation: u64,
        wizard: RuntimeWizardState,
        request: RuntimeActionRequest,
        started_at: Instant,
    },
    Confirming {
        request: RuntimeActionRequest,
        typed: String,
        cursor: usize,
        message: String,
    },
    Running {
        request_id: u64,
        generation: u64,
        request: RuntimeActionRequest,
        started_at: Instant,
    },
    Result(RuntimeActionResult),
    Error {
        action: RuntimeActionKind,
        message: String,
        recoverable: bool,
    },
}

impl Default for RuntimeActionState {
    fn default() -> Self {
        Self::Idle
    }
}

impl RuntimeActionState {
    pub(super) fn is_active(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    pub(super) fn in_flight(&self) -> Option<u64> {
        match self {
            Self::Running { request_id, .. } | Self::WizardPreviewRunning { request_id, .. } => {
                Some(*request_id)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeActionRequest {
    pub(super) action: RuntimeActionKind,
    pub(super) method: Method,
    pub(super) path: String,
    pub(super) body: Option<Value>,
    pub(super) selected_ref: Option<String>,
    pub(super) selected_short_ref: Option<String>,
    pub(super) form_values: Vec<(&'static str, String)>,
    pub(super) confirmation_token: Option<String>,
    pub(super) resource_confirmation: bool,
    pub(super) warning: Option<String>,
    pub(super) refresh_targets: Vec<NavigatorListKind>,
    pub(super) cli_hint: Option<String>,
}

impl RuntimeActionRequest {
    pub(super) fn confirmation_matches(&self, typed: &str) -> bool {
        if self.resource_confirmation {
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
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeActionResult {
    pub(super) action: RuntimeActionKind,
    pub(super) status: u16,
    pub(super) lines: Vec<(String, String)>,
    pub(super) raw_summary: String,
    pub(super) refresh_targets: Vec<NavigatorListKind>,
    pub(super) selected_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RuntimeActionError {
    AuthRequired(String),
    BadRequest(String),
    NotFound(String),
    Conflict(String),
    Timeout(String),
    Down(String),
    Protocol(String),
    Http { status: u16, message: String },
}

impl std::fmt::Display for RuntimeActionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AuthRequired(message)
            | Self::BadRequest(message)
            | Self::NotFound(message)
            | Self::Conflict(message)
            | Self::Timeout(message)
            | Self::Down(message)
            | Self::Protocol(message) => formatter.write_str(message),
            Self::Http { status, message } => write!(formatter, "HTTP {status}: {message}"),
        }
    }
}

impl RuntimeActionError {
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

pub(super) struct RuntimeActionClient {
    base_url: String,
    token: Option<String>,
    client: reqwest::Client,
}

impl RuntimeActionClient {
    pub(super) fn new(
        base_url: String,
        token: Option<String>,
        _token_source: TuiTokenSource,
    ) -> miette::Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(RUNTIME_ACTION_CONNECT_TIMEOUT)
            .build()
            .map_err(|error| miette::miette!("failed to build runtime action client: {error}"))?;
        Ok(Self {
            base_url,
            token,
            client,
        })
    }

    pub(super) async fn execute(
        &self,
        request: RuntimeActionRequest,
    ) -> Result<RuntimeActionResult, RuntimeActionError> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), request.path);
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
                RuntimeActionError::Timeout(format!("{} timed out: {error}", request.path))
            } else {
                RuntimeActionError::Down(format!("{} failed: {error}", request.path))
            }
        })?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|error| RuntimeActionError::Protocol(error.to_string()))?;
        if !status.is_success() {
            return Err(RuntimeActionError::from_status(
                status,
                &request.path,
                &text,
            ));
        }
        let value: Value = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).map_err(|error| {
                RuntimeActionError::Protocol(format!("invalid {} JSON: {error}", request.path))
            })?
        };
        Ok(RuntimeActionResult {
            action: request.action,
            status: status.as_u16(),
            lines: summarize_runtime_value(request.action, &value),
            raw_summary: bounded_value_summary(&value),
            refresh_targets: request.refresh_targets,
            selected_ref: request.selected_ref,
        })
    }
}

pub(super) fn runtime_actions_for(
    kind: NavigatorListKind,
    training_tab: NavigatorListKind,
) -> Vec<RuntimeActionKind> {
    match kind {
        NavigatorListKind::Servers => vec![
            RuntimeActionKind::ServerCreate,
            RuntimeActionKind::ServerStart,
            RuntimeActionKind::ServerStop,
            RuntimeActionKind::ServerRemove,
        ],
        NavigatorListKind::Models => vec![RuntimeActionKind::ServerCreateFromModel],
        NavigatorListKind::Datasets => vec![RuntimeActionKind::TrainPlanCreateFromDataset],
        NavigatorListKind::TrainPlans if training_tab == NavigatorListKind::TrainPlans => vec![
            RuntimeActionKind::TrainPlanPreview,
            RuntimeActionKind::TrainPlanCreate,
            RuntimeActionKind::TrainPlanRemove,
            RuntimeActionKind::TrainRunStart,
        ],
        NavigatorListKind::TrainRuns if training_tab == NavigatorListKind::TrainRuns => Vec::new(),
        _ => Vec::new(),
    }
}

pub(super) fn build_runtime_action_request(
    action: RuntimeActionKind,
    selected: Option<&NavigatorRow>,
    values: &[(&'static str, String)],
) -> Result<RuntimeActionRequest, String> {
    let selected_ref = selected.map(|row| row.item_ref.clone());
    let selected_short_ref = selected.map(|row| row.short_ref.clone());
    if action.requires_selection() && selected_ref.is_none() {
        return Err(format!("{} requires a selected row", action.label()));
    }
    validate_values(action, values)?;
    let mut body = Map::new();
    insert_body_values(action, &mut body, values)?;
    let selected_ref_str = selected_ref.as_deref();
    let path = runtime_action_path(action, selected_ref_str)?;
    let method = if action.destructive() {
        Method::DELETE
    } else {
        Method::POST
    };
    let body = match action {
        RuntimeActionKind::ServerStart => {
            Some(serde_json::json!({"wait_ready": true, "timeout_seconds": 30}))
        }
        RuntimeActionKind::TrainRunStart => Some(Value::Object(Map::new())),
        _ if method == Method::DELETE => None,
        _ => Some(Value::Object(body)),
    };
    let confirmation_token = action.destructive().then(|| {
        selected_short_ref
            .clone()
            .or_else(|| selected_ref.clone())
            .expect("destructive action requires selection")
    });
    let warning = action_warning(action, values);
    Ok(RuntimeActionRequest {
        action,
        method,
        path,
        body,
        selected_ref,
        selected_short_ref,
        form_values: values.to_vec(),
        confirmation_token,
        resource_confirmation: action.resource_confirmation(),
        warning,
        refresh_targets: action.refresh_targets().to_vec(),
        cli_hint: cli_hint(action, values, selected),
    })
}

fn runtime_action_path(
    action: RuntimeActionKind,
    selected_ref: Option<&str>,
) -> Result<String, String> {
    let selected = selected_ref.map(percent_encode_path_segment);
    Ok(match action {
        RuntimeActionKind::ServerCreate | RuntimeActionKind::ServerCreateFromModel => {
            "/v1/servers".to_string()
        }
        RuntimeActionKind::ServerStart => {
            format!(
                "/v1/servers/{}/start",
                selected.ok_or("missing server ref")?
            )
        }
        RuntimeActionKind::ServerStop => {
            format!("/v1/servers/{}/stop", selected.ok_or("missing server ref")?)
        }
        RuntimeActionKind::ServerRemove => {
            format!("/v1/servers/{}", selected.ok_or("missing server ref")?)
        }
        RuntimeActionKind::TrainPlanPreview => "/v1/train/lora/plans/preview".to_string(),
        RuntimeActionKind::TrainPlanCreate | RuntimeActionKind::TrainPlanCreateFromDataset => {
            "/v1/train/lora/plans".to_string()
        }
        RuntimeActionKind::TrainPlanRemove => {
            format!(
                "/v1/train/lora/plans/{}",
                selected.ok_or("missing plan ref")?
            )
        }
        RuntimeActionKind::TrainRunStart => {
            format!(
                "/v1/train/lora/plans/{}/runs",
                selected.ok_or("missing plan ref")?
            )
        }
    })
}

fn validate_values(
    action: RuntimeActionKind,
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
            RuntimeFieldKind::Port => {
                let parsed = value
                    .trim()
                    .parse::<u16>()
                    .map_err(|_| format!("{} must be a port between 1 and 65535", spec.name))?;
                if parsed == 0 {
                    return Err(format!("{} must be a port between 1 and 65535", spec.name));
                }
            }
            RuntimeFieldKind::PositiveInteger => {
                let parsed = value
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| format!("{} must be a positive integer", spec.name))?;
                if parsed == 0 {
                    return Err(format!("{} must be greater than zero", spec.name));
                }
            }
            RuntimeFieldKind::Number => {
                value
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| format!("{} must be a number", spec.name))?;
            }
            RuntimeFieldKind::Bool => {
                parse_bool(value).ok_or_else(|| format!("{} must be true or false", spec.name))?;
            }
            RuntimeFieldKind::Ref => {
                if value.trim().contains('/') {
                    return Err(format!(
                        "{} must be a managed/runtime ref, not a path",
                        spec.name
                    ));
                }
            }
            RuntimeFieldKind::Text => {}
        }
    }
    if matches!(
        action,
        RuntimeActionKind::TrainPlanPreview
            | RuntimeActionKind::TrainPlanCreate
            | RuntimeActionKind::TrainPlanCreateFromDataset
    ) && parse_bool(field_value(values, "peft_load_in_4bit")).unwrap_or(false)
        && parse_bool(field_value(values, "peft_load_in_8bit")).unwrap_or(false)
    {
        return Err("peft_load_in_4bit and peft_load_in_8bit are mutually exclusive".to_string());
    }
    Ok(())
}

fn insert_body_values(
    action: RuntimeActionKind,
    body: &mut Map<String, Value>,
    values: &[(&'static str, String)],
) -> Result<(), String> {
    match action {
        RuntimeActionKind::TrainPlanPreview
        | RuntimeActionKind::TrainPlanCreate
        | RuntimeActionKind::TrainPlanCreateFromDataset => {
            let mut overrides = Map::new();
            for (key, value) in values {
                if value.trim().is_empty() {
                    continue;
                }
                if is_train_override(key) {
                    insert_typed_value(&mut overrides, key, value)?;
                } else {
                    insert_typed_value(body, key, value)?;
                }
            }
            if !overrides.is_empty() {
                body.insert("overrides".to_string(), Value::Object(overrides));
            }
        }
        _ => {
            for (key, value) in values {
                if value.trim().is_empty() {
                    continue;
                }
                insert_typed_value(body, key, value)?;
            }
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
        "port"
        | "idle_seconds"
        | "max_seq_length"
        | "rank"
        | "batch_size"
        | "gradient_accumulation_steps"
        | "max_steps"
        | "seed"
        | "mlx_num_layers" => Value::Number(
            trimmed
                .parse::<u64>()
                .map_err(|_| format!("{key} must be an integer"))?
                .into(),
        ),
        "learning_rate" => {
            let number = trimmed
                .parse::<f64>()
                .map_err(|_| format!("{key} must be a number"))?;
            Value::Number(
                serde_json::Number::from_f64(number)
                    .ok_or_else(|| format!("{key} must be finite"))?,
            )
        }
        "lazy_load"
        | "mask_prompt"
        | "mlx_grad_checkpoint"
        | "peft_load_in_4bit"
        | "peft_load_in_8bit" => {
            Value::Bool(parse_bool(trimmed).ok_or_else(|| format!("{key} must be true or false"))?)
        }
        _ => Value::String(trimmed.to_string()),
    };
    body.insert(key.to_string(), typed);
    Ok(())
}

fn action_warning(action: RuntimeActionKind, values: &[(&'static str, String)]) -> Option<String> {
    match action {
        RuntimeActionKind::ServerCreate | RuntimeActionKind::ServerCreateFromModel => {
            let host = field_value(values, "host").trim();
            server_bind_warning(host)
        }
        RuntimeActionKind::TrainRunStart => Some(
            "LoRA training may consume CPU/GPU, disk, power, and time. Type RUN to launch."
                .to_string(),
        ),
        _ => None,
    }
}

fn server_bind_warning(host: &str) -> Option<String> {
    if host.eq_ignore_ascii_case("localhost") {
        return None;
    }
    let ip: IpAddr = host.parse().ok()?;
    if ip.is_loopback() {
        None
    } else {
        Some(format!(
            "host `{host}` is not loopback; confirm this bind is intentional before submit"
        ))
    }
}

fn is_train_override(key: &str) -> bool {
    matches!(
        key,
        "max_seq_length"
            | "rank"
            | "learning_rate"
            | "batch_size"
            | "gradient_accumulation_steps"
            | "max_steps"
            | "seed"
            | "mask_prompt"
            | "mlx_num_layers"
            | "mlx_grad_checkpoint"
            | "peft_load_in_4bit"
            | "peft_load_in_8bit"
    )
}

fn summarize_runtime_value(action: RuntimeActionKind, value: &Value) -> Vec<(String, String)> {
    let mut lines = vec![("action".to_string(), action.label().to_string())];
    for path in [
        &["server", "short_ref"][..],
        &["server", "running"],
        &["server", "host"],
        &["server", "port"],
        &["server", "process", "pid"],
        &["created"],
        &["readiness", "ready"],
        &["readiness", "reachable"],
        &["readiness", "error"],
        &["stopped_pid"],
        &["removed", "short_ref"],
        &["plan", "short_ref"],
        &["plan", "status"],
        &["plan", "backend"],
        &["plan", "model_ref"],
        &["plan", "dataset_ref"],
        &["created"],
        &["deduplicated"],
        &["run_count"],
        &["would_plan_path"],
        &["plan_path"],
        &["run", "short_ref"],
        &["run", "status"],
        &["run", "phase"],
        &["run", "pid"],
    ] {
        if let Some(found) = value_at_path(value, path) {
            lines.push((path.join("."), display_json_scalar(found)));
        }
    }
    if let Some(blockers) = value_at_path(value, &["plan", "blockers"]) {
        lines.push(("plan.blockers".to_string(), display_json_scalar(blockers)));
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
    action: RuntimeActionKind,
    values: &[(&'static str, String)],
    selected: Option<&NavigatorRow>,
) -> Option<String> {
    let selected = selected.map(|row| row.short_ref.as_str());
    let value = |name| field_value(values, name).trim().to_string();
    match action {
        RuntimeActionKind::ServerCreate | RuntimeActionKind::ServerCreateFromModel => {
            Some(format!(
                "tentgent server run {} --host {} --port {}",
                shell_quote(&value("runtime_ref")),
                shell_quote(&value("host")),
                shell_quote(&value("port"))
            ))
        }
        RuntimeActionKind::ServerStart => {
            selected.map(|selected| format!("tentgent server start {}", shell_quote(selected)))
        }
        RuntimeActionKind::ServerStop => {
            selected.map(|selected| format!("tentgent server stop {}", shell_quote(selected)))
        }
        RuntimeActionKind::TrainPlanCreate | RuntimeActionKind::TrainPlanCreateFromDataset => {
            Some(format!(
                "tentgent train lora plan create --model {} --dataset {}",
                shell_quote(&value("model_ref")),
                shell_quote(&value("dataset_ref"))
            ))
        }
        RuntimeActionKind::TrainRunStart => {
            selected.map(|selected| format!("tentgent train lora run {}", shell_quote(selected)))
        }
        _ => None,
    }
}

fn default_field_value(
    action: RuntimeActionKind,
    name: &str,
    selected: Option<&NavigatorRow>,
) -> String {
    match (action, name) {
        (
            RuntimeActionKind::ServerCreate | RuntimeActionKind::ServerCreateFromModel,
            "runtime_ref",
        ) => selected.map(|row| row.item_ref.clone()).unwrap_or_default(),
        (RuntimeActionKind::ServerCreate | RuntimeActionKind::ServerCreateFromModel, "host") => {
            "127.0.0.1".to_string()
        }
        (RuntimeActionKind::ServerCreate | RuntimeActionKind::ServerCreateFromModel, "port") => {
            "8780".to_string()
        }
        (
            RuntimeActionKind::ServerCreate | RuntimeActionKind::ServerCreateFromModel,
            "lazy_load",
        ) => "true".to_string(),
        (RuntimeActionKind::TrainPlanPreview | RuntimeActionKind::TrainPlanCreate, "backend")
        | (RuntimeActionKind::TrainPlanCreateFromDataset, "backend") => "auto".to_string(),
        (RuntimeActionKind::TrainPlanCreateFromDataset, "dataset_ref") => {
            selected.map(|row| row.item_ref.clone()).unwrap_or_default()
        }
        _ => String::new(),
    }
}

fn field_value<'a>(values: &'a [(&'static str, String)], name: &str) -> &'a str {
    values
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.as_str())
        .unwrap_or("")
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" => None,
        "true" | "yes" | "y" | "1" => Some(true),
        "false" | "no" | "n" | "0" => Some(false),
        _ => None,
    }
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
    use super::super::runtime_wizard::{
        RuntimePickerMode, RuntimePreviewStatus, RuntimeWizardAdvancedChoice, RuntimeWizardBackend,
        RuntimeWizardFlow, RuntimeWizardReviewRow, RuntimeWizardState, RuntimeWizardStep,
    };
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
    fn runtime_action_allowlist_excludes_auth_chat_sessions_store_and_unknown_routes() {
        let routes = RUNTIME_ACTION_ALLOWED_ROUTES.join("\n");
        assert!(!routes.contains("/v1/auth"));
        assert!(!routes.contains("/v1/chat"));
        assert!(!routes.contains("/v1/sessions"));
        assert!(!routes.contains("/v1/models/pull"));
        assert!(!routes.contains("/jobs"));
    }

    #[test]
    fn runtime_ref_routes_are_percent_encoded() {
        let selected = row("srv:abc/def ghi", "srv");
        let request =
            build_runtime_action_request(RuntimeActionKind::ServerStart, Some(&selected), &[])
                .expect("start");
        assert_eq!(request.path, "/v1/servers/srv%3Aabc%2Fdef%20ghi/start");
    }

    #[test]
    fn server_requests_match_existing_dtos_and_delete_is_empty() {
        let create = build_runtime_action_request(
            RuntimeActionKind::ServerCreate,
            None,
            &[
                ("runtime_ref", "openai:gpt-test".into()),
                ("host", "127.0.0.1".into()),
                ("port", "8799".into()),
                ("lazy_load", "true".into()),
                ("idle_seconds", "".into()),
            ],
        )
        .expect("create");
        assert_eq!(create.path, "/v1/servers");
        assert_eq!(
            create.body.as_ref().unwrap()["runtime_ref"],
            "openai:gpt-test"
        );
        assert_eq!(create.body.as_ref().unwrap()["port"], 8799);
        assert!(create.body.as_ref().unwrap().get("idle_seconds").is_none());

        let selected = row("server-full", "server-short");
        let start =
            build_runtime_action_request(RuntimeActionKind::ServerStart, Some(&selected), &[])
                .expect("start");
        assert_eq!(start.body.as_ref().unwrap()["wait_ready"], true);
        assert_eq!(start.body.as_ref().unwrap()["timeout_seconds"], 30);

        let remove =
            build_runtime_action_request(RuntimeActionKind::ServerRemove, Some(&selected), &[])
                .expect("remove");
        assert_eq!(remove.method, Method::DELETE);
        assert!(remove.body.is_none());
        assert!(remove.confirmation_matches("server-short"));
        assert!(remove.confirmation_matches("server-full"));
    }

    #[test]
    fn train_preview_and_create_share_omitting_override_builder() {
        let values = [
            ("model_ref", "model1".into()),
            ("dataset_ref", "dataset1".into()),
            ("name", "".into()),
            ("backend", "auto".into()),
            ("max_seq_length", "1024".into()),
            ("rank", "".into()),
            ("learning_rate", "".into()),
            ("batch_size", "".into()),
            ("gradient_accumulation_steps", "".into()),
            ("max_steps", "".into()),
            ("seed", "".into()),
            ("mask_prompt", "".into()),
            ("mlx_num_layers", "".into()),
            ("mlx_grad_checkpoint", "".into()),
            ("peft_load_in_4bit", "".into()),
            ("peft_load_in_8bit", "".into()),
        ];
        let preview =
            build_runtime_action_request(RuntimeActionKind::TrainPlanPreview, None, &values)
                .expect("preview");
        let create =
            build_runtime_action_request(RuntimeActionKind::TrainPlanCreate, None, &values)
                .expect("create");
        assert_eq!(preview.body, create.body);
        let body = preview.body.unwrap();
        assert_eq!(body["model_ref"], "model1");
        assert_eq!(body["dataset_ref"], "dataset1");
        assert_eq!(body["overrides"]["max_seq_length"], 1024);
        assert!(body["overrides"].get("rank").is_none());
    }

    #[test]
    fn server_create_wizard_starts_with_model_picker_and_manual_fallback_exists() {
        let wizard =
            RuntimeWizardState::new(RuntimeActionKind::ServerCreate, None).expect("wizard");
        assert_eq!(wizard.flow, RuntimeWizardFlow::CreateServer);
        assert_eq!(wizard.step, RuntimeWizardStep::PickModel);
        let picker = wizard.picker.expect("picker");
        assert_eq!(picker.kind, NavigatorListKind::Models);
        assert_eq!(picker.mode, RuntimePickerMode::Local);
    }

    #[test]
    fn lora_wizard_review_uses_shared_preview_and_create_values() {
        let mut wizard =
            RuntimeWizardState::new(RuntimeActionKind::TrainPlanCreate, None).expect("wizard");
        wizard.draft.model_ref = "model1".to_string();
        wizard.draft.dataset_ref = "dataset1".to_string();
        wizard.draft.backend = RuntimeWizardBackend::Mlx;
        wizard.draft.advanced_choice = RuntimeWizardAdvancedChoice::Customize;
        wizard.draft.rank = "8".to_string();

        let preview = build_runtime_action_request(
            RuntimeActionKind::TrainPlanPreview,
            None,
            &wizard.preview_values(),
        )
        .expect("preview");
        let create = build_runtime_action_request(
            RuntimeActionKind::TrainPlanCreate,
            None,
            &wizard.create_values(),
        )
        .expect("create");

        assert_eq!(preview.body, create.body);
        let body = create.body.unwrap();
        assert_eq!(body["model_ref"], "model1");
        assert_eq!(body["dataset_ref"], "dataset1");
        assert_eq!(body["backend"], "mlx");
        assert_eq!(body["overrides"]["rank"], 8);
    }

    #[test]
    fn lora_preview_stale_when_draft_changes_after_ready_preview() {
        let mut wizard =
            RuntimeWizardState::new(RuntimeActionKind::TrainPlanCreate, None).expect("wizard");
        wizard.preview.status = RuntimePreviewStatus::Ready;
        wizard.dirty_since_preview = false;
        wizard.mark_dirty();
        assert_eq!(wizard.preview.status, RuntimePreviewStatus::Stale);
        assert!(wizard.dirty_since_preview);
    }

    #[test]
    fn review_field_edit_jumps_to_picker_backed_steps() {
        assert_eq!(
            RuntimeWizardState::new(RuntimeActionKind::TrainPlanCreate, None)
                .unwrap()
                .review_rows()
                .first()
                .cloned(),
            Some(RuntimeWizardReviewRow::Field("model_ref", String::new()))
        );
    }

    #[test]
    fn train_run_start_sends_empty_body_and_requires_resource_confirmation() {
        let selected = row("plan-full", "plan-short");
        let request =
            build_runtime_action_request(RuntimeActionKind::TrainRunStart, Some(&selected), &[])
                .expect("run");
        assert_eq!(request.path, "/v1/train/lora/plans/plan-full/runs");
        assert_eq!(request.body.as_ref().unwrap(), &Value::Object(Map::new()));
        assert!(request.resource_confirmation);
        assert!(request.confirmation_matches("RUN"));
    }

    #[test]
    fn host_port_and_bool_validation_are_local_shape_checks() {
        let invalid = build_runtime_action_request(
            RuntimeActionKind::ServerCreate,
            None,
            &[
                ("runtime_ref", "model".into()),
                ("host", "127.0.0.1".into()),
                ("port", "70000".into()),
                ("lazy_load", "true".into()),
                ("idle_seconds", "".into()),
            ],
        );
        assert!(invalid.unwrap_err().contains("port"));

        let warning = build_runtime_action_request(
            RuntimeActionKind::ServerCreate,
            None,
            &[
                ("runtime_ref", "model".into()),
                ("host", "0.0.0.0".into()),
                ("port", "8799".into()),
                ("lazy_load", "true".into()),
                ("idle_seconds", "".into()),
            ],
        )
        .expect("warning");
        assert!(warning.warning.unwrap().contains("not loopback"));
    }
}
