use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::Value;

use super::super::display::{format_bytes, format_optional_bytes};

pub(super) const SESSION_MESSAGES_TAIL: usize = 50;
pub(super) const TRAIN_METRICS_TAIL: usize = 100;
pub(super) const LOG_TAIL_BYTES: u64 = 65_536;
pub(super) const MAX_SESSION_MESSAGES_TAIL: usize = 1_000;
pub(super) const MAX_TRAIN_METRICS_TAIL: usize = 1_000;
pub(super) const MAX_LOG_TAIL_BYTES: u64 = 262_144;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum NavigatorListKind {
    Models,
    Adapters,
    Datasets,
    Servers,
    Sessions,
    TrainPlans,
    TrainRuns,
}

impl NavigatorListKind {
    pub(super) const ALL_DASHBOARD: [Self; 7] = [
        Self::Models,
        Self::Adapters,
        Self::Datasets,
        Self::Servers,
        Self::Sessions,
        Self::TrainPlans,
        Self::TrainRuns,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Models => "Models",
            Self::Adapters => "Adapters",
            Self::Datasets => "Datasets",
            Self::Servers => "Servers",
            Self::Sessions => "Sessions",
            Self::TrainPlans => "Train plans",
            Self::TrainRuns => "Train runs",
        }
    }

    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Models => "Models",
            Self::Adapters => "Adapters",
            Self::Datasets => "Datasets",
            Self::Servers => "Servers",
            Self::Sessions => "Sessions",
            Self::TrainPlans => "Training · Plans",
            Self::TrainRuns => "Training · Runs",
        }
    }

    pub(super) fn list_path(self) -> &'static str {
        match self {
            Self::Models => "/v1/models",
            Self::Adapters => "/v1/adapters",
            Self::Datasets => "/v1/datasets",
            Self::Servers => "/v1/servers",
            Self::Sessions => "/v1/sessions",
            Self::TrainPlans => "/v1/train/lora/plans",
            Self::TrainRuns => "/v1/train/lora/runs",
        }
    }

    pub(super) fn inspect_path(self, item_ref: &str) -> String {
        let encoded = percent_encode_path_segment(item_ref);
        match self {
            Self::Models => format!("/v1/models/{encoded}"),
            Self::Adapters => format!("/v1/adapters/{encoded}"),
            Self::Datasets => format!("/v1/datasets/{encoded}"),
            Self::Servers => format!("/v1/servers/{encoded}"),
            Self::Sessions => format!("/v1/sessions/{encoded}"),
            Self::TrainPlans => format!("/v1/train/lora/plans/{encoded}"),
            Self::TrainRuns => format!("/v1/train/lora/runs/{encoded}"),
        }
    }

    fn envelope_key(self) -> &'static str {
        match self {
            Self::Models => "models",
            Self::Adapters => "adapters",
            Self::Datasets => "datasets",
            Self::Servers => "servers",
            Self::Sessions => "sessions",
            Self::TrainPlans => "plans",
            Self::TrainRuns => "runs",
        }
    }

    fn detail_key(self) -> &'static str {
        match self {
            Self::Models => "model",
            Self::Adapters => "adapter",
            Self::Datasets => "dataset",
            Self::Servers => "server",
            Self::Sessions => "session",
            Self::TrainPlans => "plan",
            Self::TrainRuns => "run",
        }
    }

    pub(super) fn column_headers(self) -> [&'static str; 5] {
        match self {
            Self::Models => ["ref", "format", "source", "size", "path"],
            Self::Adapters => ["ref", "type", "base", "format", "path"],
            Self::Datasets => ["ref", "format", "ready", "splits", "path"],
            Self::Servers => ["ref", "state", "kind", "target", "port"],
            Self::Sessions => ["ref", "title", "messages", "updated", "server/adapter"],
            Self::TrainPlans => ["ref", "status", "model", "dataset", "runs"],
            Self::TrainRuns => ["ref", "status", "phase", "pid", "plan"],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TrainingTab {
    Plans,
    Runs,
}

impl TrainingTab {
    pub(super) fn list_kind(self) -> NavigatorListKind {
        match self {
            Self::Plans => NavigatorListKind::TrainPlans,
            Self::Runs => NavigatorListKind::TrainRuns,
        }
    }

    pub(super) fn toggle(&mut self) {
        *self = match self {
            Self::Plans => Self::Runs,
            Self::Runs => Self::Plans,
        };
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Plans => "Plans",
            Self::Runs => "Runs",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ServerLogKind {
    Stdout,
    Stderr,
}

impl ServerLogKind {
    pub(super) fn toggle(&mut self) {
        *self = match self {
            Self::Stdout => Self::Stderr,
            Self::Stderr => Self::Stdout,
        };
    }

    pub(super) fn path_segment(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TailSource {
    ServerLog {
        server_ref: String,
        kind: ServerLogKind,
        tail_bytes: u64,
    },
    SessionMessages {
        session_ref: String,
        tail: usize,
    },
    TrainRunMetrics {
        run_ref: String,
        tail: usize,
    },
    TrainRunRawLog {
        run_ref: String,
        tail_bytes: u64,
    },
}

impl TailSource {
    pub(super) fn path(&self) -> String {
        match self {
            Self::ServerLog {
                server_ref,
                kind,
                tail_bytes,
            } => format!(
                "/v1/servers/{}/logs/{}?tail_bytes={}",
                percent_encode_path_segment(server_ref),
                kind.path_segment(),
                capped_log_tail(*tail_bytes)
            ),
            Self::SessionMessages { session_ref, tail } => format!(
                "/v1/sessions/{}/messages?tail={}",
                percent_encode_path_segment(session_ref),
                capped_session_tail(*tail)
            ),
            Self::TrainRunMetrics { run_ref, tail } => format!(
                "/v1/train/lora/runs/{}/metrics?tail={}",
                percent_encode_path_segment(run_ref),
                capped_metrics_tail(*tail)
            ),
            Self::TrainRunRawLog {
                run_ref,
                tail_bytes,
            } => format!(
                "/v1/train/lora/runs/{}/logs/raw?tail_bytes={}",
                percent_encode_path_segment(run_ref),
                capped_log_tail(*tail_bytes)
            ),
        }
    }

    pub(super) fn title(&self) -> String {
        match self {
            Self::ServerLog { kind, .. } => format!("Server log · {}", kind.label()),
            Self::SessionMessages { .. } => "Session messages".to_string(),
            Self::TrainRunMetrics { .. } => "Train metrics".to_string(),
            Self::TrainRunRawLog { .. } => "Train raw log".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct TailPane {
    pub(super) source: TailSource,
    pub(super) loaded_at: String,
    pub(super) scroll_offset: usize,
    pub(super) truncated: bool,
    pub(super) lines: Vec<String>,
    pub(super) error: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct NavigatorRow {
    pub(super) item_ref: String,
    pub(super) short_ref: String,
    pub(super) columns: Vec<String>,
    pub(super) search_text: String,
    pub(super) summary: Vec<(String, String)>,
    pub(super) raw: Value,
}

#[derive(Debug, Clone)]
pub(super) struct NavigatorDetail {
    pub(super) item_ref: String,
    pub(super) loaded_at: String,
    pub(super) lines: Vec<(String, String)>,
    pub(super) raw: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum NavigatorLoadState {
    Idle,
    Loading { request_id: u64 },
    Ready,
    Error { message: String, stale: bool },
    StaleItem { message: String },
}

impl NavigatorLoadState {
    pub(super) fn label(&self) -> String {
        match self {
            Self::Idle => "not loaded".to_string(),
            Self::Loading { .. } => "loading".to_string(),
            Self::Ready => "ready".to_string(),
            Self::Error { message, stale } => {
                if *stale {
                    format!("stale; {message}")
                } else {
                    message.clone()
                }
            }
            Self::StaleItem { message } => message.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct NavigatorListState {
    pub(super) rows: Vec<NavigatorRow>,
    pub(super) selected_index: usize,
    pub(super) selected_ref: Option<String>,
    pub(super) filter: String,
    pub(super) load_state: NavigatorLoadState,
    pub(super) last_refreshed: Option<String>,
    pub(super) detail_cache: BTreeMap<String, NavigatorDetail>,
    pub(super) active_tail: Option<TailPane>,
    pub(super) server_log_kind: ServerLogKind,
}

impl Default for NavigatorListState {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            selected_index: 0,
            selected_ref: None,
            filter: String::new(),
            load_state: NavigatorLoadState::Idle,
            last_refreshed: None,
            detail_cache: BTreeMap::new(),
            active_tail: None,
            server_log_kind: ServerLogKind::Stderr,
        }
    }
}

impl NavigatorListState {
    pub(super) fn apply_rows(&mut self, rows: Vec<NavigatorRow>) {
        let previous_ref = self.selected_ref.clone();
        self.rows = rows;
        self.selected_index = previous_ref
            .as_deref()
            .and_then(|selected| {
                self.visible_rows()
                    .iter()
                    .position(|row| row.item_ref == selected)
            })
            .unwrap_or(0);
        self.selected_ref = self.selected_row().map(|row| row.item_ref.clone());
        self.load_state = NavigatorLoadState::Ready;
        self.last_refreshed = Some(now_label());
    }

    pub(super) fn visible_rows(&self) -> Vec<&NavigatorRow> {
        let filter = self.filter.trim().to_ascii_lowercase();
        if filter.is_empty() {
            return self.rows.iter().collect();
        }
        self.rows
            .iter()
            .filter(|row| row.search_text.to_ascii_lowercase().contains(&filter))
            .collect()
    }

    pub(super) fn selected_row(&self) -> Option<&NavigatorRow> {
        self.visible_rows().get(self.selected_index).copied()
    }

    pub(super) fn move_selection(&mut self, delta: isize) {
        let len = self.visible_rows().len();
        self.selected_index = move_index(self.selected_index, len, delta);
        self.selected_ref = self.selected_row().map(|row| row.item_ref.clone());
    }

    pub(super) fn set_filter(&mut self, filter: String) {
        let previous_ref = self.selected_ref.clone();
        self.filter = filter;
        self.selected_index = previous_ref
            .as_deref()
            .and_then(|selected| {
                self.visible_rows()
                    .iter()
                    .position(|row| row.item_ref == selected)
            })
            .unwrap_or(0);
        self.selected_ref = self.selected_row().map(|row| row.item_ref.clone());
    }

    pub(super) fn selected_detail(&self) -> Option<&NavigatorDetail> {
        let row = self.selected_row()?;
        self.detail_cache.get(&row.item_ref)
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct NavigatorState {
    pub(super) models: NavigatorListState,
    pub(super) adapters: NavigatorListState,
    pub(super) datasets: NavigatorListState,
    pub(super) servers: NavigatorListState,
    pub(super) sessions: NavigatorListState,
    pub(super) train_plans: NavigatorListState,
    pub(super) train_runs: NavigatorListState,
    pub(super) training_tab: TrainingTab,
}

impl NavigatorState {
    pub(super) fn state(&self, kind: NavigatorListKind) -> &NavigatorListState {
        match kind {
            NavigatorListKind::Models => &self.models,
            NavigatorListKind::Adapters => &self.adapters,
            NavigatorListKind::Datasets => &self.datasets,
            NavigatorListKind::Servers => &self.servers,
            NavigatorListKind::Sessions => &self.sessions,
            NavigatorListKind::TrainPlans => &self.train_plans,
            NavigatorListKind::TrainRuns => &self.train_runs,
        }
    }

    pub(super) fn state_mut(&mut self, kind: NavigatorListKind) -> &mut NavigatorListState {
        match kind {
            NavigatorListKind::Models => &mut self.models,
            NavigatorListKind::Adapters => &mut self.adapters,
            NavigatorListKind::Datasets => &mut self.datasets,
            NavigatorListKind::Servers => &mut self.servers,
            NavigatorListKind::Sessions => &mut self.sessions,
            NavigatorListKind::TrainPlans => &mut self.train_plans,
            NavigatorListKind::TrainRuns => &mut self.train_runs,
        }
    }
}

impl Default for TrainingTab {
    fn default() -> Self {
        Self::Plans
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct DashboardState {
    pub(super) cards: BTreeMap<NavigatorListKind, DashboardCard>,
}

impl DashboardState {
    pub(super) fn apply_updates(&mut self, updates: Vec<DashboardCountUpdate>) {
        for update in updates {
            let card = self
                .cards
                .entry(update.kind)
                .or_insert_with(|| DashboardCard {
                    label: update.kind.label().to_string(),
                    ..DashboardCard::default()
                });
            match update.result {
                Ok(count) => {
                    card.count_label = Some(count);
                    card.error = None;
                    card.last_ok = Some(now_label());
                    card.stale = false;
                }
                Err(error) => {
                    card.error = Some(error.to_string());
                    card.stale = card.count_label.is_some();
                }
            }
        }
    }

    pub(super) fn card(&self, kind: NavigatorListKind) -> DashboardCard {
        self.cards
            .get(&kind)
            .cloned()
            .unwrap_or_else(|| DashboardCard {
                label: kind.label().to_string(),
                ..DashboardCard::default()
            })
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct DashboardCard {
    pub(super) label: String,
    pub(super) count_label: Option<String>,
    pub(super) error: Option<String>,
    pub(super) last_ok: Option<String>,
    pub(super) stale: bool,
}

#[derive(Debug, Clone)]
pub(super) struct DashboardCountUpdate {
    pub(super) kind: NavigatorListKind,
    pub(super) result: Result<String, NavigatorError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum NavigatorError {
    AuthRequired(String),
    Down(String),
    NotFound(String),
    Timeout(String),
    Protocol(String),
    Server(String),
    Http { status: u16, message: String },
}

impl NavigatorError {
    pub(super) fn from_status(status: StatusCode, path: &str) -> Option<Self> {
        if status.is_success() {
            return None;
        }
        if status == StatusCode::UNAUTHORIZED {
            return Some(Self::AuthRequired(format!("{path} requires daemon auth")));
        }
        if status == StatusCode::NOT_FOUND {
            return Some(Self::NotFound(format!("{path} was not found")));
        }
        if status.is_server_error() {
            return Some(Self::Server(format!("{path} returned {status}")));
        }
        Some(Self::Http {
            status: status.as_u16(),
            message: format!("{path} returned {status}"),
        })
    }

    pub(super) fn is_auth_required(&self) -> bool {
        matches!(self, Self::AuthRequired(_))
    }

    pub(super) fn is_down(&self) -> bool {
        matches!(self, Self::Down(_))
    }

    pub(super) fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound(_))
    }
}

impl std::fmt::Display for NavigatorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AuthRequired(message)
            | Self::Down(message)
            | Self::NotFound(message)
            | Self::Timeout(message)
            | Self::Protocol(message)
            | Self::Server(message) => formatter.write_str(message),
            Self::Http { message, .. } => formatter.write_str(message),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct JsonEnvelope {
    #[serde(flatten)]
    values: BTreeMap<String, Value>,
}

pub(super) fn parse_list(
    kind: NavigatorListKind,
    value: Value,
) -> Result<Vec<NavigatorRow>, String> {
    let envelope: JsonEnvelope =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    let values = envelope
        .values
        .get(kind.envelope_key())
        .and_then(Value::as_array)
        .ok_or_else(|| format!("missing `{}` array", kind.envelope_key()))?;
    Ok(values
        .iter()
        .map(|item| row_from_value(kind, item.clone()))
        .collect())
}

pub(super) fn parse_detail(
    kind: NavigatorListKind,
    value: Value,
) -> Result<NavigatorDetail, String> {
    let envelope: JsonEnvelope =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    let item = envelope
        .values
        .get(kind.detail_key())
        .cloned()
        .ok_or_else(|| format!("missing `{}` object", kind.detail_key()))?;
    let item_ref = item_ref(kind, &item).unwrap_or_else(|| "(unknown)".to_string());
    Ok(NavigatorDetail {
        item_ref,
        loaded_at: now_label(),
        lines: detail_lines(kind, &item),
        raw: item,
    })
}

pub(super) fn parse_tail(source: TailSource, value: Value) -> Result<TailPane, String> {
    match &source {
        TailSource::SessionMessages { .. } => {
            let messages = value
                .get("messages")
                .and_then(Value::as_array)
                .ok_or_else(|| "missing `messages` array".to_string())?;
            let truncated = bool_field(&value, "truncated").unwrap_or(false);
            let mut lines = Vec::new();
            for message in messages {
                let index = usize_field(message, "index")
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "?".to_string());
                let role = string_field(message, "role").unwrap_or("message");
                let content = string_field(message, "content").unwrap_or("");
                lines.push(format!("#{index} {role}: {}", truncate(content, 240)));
            }
            Ok(TailPane {
                source,
                loaded_at: now_label(),
                scroll_offset: 0,
                truncated,
                lines,
                error: None,
            })
        }
        TailSource::TrainRunMetrics { .. } => {
            let events = value
                .get("events")
                .and_then(Value::as_array)
                .ok_or_else(|| "missing `events` array".to_string())?;
            let truncated = bool_field(&value, "truncated").unwrap_or(false);
            let lines = events
                .iter()
                .map(|event| {
                    let index = usize_field(event, "index")
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "?".to_string());
                    let body = event.get("event").unwrap_or(event);
                    format!("#{index} {}", truncate(&body.to_string(), 240))
                })
                .collect();
            Ok(TailPane {
                source,
                loaded_at: now_label(),
                scroll_offset: 0,
                truncated,
                lines,
                error: None,
            })
        }
        TailSource::ServerLog { .. } | TailSource::TrainRunRawLog { .. } => {
            let log = value
                .get("log")
                .ok_or_else(|| "missing `log` object".to_string())?;
            let content = string_field(log, "content").unwrap_or("");
            let mut lines = vec![
                format!("path: {}", str_or_dash(log, "path")),
                format!(
                    "exists: {}",
                    bool_field(log, "exists")
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "-".to_string())
                ),
                format!(
                    "size: {}",
                    format_optional_bytes(u64_field(log, "total_bytes"))
                ),
                format!(
                    "modified_at: {}",
                    string_field(log, "modified_at").unwrap_or("-")
                ),
                String::new(),
            ];
            lines.extend(content.lines().map(ToOwned::to_owned));
            Ok(TailPane {
                source,
                loaded_at: now_label(),
                scroll_offset: 0,
                truncated: bool_field(log, "truncated").unwrap_or(false),
                lines,
                error: None,
            })
        }
    }
}

pub(super) fn count_label(kind: NavigatorListKind, rows: &[NavigatorRow]) -> String {
    match kind {
        NavigatorListKind::Servers => {
            let running = rows
                .iter()
                .filter(|row| {
                    row.raw
                        .get("running")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                })
                .count();
            format!("{running}/{} running", rows.len())
        }
        NavigatorListKind::TrainRuns => {
            let active = rows
                .iter()
                .filter(|row| {
                    matches!(
                        string_field(&row.raw, "status"),
                        Some("running" | "starting" | "queued")
                    ) || row
                        .raw
                        .get("process_running")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                })
                .count();
            format!("{active}/{} active", rows.len())
        }
        _ => rows.len().to_string(),
    }
}

fn row_from_value(kind: NavigatorListKind, item: Value) -> NavigatorRow {
    let item_ref = item_ref(kind, &item).unwrap_or_else(|| "(missing-ref)".to_string());
    let short_ref = string_field(&item, "short_ref")
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| short_ref(&item_ref));
    let columns = columns_for(kind, &item, &short_ref);
    let summary = detail_lines(kind, &item);
    let search_text = searchable_text(&item, &columns, &summary);
    NavigatorRow {
        item_ref,
        short_ref,
        columns,
        search_text,
        summary,
        raw: item,
    }
}

fn columns_for(kind: NavigatorListKind, item: &Value, short_ref: &str) -> Vec<String> {
    match kind {
        NavigatorListKind::Models => vec![
            short_ref.to_string(),
            str_or_dash(item, "format"),
            str_or_dash(item, "source_kind"),
            format_optional_bytes(u64_field(item, "total_bytes")),
            str_or_dash(item, "store_path"),
        ],
        NavigatorListKind::Adapters => vec![
            short_ref.to_string(),
            str_or_dash(item, "type"),
            str_or_dash(item, "base_model_ref"),
            str_or_dash(item, "format"),
            str_or_dash(item, "store_path"),
        ],
        NavigatorListKind::Datasets => vec![
            short_ref.to_string(),
            str_or_dash(item, "format"),
            bool_field(item, "tuning_ready")
                .map(|value| if value { "ready" } else { "not ready" }.to_string())
                .unwrap_or_else(|| "-".to_string()),
            split_label(item.get("splits")),
            str_or_dash(item, "store_path"),
        ],
        NavigatorListKind::Servers => vec![
            short_ref.to_string(),
            bool_field(item, "running")
                .map(|running| if running { "running" } else { "stopped" }.to_string())
                .unwrap_or_else(|| "-".to_string()),
            str_or_dash(item, "runtime_kind"),
            first_nonempty(&[
                string_field(item, "model_ref"),
                string_field(item, "provider_model"),
                string_field(item, "provider"),
            ]),
            u64_field(item, "port")
                .map(|port| port.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ],
        NavigatorListKind::Sessions => vec![
            short_ref.to_string(),
            string_field(item, "title")
                .unwrap_or("(untitled)")
                .to_string(),
            usize_field(item, "message_count")
                .map(|count| count.to_string())
                .unwrap_or_else(|| "-".to_string()),
            str_or_dash(item, "updated_at"),
            compact_pair_label(
                "srv",
                string_field(item, "default_server_ref"),
                "adp",
                string_field(item, "adapter_ref"),
            ),
        ],
        NavigatorListKind::TrainPlans => vec![
            short_ref.to_string(),
            str_or_dash(item, "status"),
            str_or_dash(item, "model_ref"),
            str_or_dash(item, "dataset_ref"),
            usize_field(item, "run_count")
                .map(|count| count.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ],
        NavigatorListKind::TrainRuns => vec![
            short_ref.to_string(),
            str_or_dash(item, "status"),
            str_or_dash(item, "phase"),
            u64_field(item, "pid")
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "-".to_string()),
            str_or_dash(item, "plan_ref"),
        ],
    }
}

fn detail_lines(kind: NavigatorListKind, item: &Value) -> Vec<(String, String)> {
    let keys: &[&str] = match kind {
        NavigatorListKind::Models => &[
            "model_ref",
            "short_ref",
            "format",
            "detected_formats",
            "source_kind",
            "source_repo",
            "source_revision",
            "store_path",
            "manifest_path",
            "total_bytes",
            "file_count",
            "imported_at",
        ],
        NavigatorListKind::Adapters => &[
            "adapter_ref",
            "short_ref",
            "type",
            "format",
            "base_model_ref",
            "model_family",
            "backend_support",
            "source_kind",
            "store_path",
            "training_run_ref",
            "imported_at",
        ],
        NavigatorListKind::Datasets => &[
            "dataset_ref",
            "short_ref",
            "format",
            "tuning_ready",
            "splits",
            "warnings",
            "source_kind",
            "source_path",
            "store_path",
            "imported_at",
        ],
        NavigatorListKind::Servers => &[
            "server_ref",
            "short_ref",
            "runtime_kind",
            "running",
            "model_ref",
            "provider",
            "provider_model",
            "host",
            "port",
            "process",
            "server_dir",
            "stdout_log",
            "stderr_log",
            "created_at",
        ],
        NavigatorListKind::Sessions => &[
            "session_ref",
            "short_ref",
            "title",
            "message_count",
            "default_server_ref",
            "adapter_ref",
            "tags",
            "store_path",
            "messages_path",
            "created_at",
            "updated_at",
        ],
        NavigatorListKind::TrainPlans => &[
            "plan_ref",
            "short_ref",
            "name",
            "status",
            "model_ref",
            "dataset_ref",
            "requested_backend",
            "backend",
            "run_count",
            "plan_dir",
            "plan_path",
            "created_at",
        ],
        NavigatorListKind::TrainRuns => &[
            "run_ref",
            "short_ref",
            "status",
            "phase",
            "process_running",
            "stale",
            "error",
            "plan_ref",
            "model_ref",
            "dataset_ref",
            "backend",
            "pid",
            "adapter_ref",
            "run_dir",
            "metrics_path",
            "raw_log_path",
            "created_at",
            "started_at",
            "ended_at",
        ],
    };
    keys.iter()
        .filter_map(|key| item.get(*key).map(|value| detail_line_value(key, value)))
        .collect()
}

fn detail_line_value(key: &str, value: &Value) -> (String, String) {
    match key {
        "total_bytes" | "size_bytes" => (
            "size".to_string(),
            value
                .as_u64()
                .map(format_bytes)
                .unwrap_or_else(|| scalar(value)),
        ),
        _ => (key.to_string(), scalar(value)),
    }
}

fn item_ref(kind: NavigatorListKind, item: &Value) -> Option<String> {
    let key = match kind {
        NavigatorListKind::Models => "model_ref",
        NavigatorListKind::Adapters => "adapter_ref",
        NavigatorListKind::Datasets => "dataset_ref",
        NavigatorListKind::Servers => "server_ref",
        NavigatorListKind::Sessions => "session_ref",
        NavigatorListKind::TrainPlans => "plan_ref",
        NavigatorListKind::TrainRuns => "run_ref",
    };
    string_field(item, key).map(ToOwned::to_owned)
}

fn searchable_text(item: &Value, columns: &[String], summary: &[(String, String)]) -> String {
    let mut values = columns.to_vec();
    for key in [
        "status",
        "runtime_kind",
        "provider",
        "provider_model",
        "model_ref",
        "dataset_ref",
        "store_path",
        "server_dir",
        "run_dir",
        "title",
        "name",
    ] {
        if let Some(value) = string_field(item, key) {
            values.push(value.to_string());
        }
    }
    values.extend(
        summary
            .iter()
            .flat_map(|(key, value)| [key.clone(), value.clone()]),
    );
    values.join(" ")
}

pub(super) fn percent_encode_path_segment(value: &str) -> String {
    let mut output = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                output.push(*byte as char)
            }
            other => output.push_str(&format!("%{other:02X}")),
        }
    }
    output
}

pub(super) fn capped_session_tail(value: usize) -> usize {
    value.clamp(1, MAX_SESSION_MESSAGES_TAIL)
}

pub(super) fn capped_metrics_tail(value: usize) -> usize {
    value.clamp(1, MAX_TRAIN_METRICS_TAIL)
}

pub(super) fn capped_log_tail(value: u64) -> u64 {
    value.clamp(1, MAX_LOG_TAIL_BYTES)
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

fn string_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn str_or_dash(value: &Value, key: &str) -> String {
    string_field(value, key).unwrap_or("-").to_string()
}

fn bool_field(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

fn u64_field(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

fn usize_field(value: &Value, key: &str) -> Option<usize> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
}

fn scalar(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => "-".to_string(),
        _ => truncate(&value.to_string(), 160),
    }
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

pub(super) fn display_short_ref(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "(empty)".to_string()
    } else if trimmed.chars().count() > 12 {
        trimmed.chars().take(12).collect()
    } else {
        trimmed.to_string()
    }
}

pub(super) fn display_optional_short_ref(value: Option<&str>) -> String {
    value
        .filter(|value| !value.trim().is_empty())
        .map(display_short_ref)
        .unwrap_or_else(|| "-".to_string())
}

fn short_ref(value: &str) -> String {
    display_short_ref(value)
}

fn compact_pair_label(
    left_label: &str,
    left: Option<&str>,
    right_label: &str,
    right: Option<&str>,
) -> String {
    match (
        display_optional_short_ref(left),
        display_optional_short_ref(right),
    ) {
        (left, right) if left == "-" && right == "-" => "-".to_string(),
        (left, right) if right == "-" => format!("{left_label}:{left}"),
        (left, right) if left == "-" => format!("{right_label}:{right}"),
        (left, right) => format!("{left_label}:{left} {right_label}:{right}"),
    }
}

fn split_label(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return "-".to_string();
    };
    let mut parts = Vec::new();
    for key in ["train", "validation", "test", "eval_cases"] {
        if value.get(key).and_then(Value::as_str).is_some() {
            parts.push(key);
        }
    }
    if parts.is_empty() {
        "-".to_string()
    } else {
        parts.join(",")
    }
}

fn first_nonempty(values: &[Option<&str>]) -> String {
    values
        .iter()
        .flatten()
        .find(|value| !value.trim().is_empty())
        .map(|value| (*value).to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn now_label() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("{seconds}s")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_encoding_handles_path_refs() {
        assert_eq!(
            percent_encode_path_segment("openai:gpt 4/ref"),
            "openai%3Agpt%204%2Fref"
        );
        assert_eq!(
            NavigatorListKind::Servers.inspect_path("srv:abc/def"),
            "/v1/servers/srv%3Aabc%2Fdef"
        );
    }

    #[test]
    fn tail_paths_are_bounded() {
        let source = TailSource::SessionMessages {
            session_ref: "ses/1".to_string(),
            tail: usize::MAX,
        };
        assert_eq!(source.path(), "/v1/sessions/ses%2F1/messages?tail=1000");

        let source = TailSource::TrainRunRawLog {
            run_ref: "run 1".to_string(),
            tail_bytes: u64::MAX,
        };
        assert_eq!(
            source.path(),
            "/v1/train/lora/runs/run%201/logs/raw?tail_bytes=262144"
        );
    }

    #[test]
    fn dashboard_preserves_previous_count_on_partial_failure() {
        let mut dashboard = DashboardState::default();
        dashboard.apply_updates(vec![DashboardCountUpdate {
            kind: NavigatorListKind::Models,
            result: Ok("3".to_string()),
        }]);
        dashboard.apply_updates(vec![DashboardCountUpdate {
            kind: NavigatorListKind::Models,
            result: Err(NavigatorError::Server("boom".to_string())),
        }]);

        let card = dashboard.card(NavigatorListKind::Models);
        assert_eq!(card.count_label.as_deref(), Some("3"));
        assert!(card.stale);
        assert_eq!(card.error.as_deref(), Some("boom"));
    }

    #[test]
    fn parse_list_uses_existing_envelope_shape() {
        let value = serde_json::json!({
            "servers": [
                {
                    "server_ref": "srv_local",
                    "short_ref": "srv",
                    "runtime_kind": "local",
                    "running": true,
                    "model_ref": "model_a",
                    "port": 18765
                }
            ],
            "future": "ignored"
        });

        let rows = parse_list(NavigatorListKind::Servers, value).expect("rows");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].item_ref, "srv_local");
        assert!(rows[0].search_text.contains("model_a"));
    }

    #[test]
    fn model_rows_format_total_bytes_as_size() {
        let value = serde_json::json!({
            "models": [
                {
                    "model_ref": "model_1",
                    "short_ref": "model",
                    "format": "mlx",
                    "detected_formats": ["mlx"],
                    "source_kind": "local",
                    "source_repo": null,
                    "source_revision": null,
                    "store_path": "/tmp/model",
                    "manifest_path": "/tmp/model/manifest.json",
                    "total_bytes": 1536,
                    "file_count": 2,
                    "imported_at": "2026-05-07T00:00:00Z"
                }
            ]
        });

        let rows = parse_list(NavigatorListKind::Models, value).expect("models");

        assert_eq!(rows[0].columns[3], "1.5 KiB");
        assert!(rows[0]
            .summary
            .iter()
            .any(|(key, value)| key == "size" && value == "1.5 KiB"));
        assert!(!rows[0].summary.iter().any(|(key, _)| key == "total_bytes"));
    }

    #[test]
    fn detail_line_value_formats_size_bytes() {
        assert_eq!(
            detail_line_value("size_bytes", &serde_json::json!(2048)),
            ("size".to_string(), "2.0 KiB".to_string())
        );
    }

    #[test]
    fn sessions_table_columns_use_short_server_and_adapter_refs() {
        let server_ref = "5ab47943b50d1716340db7e1a80f4feac0febd26fbd08b3552f26f3128707626";
        let adapter_ref = "4c9fadc6cd715764b319ff1d6584e79bfb2cf5a2dc6ee9fac595fdc3a2186dc7";
        let value = serde_json::json!({
            "sessions": [
                {
                    "session_ref": "b00fc44d389a91f7df49afb6",
                    "short_ref": "b00fc44d389a",
                    "title": "TUI chat",
                    "message_count": 8,
                    "updated_at": "2026-05-05T05:54:10Z",
                    "default_server_ref": server_ref,
                    "adapter_ref": adapter_ref,
                    "tags": [],
                    "store_path": "/tmp/session"
                }
            ]
        });

        let rows = parse_list(NavigatorListKind::Sessions, value).expect("sessions");

        assert_eq!(rows[0].columns[4], "srv:5ab47943b50d adp:4c9fadc6cd71");
        assert!(!rows[0].columns[4].contains("1716340db7e1"));
        assert_eq!(
            NavigatorListKind::Sessions.column_headers()[4],
            "server/adapter"
        );
    }

    #[test]
    fn navigator_routes_are_read_only_and_skip_auth_route() {
        for kind in NavigatorListKind::ALL_DASHBOARD {
            let list = kind.list_path().to_string();
            let inspect = kind.inspect_path("ref:with space");
            for path in [list, inspect] {
                assert!(path.starts_with("/v1/"));
                assert!(!path.contains("/import"));
                assert!(!path.contains("/pull"));
                assert!(!path.contains("/start"));
                assert!(!path.contains("/stop"));
                assert!(!path.contains("/auth"));
            }
        }
    }

    #[test]
    fn local_filter_preserves_selection_by_ref_when_possible() {
        let mut state = NavigatorListState::default();
        state.apply_rows(vec![
            NavigatorRow {
                item_ref: "model-a".to_string(),
                short_ref: "a".to_string(),
                columns: vec!["a".to_string()],
                search_text: "alpha".to_string(),
                summary: Vec::new(),
                raw: Value::Null,
            },
            NavigatorRow {
                item_ref: "model-b".to_string(),
                short_ref: "b".to_string(),
                columns: vec!["b".to_string()],
                search_text: "beta".to_string(),
                summary: Vec::new(),
                raw: Value::Null,
            },
        ]);
        state.move_selection(1);
        assert_eq!(state.selected_ref.as_deref(), Some("model-b"));

        state.set_filter("bet".to_string());

        assert_eq!(state.selected_ref.as_deref(), Some("model-b"));
        assert_eq!(state.visible_rows().len(), 1);
    }
}
