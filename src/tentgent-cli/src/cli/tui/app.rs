use std::{
    env,
    net::IpAddr,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use miette::IntoDiagnostic;
use reqwest::Url;
use tentgent_core::{
    auth::{env_key_status, AuthManager, KeyStatus, Provider},
    config::{
        config_path, resolve_daemon_token, resolve_daemon_url, DaemonTokenSource, DaemonUrlInputs,
        DaemonUrlResolution, TentgentConfig, DAEMON_URL_ENV_VAR,
    },
    daemon::{DaemonInspection, DaemonManager, DEFAULT_DAEMON_PORT},
};
use tentgent_http::security::DAEMON_TOKEN_ENV_VAR;
use tokio::{sync::mpsc, task::JoinHandle};

use crate::cli::{
    commands::TuiCommand,
    daemon::{start_daemon_detached, DetachedDaemonOptions, DetachedDaemonStartOutcome},
};

use super::{
    chat::{
        adapter_rows_from_navigator, ChatClient, ChatConflictKind, ChatContextMode, ChatError,
        ChatFocus, ChatLoadState, ChatMessages, ChatOverview, ChatPhase, ChatSendRequest,
        ChatSendState, ChatState, ChatStreamEvent, SseDecoder,
    },
    daemon_client::{DaemonClient, DaemonConnectionState, DaemonSnapshot, TuiTokenSource},
    navigator::{
        count_label, DashboardCountUpdate, DashboardState, NavigatorDetail, NavigatorError,
        NavigatorListKind, NavigatorLoadState, NavigatorRow, NavigatorState, TailPane, TailSource,
        LOG_TAIL_BYTES, SESSION_MESSAGES_TAIL, TRAIN_METRICS_TAIL,
    },
    resource::{
        collect_resource_snapshot, ResourceInputs, ResourceLoadState, ResourceSnapshot,
        ResourceState,
    },
    terminal::TerminalSession,
};

const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(100);
const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

type TuiEventSender = mpsc::UnboundedSender<TuiEvent>;
type TuiEventReceiver = mpsc::UnboundedReceiver<TuiEvent>;

#[derive(Debug)]
enum TuiEvent {
    RefreshFinished {
        request_id: u64,
        generation: u64,
        result: Result<RefreshData, String>,
    },
    StartFinished {
        request_id: u64,
        generation: u64,
        result: Result<DetachedDaemonStartOutcome, String>,
    },
    ProviderActionFinished {
        request_id: u64,
        provider: Provider,
        result: ProviderActionResult,
    },
    NavigatorListFinished {
        request_id: u64,
        generation: u64,
        kind: NavigatorListKind,
        result: Result<Vec<NavigatorRow>, NavigatorError>,
    },
    NavigatorDetailFinished {
        request_id: u64,
        generation: u64,
        kind: NavigatorListKind,
        item_ref: String,
        result: Result<NavigatorDetail, NavigatorError>,
    },
    NavigatorTailFinished {
        request_id: u64,
        generation: u64,
        kind: NavigatorListKind,
        source: TailSource,
        result: Result<TailPane, NavigatorError>,
    },
    ResourceFinished {
        request_id: u64,
        generation: u64,
        result: Result<ResourceSnapshot, String>,
    },
    ChatOverviewFinished {
        request_id: u64,
        generation: u64,
        result: Result<ChatOverview, ChatError>,
    },
    ChatSessionCreated {
        request_id: u64,
        generation: u64,
        result: Result<super::chat::ChatSessionRow, ChatError>,
    },
    ChatMessagesFinished {
        request_id: u64,
        generation: u64,
        session_ref: String,
        result: Result<ChatMessages, ChatError>,
    },
    ChatDelta {
        request_id: u64,
        generation: u64,
        delta: String,
    },
    ChatDone {
        request_id: u64,
        generation: u64,
        finish_reason: String,
    },
    ChatSendError {
        request_id: u64,
        generation: u64,
        error: ChatError,
        retry: Option<ChatSendRequest>,
        may_have_committed: bool,
    },
    ChatNonStreamFinished {
        request_id: u64,
        generation: u64,
        result: Result<String, ChatError>,
    },
}

#[derive(Debug)]
struct RefreshData {
    inspection: DaemonInspection,
    config: TentgentConfig,
    config_error: Option<String>,
    daemon_url: DaemonUrlResolution,
    daemon_token: TuiDaemonToken,
    daemon: DaemonSnapshot,
    dashboard_counts: Option<Vec<DashboardCountUpdate>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AppMode {
    Bootstrap(BootstrapReason),
    Operator,
}

impl AppMode {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Bootstrap(reason) => reason.label(),
            Self::Operator => "operator",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BootstrapReason {
    DaemonDown,
    AuthRequired,
    DaemonTimeout,
    ProtocolError,
    ConfigError,
}

impl BootstrapReason {
    fn label(self) -> &'static str {
        match self {
            Self::DaemonDown => "bootstrap: daemon down",
            Self::AuthRequired => "bootstrap: auth required",
            Self::DaemonTimeout => "bootstrap: daemon timeout",
            Self::ProtocolError => "bootstrap: daemon protocol error",
            Self::ConfigError => "bootstrap: config error",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FocusPane {
    Menu,
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MenuItem {
    StartDaemon,
    ProviderAuth,
    Settings,
    Dashboard,
    Chat,
    Models,
    Adapters,
    Datasets,
    Servers,
    Sessions,
    Training,
    Resources,
}

#[derive(Debug, Clone)]
pub(super) struct MenuEntry {
    pub(super) item: MenuItem,
    pub(super) label: &'static str,
    pub(super) detail: &'static str,
    pub(super) enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SettingsItem {
    RuntimeHome,
    DaemonHost,
    DaemonPort,
    DaemonToken,
    ConfigPath,
    RuntimeDir,
    LogDir,
}

#[derive(Debug, Clone)]
pub(super) struct SettingsEntry {
    pub(super) item: SettingsItem,
    pub(super) label: &'static str,
    pub(super) value: String,
    pub(super) detail: &'static str,
    pub(super) editable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ProviderAuthDisplayState {
    EnvPresent,
    EnvMissingKeychainNotChecked,
    KeychainPresentChecked,
    KeychainMissingChecked,
    CheckFailed(String),
    Pending(&'static str),
}

impl ProviderAuthDisplayState {
    pub(super) fn label(&self) -> String {
        match self {
            Self::EnvPresent => "env-only; keychain not checked".to_string(),
            Self::EnvMissingKeychainNotChecked => "env missing; keychain not checked".to_string(),
            Self::KeychainPresentChecked => "manual check: keychain present".to_string(),
            Self::KeychainMissingChecked => "manual check: keychain missing".to_string(),
            Self::CheckFailed(error) => format!("manual check failed: {error}"),
            Self::Pending(action) => format!("{action} pending"),
        }
    }

    pub(super) fn source_label(&self) -> &'static str {
        match self {
            Self::EnvPresent => "env-only",
            Self::EnvMissingKeychainNotChecked => "not checked",
            Self::KeychainPresentChecked => "manual keychain check",
            Self::KeychainMissingChecked => "manual keychain check",
            Self::CheckFailed(_) => "manual keychain check",
            Self::Pending(_) => "pending",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ProviderAuthRow {
    pub(super) provider: Provider,
    pub(super) state: ProviderAuthDisplayState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum InputMode {
    Normal,
    EditingHome {
        value: String,
        cursor: usize,
    },
    EditingDaemonHost {
        value: String,
        cursor: usize,
    },
    EditingDaemonPort {
        value: String,
        cursor: usize,
    },
    EditingDaemonToken {
        value: String,
        cursor: usize,
    },
    EditingFilter {
        kind: NavigatorListKind,
        value: String,
        cursor: usize,
    },
    EditingResourceFilter {
        value: String,
        cursor: usize,
    },
    EditingProviderKey {
        provider: Provider,
        value: String,
    },
    ConfirmRemove {
        provider: Provider,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct InputLine {
    pub(super) label: String,
    pub(super) value: String,
    pub(super) cursor: usize,
    pub(super) masked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TuiDaemonToken {
    pub(super) token: Option<String>,
    pub(super) source: TuiTokenSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DaemonActionState {
    Idle,
    Starting {
        request_id: u64,
        phase: StartPhase,
        warning: Option<String>,
    },
    StartFailed {
        message: String,
        stdout_log: Option<PathBuf>,
        stderr_log: Option<PathBuf>,
    },
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StartPhase {
    ResolvingHome,
    SpawningDetachedDaemon,
    PollingHealthz,
}

#[derive(Debug, Clone)]
struct ProviderActionResult {
    state: ProviderAuthDisplayState,
    message: String,
}

enum ProviderActionRequest {
    Check,
    Set(String),
    Remove,
}

pub(super) struct TuiApp {
    pub(super) home: PathBuf,
    pub(super) config_path: PathBuf,
    pub(super) config: TentgentConfig,
    pub(super) config_error: Option<String>,
    pub(super) inspection: DaemonInspection,
    pub(super) daemon_url: DaemonUrlResolution,
    pub(super) daemon_token: TuiDaemonToken,
    pub(super) daemon: DaemonSnapshot,
    pub(super) auth_rows: Vec<ProviderAuthRow>,
    pub(super) navigator: NavigatorState,
    pub(super) resources: ResourceState,
    pub(super) chat: ChatState,
    pub(super) dashboard: DashboardState,
    pub(super) mode: AppMode,
    pub(super) focus: FocusPane,
    pub(super) selected_menu: usize,
    pub(super) selected_provider: usize,
    pub(super) selected_setting: usize,
    pub(super) input_mode: InputMode,
    pub(super) daemon_action: DaemonActionState,
    pub(super) message: String,
    pub(super) should_quit: bool,
    pub(super) refresh_in_flight: Option<u64>,
    pub(super) start_in_flight: Option<u64>,
    pub(super) provider_action_in_flight: Option<(u64, Provider)>,
    pub(super) navigator_in_flight: Option<(u64, NavigatorListKind)>,
    pub(super) detail_in_flight: Option<(u64, NavigatorListKind, String)>,
    pub(super) tail_in_flight: Option<(u64, NavigatorListKind)>,
    pub(super) resource_in_flight: Option<u64>,
    pub(super) chat_in_flight: Option<u64>,
    chat_task: Option<JoinHandle<()>>,
    generation: u64,
    request_counter: u64,
    flag_daemon_url: Option<String>,
    flag_token: Option<String>,
    session_token: Option<String>,
}

impl TuiApp {
    pub(super) fn new(command: TuiCommand) -> miette::Result<Self> {
        let manager = DaemonManager::new(command.home.as_deref()).into_diagnostic()?;
        let inspection = manager.status().into_diagnostic()?;
        let home = inspection.home_dir.clone();
        let config_path = config_path(&home);
        let (config, config_error) = load_config_with_error(&home);
        let daemon_url = resolve_daemon_url(DaemonUrlInputs {
            flag_url: command.daemon_url.as_deref(),
            env_url: env_string(DAEMON_URL_ENV_VAR).as_deref(),
            config_url: config.daemon.url.as_deref(),
            metadata: inspection.process.as_ref(),
        });
        let config_error = config_error.or_else(|| daemon_url.config_error.clone());
        let daemon_token = resolve_tui_daemon_token(
            command.token.as_deref(),
            env_string(DAEMON_TOKEN_ENV_VAR).as_deref(),
            None,
        );
        let mut app = Self {
            home,
            config_path,
            config,
            config_error,
            inspection,
            daemon_url,
            daemon_token,
            daemon: DaemonSnapshot::idle(),
            auth_rows: provider_env_rows(),
            navigator: NavigatorState::default(),
            resources: ResourceState::default(),
            chat: ChatState::default(),
            dashboard: DashboardState::default(),
            mode: AppMode::Bootstrap(BootstrapReason::DaemonDown),
            focus: FocusPane::Menu,
            selected_menu: 0,
            selected_provider: 0,
            selected_setting: 0,
            input_mode: InputMode::Normal,
            daemon_action: DaemonActionState::Idle,
            message: "checking daemon; no Keychain reads on automatic refresh".to_string(),
            should_quit: false,
            refresh_in_flight: None,
            start_in_flight: None,
            provider_action_in_flight: None,
            navigator_in_flight: None,
            detail_in_flight: None,
            tail_in_flight: None,
            resource_in_flight: None,
            chat_in_flight: None,
            chat_task: None,
            generation: 0,
            request_counter: 0,
            flag_daemon_url: command.daemon_url,
            flag_token: command.token,
            session_token: None,
        };
        app.update_mode();
        Ok(app)
    }

    pub(super) fn menu_entries(&self) -> Vec<MenuEntry> {
        match self.mode {
            AppMode::Bootstrap(reason) => {
                let can_start = !matches!(
                    reason,
                    BootstrapReason::AuthRequired | BootstrapReason::ConfigError
                );
                vec![
                    MenuEntry {
                        item: MenuItem::StartDaemon,
                        label: "Start daemon",
                        detail: "explicit detached local start",
                        enabled: can_start,
                    },
                    MenuEntry {
                        item: MenuItem::Settings,
                        label: "Settings",
                        detail: "home, daemon URL, host, port, token",
                        enabled: true,
                    },
                    MenuEntry {
                        item: MenuItem::ProviderAuth,
                        label: "Provider auth",
                        detail: "explicit provider-scoped setup",
                        enabled: true,
                    },
                ]
            }
            AppMode::Operator => vec![
                MenuEntry {
                    item: MenuItem::Dashboard,
                    label: "Dashboard",
                    detail: "daemon monitoring summary",
                    enabled: true,
                },
                MenuEntry {
                    item: MenuItem::Chat,
                    label: "Chat",
                    detail: "session workspace",
                    enabled: true,
                },
                MenuEntry {
                    item: MenuItem::Resources,
                    label: "Resources",
                    detail: "local disk and process monitor",
                    enabled: true,
                },
                MenuEntry {
                    item: MenuItem::Settings,
                    label: "Settings",
                    detail: "home, daemon URL, host, port, token",
                    enabled: true,
                },
                MenuEntry {
                    item: MenuItem::ProviderAuth,
                    label: "Provider auth",
                    detail: "explicit provider-scoped setup",
                    enabled: true,
                },
                MenuEntry {
                    item: MenuItem::Models,
                    label: "Models",
                    detail: "read-only navigator",
                    enabled: true,
                },
                MenuEntry {
                    item: MenuItem::Adapters,
                    label: "Adapters",
                    detail: "read-only navigator",
                    enabled: true,
                },
                MenuEntry {
                    item: MenuItem::Datasets,
                    label: "Datasets",
                    detail: "read-only navigator",
                    enabled: true,
                },
                MenuEntry {
                    item: MenuItem::Servers,
                    label: "Servers",
                    detail: "read-only navigator",
                    enabled: true,
                },
                MenuEntry {
                    item: MenuItem::Sessions,
                    label: "Sessions",
                    detail: "read-only navigator",
                    enabled: true,
                },
                MenuEntry {
                    item: MenuItem::Training,
                    label: "Training",
                    detail: "plans / runs",
                    enabled: true,
                },
            ],
        }
    }

    pub(super) fn selected_menu_entry(&self) -> MenuEntry {
        let entries = self.menu_entries();
        entries
            .get(self.selected_menu.min(entries.len().saturating_sub(1)))
            .cloned()
            .expect("TUI always has menu entries")
    }

    pub(super) fn selected_provider(&self) -> Provider {
        Provider::ALL[self.selected_provider.min(Provider::ALL.len() - 1)]
    }

    pub(super) fn settings_entries(&self) -> Vec<SettingsEntry> {
        vec![
            SettingsEntry {
                item: SettingsItem::RuntimeHome,
                label: "Home",
                value: self.home.display().to_string(),
                detail: "current TUI session",
                editable: true,
            },
            SettingsEntry {
                item: SettingsItem::DaemonHost,
                label: "Daemon host",
                value: self.daemon_host_label(),
                detail: "saved via daemon.url",
                editable: true,
            },
            SettingsEntry {
                item: SettingsItem::DaemonPort,
                label: "Daemon port",
                value: self.daemon_port_label(),
                detail: "saved via daemon.url",
                editable: true,
            },
            SettingsEntry {
                item: SettingsItem::DaemonToken,
                label: "Daemon token",
                value: token_source_setting_label(self.daemon_token.source),
                detail: "current TUI session only",
                editable: true,
            },
            SettingsEntry {
                item: SettingsItem::ConfigPath,
                label: "Config",
                value: self.config_path.display().to_string(),
                detail: "read-only path",
                editable: false,
            },
            SettingsEntry {
                item: SettingsItem::RuntimeDir,
                label: "Runtime dir",
                value: self.inspection.runtime_dir.display().to_string(),
                detail: "read-only path",
                editable: false,
            },
            SettingsEntry {
                item: SettingsItem::LogDir,
                label: "Log dir",
                value: self.inspection.log_dir.display().to_string(),
                detail: "read-only path",
                editable: false,
            },
        ]
    }

    pub(super) fn selected_setting_entry(&self) -> SettingsEntry {
        let entries = self.settings_entries();
        entries
            .get(self.selected_setting.min(entries.len().saturating_sub(1)))
            .cloned()
            .expect("TUI always has settings entries")
    }

    pub(super) fn active_input_line(&self) -> Option<InputLine> {
        match &self.input_mode {
            InputMode::Normal => None,
            InputMode::EditingHome { value, cursor } => {
                Some(input_line("home", value, *cursor, false))
            }
            InputMode::EditingDaemonHost { value, cursor } => {
                Some(input_line("daemon host", value, *cursor, false))
            }
            InputMode::EditingDaemonPort { value, cursor } => {
                Some(input_line("daemon port", value, *cursor, false))
            }
            InputMode::EditingDaemonToken { value, cursor } => {
                Some(input_line("daemon token", value, *cursor, true))
            }
            InputMode::EditingFilter { value, cursor, .. } => {
                Some(input_line("filter", value, *cursor, false))
            }
            InputMode::EditingResourceFilter { value, cursor } => {
                Some(input_line("resource filter", value, *cursor, false))
            }
            InputMode::EditingProviderKey { provider, value } => Some(format!(
                "{} key: {}",
                provider.display_name(),
                mask_secret(value)
            ))
            .map(|value| InputLine {
                label: value,
                value: String::new(),
                cursor: 0,
                masked: false,
            }),
            InputMode::ConfirmRemove { provider } => Some(format!(
                "remove {} keychain entry? y/N",
                provider.display_name()
            ))
            .map(|value| InputLine {
                label: value,
                value: String::new(),
                cursor: 0,
                masked: false,
            }),
        }
    }

    pub(super) fn can_start_daemon(&self) -> bool {
        matches!(
            self.mode,
            AppMode::Bootstrap(
                BootstrapReason::DaemonDown
                    | BootstrapReason::DaemonTimeout
                    | BootstrapReason::ProtocolError
            )
        ) && !matches!(self.daemon_action, DaemonActionState::Starting { .. })
    }

    #[cfg(test)]
    pub(super) fn detached_start_options(&self) -> miette::Result<DetachedDaemonOptions> {
        let bind = daemon_start_bind_from_url(&self.daemon_url.url)
            .map_err(|error| miette::miette!("{error}"))?;
        Ok(DetachedDaemonOptions {
            home: Some(self.home.clone()),
            host: Some(bind.host),
            port: Some(bind.port),
            allow_unsafe_bind: false,
        })
    }

    pub(super) fn start_command(&self) -> String {
        match daemon_start_bind_from_url(&self.daemon_url.url) {
            Ok(bind) => format!(
                "tentgent daemon start --home {} --host {} --port {}",
                shell_single_quote(&self.home),
                bind.host,
                bind.port
            ),
            Err(_) => format!(
                "tentgent daemon start --home {} --host 127.0.0.1 --port {}",
                shell_single_quote(&self.home),
                DEFAULT_DAEMON_PORT
            ),
        }
    }

    pub(super) fn start_target_warning(&self) -> Option<String> {
        daemon_start_bind_from_url(&self.daemon_url.url)
            .ok()
            .and_then(|bind| bind.warning)
    }

    pub(super) fn daemon_host_label(&self) -> String {
        daemon_url_host(&self.daemon_url.url).unwrap_or_else(|| "(invalid)".to_string())
    }

    pub(super) fn daemon_port_label(&self) -> String {
        daemon_url_port(&self.daemon_url.url)
            .map(|port| port.to_string())
            .unwrap_or_else(|| format!("(implicit {DEFAULT_DAEMON_PORT})"))
    }

    pub(super) fn current_navigator_kind(&self) -> Option<NavigatorListKind> {
        match self.selected_menu_entry().item {
            MenuItem::Models => Some(NavigatorListKind::Models),
            MenuItem::Adapters => Some(NavigatorListKind::Adapters),
            MenuItem::Datasets => Some(NavigatorListKind::Datasets),
            MenuItem::Servers => Some(NavigatorListKind::Servers),
            MenuItem::Sessions => Some(NavigatorListKind::Sessions),
            MenuItem::Training => Some(self.navigator.training_tab.list_kind()),
            _ => None,
        }
    }

    fn ensure_current_navigator_loaded(&mut self, tx: &TuiEventSender) {
        let Some(kind) = self.current_navigator_kind() else {
            return;
        };
        if matches!(
            self.navigator.state(kind).load_state,
            NavigatorLoadState::Idle
        ) {
            self.request_navigator_list(kind, tx, format!("loading {}", kind.label()));
        }
    }

    fn refresh_current_view(&mut self, tx: &TuiEventSender) {
        if self.selected_menu_entry().item == MenuItem::Chat {
            self.refresh_chat_view(tx);
        } else if self.selected_menu_entry().item == MenuItem::Resources {
            self.request_resource_snapshot(tx, "scanning local resources");
        } else if let Some(kind) = self.current_navigator_kind() {
            if self.refresh_active_tail(kind, tx) {
                return;
            }
            self.request_navigator_list(kind, tx, format!("refreshing {}", kind.label()));
        } else {
            self.request_refresh(tx, "refreshing daemon status");
        }
    }

    fn ensure_resources_loaded(&mut self, tx: &TuiEventSender) {
        if matches!(self.resources.load_state, ResourceLoadState::Idle)
            && self.resources.snapshot.is_none()
        {
            self.request_resource_snapshot(tx, "scanning local resources");
        }
    }

    fn request_resource_snapshot(&mut self, tx: &TuiEventSender, message: impl Into<String>) {
        if self.resource_in_flight.is_some() {
            return;
        }
        let request_id = self.next_request_id();
        let generation = self.generation;
        self.resource_in_flight = Some(request_id);
        self.resources.load_state = ResourceLoadState::Loading { request_id };
        self.message = message.into();
        let inputs =
            ResourceInputs::from_state(self.home.clone(), self.inspection.clone(), &self.navigator);
        let tx = tx.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || collect_resource_snapshot(inputs))
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(TuiEvent::ResourceFinished {
                request_id,
                generation,
                result,
            });
        });
    }

    fn request_navigator_list(
        &mut self,
        kind: NavigatorListKind,
        tx: &TuiEventSender,
        message: impl Into<String>,
    ) {
        if self.navigator_in_flight.is_some() {
            return;
        }
        let request_id = self.next_request_id();
        let generation = self.generation;
        self.navigator_in_flight = Some((request_id, kind));
        self.navigator.state_mut(kind).load_state = NavigatorLoadState::Loading { request_id };
        self.message = message.into();
        let client = match self.daemon_client() {
            Ok(client) => client,
            Err(error) => {
                self.navigator_in_flight = None;
                self.navigator.state_mut(kind).load_state = NavigatorLoadState::Error {
                    message: error.to_string(),
                    stale: !self.navigator.state(kind).rows.is_empty(),
                };
                return;
            }
        };
        let tx = tx.clone();
        tokio::spawn(async move {
            let result = client.list_navigator(kind).await;
            let _ = tx.send(TuiEvent::NavigatorListFinished {
                request_id,
                generation,
                kind,
                result,
            });
        });
    }

    fn request_selected_detail(&mut self, kind: NavigatorListKind, tx: &TuiEventSender) {
        if self.detail_in_flight.is_some() {
            return;
        }
        let Some(item_ref) = self
            .navigator
            .state(kind)
            .selected_row()
            .map(|row| row.item_ref.clone())
        else {
            self.message = format!("{} has no selected row", kind.label());
            return;
        };
        let request_id = self.next_request_id();
        let generation = self.generation;
        self.detail_in_flight = Some((request_id, kind, item_ref.clone()));
        self.message = format!("inspecting {item_ref}");
        let client = match self.daemon_client() {
            Ok(client) => client,
            Err(error) => {
                self.detail_in_flight = None;
                self.navigator.state_mut(kind).load_state = NavigatorLoadState::Error {
                    message: error.to_string(),
                    stale: true,
                };
                return;
            }
        };
        let tx = tx.clone();
        tokio::spawn(async move {
            let result = client.inspect_navigator(kind, &item_ref).await;
            let _ = tx.send(TuiEvent::NavigatorDetailFinished {
                request_id,
                generation,
                kind,
                item_ref,
                result,
            });
        });
    }

    fn request_current_tail(&mut self, tx: &TuiEventSender) {
        let Some(kind) = self.current_navigator_kind() else {
            return;
        };
        let source = match kind {
            NavigatorListKind::Servers => {
                let state = self.navigator.state(kind);
                let Some(row) = state.selected_row() else {
                    self.message = "select a server before opening logs".to_string();
                    return;
                };
                TailSource::ServerLog {
                    server_ref: row.item_ref.clone(),
                    kind: state.server_log_kind,
                    tail_bytes: LOG_TAIL_BYTES,
                }
            }
            NavigatorListKind::TrainRuns => {
                let Some(row) = self.navigator.state(kind).selected_row() else {
                    self.message = "select a train run before opening logs".to_string();
                    return;
                };
                TailSource::TrainRunRawLog {
                    run_ref: row.item_ref.clone(),
                    tail_bytes: LOG_TAIL_BYTES,
                }
            }
            NavigatorListKind::Sessions => {
                self.request_session_messages(tx);
                return;
            }
            _ => {
                self.message =
                    "logs are available for servers and train runs in Slice 2".to_string();
                return;
            }
        };
        self.request_tail(kind, source, tx);
    }

    fn request_session_messages(&mut self, tx: &TuiEventSender) {
        let Some(kind) = self.current_navigator_kind() else {
            return;
        };
        if kind != NavigatorListKind::Sessions {
            return;
        }
        let Some(row) = self.navigator.state(kind).selected_row() else {
            self.message = "select a session before opening messages".to_string();
            return;
        };
        self.request_tail(
            kind,
            TailSource::SessionMessages {
                session_ref: row.item_ref.clone(),
                tail: SESSION_MESSAGES_TAIL,
            },
            tx,
        );
    }

    fn request_train_metrics(&mut self, tx: &TuiEventSender) {
        let kind = NavigatorListKind::TrainRuns;
        if self.current_navigator_kind() != Some(kind) {
            self.message = "metrics are available on Training Runs".to_string();
            return;
        }
        let Some(row) = self.navigator.state(kind).selected_row() else {
            self.message = "select a train run before opening metrics".to_string();
            return;
        };
        self.request_tail(
            kind,
            TailSource::TrainRunMetrics {
                run_ref: row.item_ref.clone(),
                tail: TRAIN_METRICS_TAIL,
            },
            tx,
        );
    }

    fn request_tail(&mut self, kind: NavigatorListKind, source: TailSource, tx: &TuiEventSender) {
        if self.tail_in_flight.is_some() {
            return;
        }
        let request_id = self.next_request_id();
        let generation = self.generation;
        self.tail_in_flight = Some((request_id, kind));
        self.message = format!("loading {}", source.title());
        let client = match self.daemon_client() {
            Ok(client) => client,
            Err(error) => {
                self.tail_in_flight = None;
                self.navigator.state_mut(kind).load_state = NavigatorLoadState::Error {
                    message: error.to_string(),
                    stale: true,
                };
                return;
            }
        };
        let source_for_task = source.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let result = client.fetch_tail(source_for_task).await;
            let _ = tx.send(TuiEvent::NavigatorTailFinished {
                request_id,
                generation,
                kind,
                source,
                result,
            });
        });
    }

    fn refresh_active_tail(&mut self, kind: NavigatorListKind, tx: &TuiEventSender) -> bool {
        let Some(source) = self
            .navigator
            .state(kind)
            .active_tail
            .as_ref()
            .map(|tail| tail.source.clone())
        else {
            return false;
        };
        self.request_tail(kind, source, tx);
        true
    }

    fn close_tail_view(&mut self) -> bool {
        let Some(kind) = self.current_navigator_kind() else {
            return false;
        };
        let state = self.navigator.state_mut(kind);
        if state.active_tail.is_some() {
            state.active_tail = None;
            true
        } else {
            false
        }
    }

    fn toggle_training_tab(&mut self, tx: &TuiEventSender) {
        self.navigator.training_tab.toggle();
        let kind = self.navigator.training_tab.list_kind();
        self.message = format!("Training tab: {}", self.navigator.training_tab.label());
        self.request_navigator_list(kind, tx, format!("loading {}", kind.label()));
    }

    fn toggle_server_log_kind(&mut self) {
        if self.current_navigator_kind() != Some(NavigatorListKind::Servers) {
            return;
        }
        let state = self.navigator.state_mut(NavigatorListKind::Servers);
        state.server_log_kind.toggle();
        state.active_tail = None;
        self.message = format!("server log source: {}", state.server_log_kind.label());
    }

    fn begin_filter_edit(&mut self) {
        if self.selected_menu_entry().item == MenuItem::Resources {
            self.focus = FocusPane::Detail;
            let value = self.resources.filter.clone();
            let cursor = value.chars().count();
            self.input_mode = InputMode::EditingResourceFilter { value, cursor };
            self.message = "enter applies local resource filter; esc cancels".to_string();
            return;
        }
        let Some(kind) = self.current_navigator_kind() else {
            self.message = "filter is available inside navigator screens".to_string();
            return;
        };
        self.focus = FocusPane::Detail;
        let value = self.navigator.state(kind).filter.clone();
        let cursor = value.chars().count();
        self.input_mode = InputMode::EditingFilter {
            kind,
            value,
            cursor,
        };
        self.message = "enter applies local filter; esc cancels".to_string();
    }

    fn save_filter(&mut self, kind: NavigatorListKind, value: String) {
        self.bump_generation();
        self.navigator
            .state_mut(kind)
            .set_filter(value.trim().to_string());
        self.message = format!("filtered {}", kind.label());
    }

    fn save_resource_filter(&mut self, value: String) {
        self.bump_generation();
        self.resources.set_filter(value.trim().to_string());
        self.message = "filtered Resources locally".to_string();
    }

    fn daemon_client(&self) -> miette::Result<DaemonClient> {
        DaemonClient::new(
            self.daemon_url.url.clone(),
            self.daemon_token.token.clone(),
            self.daemon_token.source,
        )
    }

    fn chat_client(&self) -> miette::Result<ChatClient> {
        ChatClient::new(
            self.daemon_url.url.clone(),
            self.daemon_token.token.clone(),
            self.daemon_token.source,
        )
    }

    fn ensure_chat_loaded(&mut self, tx: &TuiEventSender) {
        if matches!(self.chat.load_state, ChatLoadState::Idle) && self.chat.servers.is_empty() {
            self.request_chat_overview(tx, "loading chat workspace");
        }
    }

    fn refresh_chat_view(&mut self, tx: &TuiEventSender) {
        if self.chat.phase == ChatPhase::Workspace {
            if let Some(session_ref) = self.chat.selected_session_ref.clone() {
                self.request_chat_messages(session_ref, tx, "refreshing chat messages");
                return;
            }
        }
        self.request_chat_overview(tx, "refreshing chat choices");
    }

    fn request_chat_overview(&mut self, tx: &TuiEventSender, message: impl Into<String>) {
        if self.chat_in_flight.is_some() {
            return;
        }
        let request_id = self.next_request_id();
        let generation = self.generation;
        self.chat_in_flight = Some(request_id);
        self.chat.load_state = ChatLoadState::Loading { request_id };
        self.message = message.into();
        let client = match self.chat_client() {
            Ok(client) => client,
            Err(error) => {
                self.chat_in_flight = None;
                self.chat.load_state = ChatLoadState::Error {
                    message: error.to_string(),
                    stale: !self.chat.servers.is_empty() || !self.chat.sessions.is_empty(),
                };
                return;
            }
        };
        let tx = tx.clone();
        tokio::spawn(async move {
            let result = client.overview().await;
            let _ = tx.send(TuiEvent::ChatOverviewFinished {
                request_id,
                generation,
                result,
            });
        });
    }

    fn request_chat_messages(
        &mut self,
        session_ref: String,
        tx: &TuiEventSender,
        message: impl Into<String>,
    ) {
        if self.chat_in_flight.is_some() {
            return;
        }
        let request_id = self.next_request_id();
        let generation = self.generation;
        self.chat_in_flight = Some(request_id);
        self.chat.load_state = ChatLoadState::Loading { request_id };
        self.message = message.into();
        let client = match self.chat_client() {
            Ok(client) => client,
            Err(error) => {
                self.chat_in_flight = None;
                self.chat.load_state = ChatLoadState::Error {
                    message: error.to_string(),
                    stale: !self.chat.transcript.is_empty(),
                };
                return;
            }
        };
        let session_ref_for_task = session_ref.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let result = match client.inspect_session(&session_ref_for_task).await {
                Ok(_) => client.session_messages(&session_ref_for_task).await,
                Err(error) => Err(error),
            };
            let _ = tx.send(TuiEvent::ChatMessagesFinished {
                request_id,
                generation,
                session_ref,
                result,
            });
        });
    }

    fn begin_chat_create_session(&mut self, tx: &TuiEventSender) {
        if self.chat_in_flight.is_some() || self.chat.send_state.is_in_flight() {
            self.message = "chat action already in progress".to_string();
            return;
        }
        let Some(server_ref) = self.chat.selected_server_ref.clone() else {
            self.chat.phase = ChatPhase::ChooseServer;
            self.message = "choose a running server before creating a session".to_string();
            return;
        };
        let request_id = self.next_request_id();
        let generation = self.generation;
        let title = format!("TUI chat {}", timestamp_label());
        let adapter_ref = self.chat.selected_adapter_ref.clone();
        self.chat_in_flight = Some(request_id);
        self.chat.send_state = ChatSendState::CreatingSession { request_id };
        self.message = "creating chat session".to_string();
        let client = match self.chat_client() {
            Ok(client) => client,
            Err(error) => {
                self.chat_in_flight = None;
                self.chat.send_state = ChatSendState::Error;
                self.chat.last_error = Some(error.to_string());
                return;
            }
        };
        let tx = tx.clone();
        tokio::spawn(async move {
            let result = client.create_session(title, server_ref, adapter_ref).await;
            let _ = tx.send(TuiEvent::ChatSessionCreated {
                request_id,
                generation,
                result,
            });
        });
    }

    fn begin_chat_send(&mut self, tx: &TuiEventSender, stream: bool) {
        if self.chat_in_flight.is_some() || self.chat.send_state.is_in_flight() {
            self.message = "chat send already in progress".to_string();
            return;
        }
        let request_id = self.next_request_id();
        let Some(request) = self.chat_send_request(request_id, stream) else {
            return;
        };
        let generation = self.generation;
        self.chat_in_flight = Some(request_id);
        if stream {
            self.chat
                .start_pending_send(request_id, request.prompt.clone());
            self.chat.composer.clear();
            self.chat.composer_cursor = 0;
            self.message = "streaming chat turn".to_string();
        } else {
            self.chat.pending_user = Some(request.prompt.clone());
            self.chat.pending_assistant = Some(String::new());
            self.chat.pending_interrupted = false;
            self.chat.send_state = ChatSendState::Sending { request_id };
            self.chat.retry_non_stream = None;
            self.message = "sending non-stream retry".to_string();
        }
        let client = match self.chat_client() {
            Ok(client) => client,
            Err(error) => {
                self.chat_in_flight = None;
                self.chat.send_state = ChatSendState::Error;
                self.chat.last_error = Some(error.to_string());
                return;
            }
        };
        let tx = tx.clone();
        if stream {
            let request_for_retry = request.clone();
            let handle = tokio::spawn(async move {
                run_chat_stream_task(
                    client,
                    request,
                    request_for_retry,
                    request_id,
                    generation,
                    tx,
                )
                .await;
            });
            self.chat_task = Some(handle);
        } else {
            let handle = tokio::spawn(async move {
                let result = client.post_non_stream(&request).await;
                let _ = tx.send(TuiEvent::ChatNonStreamFinished {
                    request_id,
                    generation,
                    result,
                });
            });
            self.chat_task = Some(handle);
        }
    }

    fn chat_send_request(&mut self, request_id: u64, stream: bool) -> Option<ChatSendRequest> {
        if self.chat.phase != ChatPhase::Workspace {
            self.message = "choose a running server and session before sending".to_string();
            return None;
        }
        let Some(server_ref) = self.chat.selected_server_ref.clone() else {
            self.chat.phase = ChatPhase::ChooseServer;
            self.message = "choose a running server before sending".to_string();
            return None;
        };
        let Some(session_ref) = self.chat.selected_session_ref.clone() else {
            self.chat.phase = ChatPhase::ChooseSession;
            self.message = "choose or create a session before sending".to_string();
            return None;
        };
        let prompt = self.chat.composer.trim().to_string();
        if prompt.is_empty() {
            self.message = "composer is empty".to_string();
            return None;
        }
        let context_mode = self.chat.context_mode;
        Some(ChatSendRequest {
            request_id,
            server_ref,
            session_ref,
            adapter_ref: self.chat.selected_adapter_ref.clone(),
            prompt,
            context_mode,
            max_session_messages: context_mode.max_session_messages(),
            stream,
        })
    }

    fn begin_chat_retry_non_stream(&mut self, tx: &TuiEventSender) {
        let Some(request) = self.chat.retry_non_stream.clone() else {
            self.message = "no non-stream retry is available".to_string();
            return;
        };
        self.chat.composer = request.prompt.clone();
        self.chat.composer_cursor = self.chat.composer.chars().count();
        let request_id = self.next_request_id();
        self.begin_chat_send_with_request(tx, request.with_request_id(request_id).non_stream());
    }

    fn begin_chat_send_with_request(&mut self, tx: &TuiEventSender, request: ChatSendRequest) {
        if self.chat_in_flight.is_some() || self.chat.send_state.is_in_flight() {
            self.message = "chat send already in progress".to_string();
            return;
        }
        let request_id = request.request_id;
        let generation = self.generation;
        self.chat_in_flight = Some(request_id);
        self.chat.pending_user = Some(request.prompt.clone());
        self.chat.pending_assistant = Some(String::new());
        self.chat.pending_interrupted = false;
        self.chat.send_state = ChatSendState::Sending { request_id };
        self.chat.retry_non_stream = None;
        self.chat.composer.clear();
        self.chat.composer_cursor = 0;
        self.message = "sending non-stream retry".to_string();
        let client = match self.chat_client() {
            Ok(client) => client,
            Err(error) => {
                self.chat_in_flight = None;
                self.chat.send_state = ChatSendState::Error;
                self.chat.last_error = Some(error.to_string());
                return;
            }
        };
        let tx = tx.clone();
        self.chat_task = Some(tokio::spawn(async move {
            let result = client.post_non_stream(&request).await;
            let _ = tx.send(TuiEvent::ChatNonStreamFinished {
                request_id,
                generation,
                result,
            });
        }));
    }

    fn cancel_chat_task(&mut self, tx: &TuiEventSender) {
        if !self.chat.send_state.is_in_flight() {
            return;
        }
        if let Some(handle) = self.chat_task.take() {
            handle.abort();
        }
        self.bump_generation();
        self.chat.send_state = ChatSendState::Idle;
        self.chat.pending_interrupted = true;
        self.chat.last_error =
            Some("chat request canceled; refreshing daemon transcript".to_string());
        self.message = "chat request canceled".to_string();
        if let Some(session_ref) = self.chat.selected_session_ref.clone() {
            self.request_chat_messages(session_ref, tx, "refreshing after cancel");
        }
    }

    fn request_refresh(&mut self, tx: &TuiEventSender, message: impl Into<String>) {
        if self.refresh_in_flight.is_some() {
            return;
        }
        let request_id = self.next_request_id();
        let generation = self.generation;
        self.refresh_in_flight = Some(request_id);
        self.message = message.into();
        let inputs = RefreshInputs {
            home: self.home.clone(),
            flag_daemon_url: self.flag_daemon_url.clone(),
            flag_token: self.flag_token.clone(),
            session_token: self.session_token.clone(),
        };
        let tx = tx.clone();
        tokio::spawn(async move {
            let result = collect_refresh(inputs).await;
            let _ = tx.send(TuiEvent::RefreshFinished {
                request_id,
                generation,
                result,
            });
        });
    }

    fn handle_event(&mut self, event: Event, tx: &TuiEventSender) -> miette::Result<()> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key, tx)?,
            Event::Resize(_, _) => {
                self.message = "resized; layout recalculated".to_string();
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_tui_event(&mut self, event: TuiEvent, tx: &TuiEventSender) -> miette::Result<()> {
        match event {
            TuiEvent::RefreshFinished {
                request_id,
                generation,
                result,
            } => {
                if self.refresh_in_flight != Some(request_id) || self.generation != generation {
                    return Ok(());
                }
                self.refresh_in_flight = None;
                match result {
                    Ok(data) => self.apply_refresh(data),
                    Err(error) => {
                        self.daemon = DaemonSnapshot::down(error.clone());
                        self.message = format!("refresh failed: {error}");
                        self.update_mode();
                    }
                }
            }
            TuiEvent::StartFinished {
                request_id,
                generation,
                result,
            } => {
                if self.start_in_flight != Some(request_id) || self.generation != generation {
                    return Ok(());
                }
                self.start_in_flight = None;
                match result {
                    Ok(outcome) => {
                        self.inspection = outcome.inspection.clone();
                        self.daemon_action = DaemonActionState::Ready;
                        self.daemon = DaemonSnapshot {
                            state: DaemonConnectionState::Ready,
                            detail: "daemon health is ready; refreshing status".to_string(),
                            status: None,
                            doctor: None,
                        };
                        self.mode = AppMode::Operator;
                        self.selected_menu = 0;
                        let base_message = if outcome.already_running {
                            format!("daemon already running at {}", outcome.daemon_url)
                        } else {
                            format!("daemon started at {}", outcome.daemon_url)
                        };
                        self.message = outcome
                            .status_warning
                            .map(|warning| format!("{base_message}; {warning}"))
                            .unwrap_or(base_message);
                        self.request_refresh(tx, "refreshing operator dashboard");
                    }
                    Err(error) => {
                        self.daemon_action = DaemonActionState::StartFailed {
                            message: error.clone(),
                            stdout_log: Some(self.inspection.stdout_log_path.clone()),
                            stderr_log: Some(self.inspection.stderr_log_path.clone()),
                        };
                        self.message = format!("daemon start failed: {error}");
                        self.update_mode();
                    }
                }
            }
            TuiEvent::ProviderActionFinished {
                request_id,
                provider,
                result,
            } => {
                if self.provider_action_in_flight != Some((request_id, provider)) {
                    return Ok(());
                }
                self.provider_action_in_flight = None;
                self.set_provider_state(provider, result.state);
                self.message = result.message;
            }
            TuiEvent::NavigatorListFinished {
                request_id,
                generation,
                kind,
                result,
            } => {
                if self.navigator_in_flight != Some((request_id, kind))
                    || self.generation != generation
                {
                    return Ok(());
                }
                self.navigator_in_flight = None;
                self.apply_navigator_list_result(kind, result);
            }
            TuiEvent::NavigatorDetailFinished {
                request_id,
                generation,
                kind,
                item_ref,
                result,
            } => {
                if self.detail_in_flight != Some((request_id, kind, item_ref.clone()))
                    || self.generation != generation
                {
                    return Ok(());
                }
                self.detail_in_flight = None;
                self.apply_navigator_detail_result(kind, item_ref, result);
            }
            TuiEvent::NavigatorTailFinished {
                request_id,
                generation,
                kind,
                source,
                result,
            } => {
                if self.tail_in_flight != Some((request_id, kind)) || self.generation != generation
                {
                    return Ok(());
                }
                self.tail_in_flight = None;
                self.apply_tail_result(kind, source, result);
            }
            TuiEvent::ResourceFinished {
                request_id,
                generation,
                result,
            } => {
                if self.resource_in_flight != Some(request_id) || self.generation != generation {
                    return Ok(());
                }
                self.resource_in_flight = None;
                match result {
                    Ok(snapshot) => {
                        let warning_count = snapshot.warnings.len();
                        self.resources.snapshot = Some(snapshot);
                        self.resources.load_state = ResourceLoadState::Ready;
                        self.message = format!("resource scan complete; {warning_count} warnings");
                    }
                    Err(error) => {
                        self.resources.load_state = ResourceLoadState::Error {
                            message: error.clone(),
                            stale: self.resources.snapshot.is_some(),
                        };
                        self.message = format!("resource scan failed: {error}");
                    }
                }
            }
            TuiEvent::ChatOverviewFinished {
                request_id,
                generation,
                result,
            } => {
                if self.chat_in_flight != Some(request_id) || self.generation != generation {
                    return Ok(());
                }
                self.chat_in_flight = None;
                match result {
                    Ok(overview) => {
                        let adapters = adapter_rows_from_navigator(&self.navigator.adapters.rows);
                        let running = overview.servers.len();
                        self.chat
                            .apply_overview(overview.servers, overview.sessions, adapters);
                        self.message = if running == 0 {
                            "no running server; start one from CLI or later server actions"
                                .to_string()
                        } else {
                            format!("loaded chat workspace; {running} running servers")
                        };
                    }
                    Err(error) => self.apply_chat_error(error, tx, false),
                }
            }
            TuiEvent::ChatSessionCreated {
                request_id,
                generation,
                result,
            } => {
                if self.chat_in_flight != Some(request_id) || self.generation != generation {
                    return Ok(());
                }
                self.chat_in_flight = None;
                match result {
                    Ok(session) => {
                        self.chat.selected_session_ref = Some(session.session_ref.clone());
                        self.chat.sessions.insert(0, session);
                        self.chat.selected_session = 1;
                        self.chat.send_state = ChatSendState::Idle;
                        self.chat.recompute_phase();
                        self.message = "created chat session".to_string();
                        if let Some(session_ref) = self.chat.selected_session_ref.clone() {
                            self.request_chat_messages(session_ref, tx, "loading new session");
                        }
                    }
                    Err(error) => self.apply_chat_error(error, tx, true),
                }
            }
            TuiEvent::ChatMessagesFinished {
                request_id,
                generation,
                session_ref,
                result,
            } => {
                if self.chat_in_flight != Some(request_id) || self.generation != generation {
                    return Ok(());
                }
                self.chat_in_flight = None;
                if self.chat.selected_session_ref.as_deref() != Some(session_ref.as_str()) {
                    return Ok(());
                }
                match result {
                    Ok(messages) => {
                        let count = messages.total_messages;
                        if let Some(selected) = &self.chat.selected_session_ref {
                            if let Some(session) = self
                                .chat
                                .sessions
                                .iter_mut()
                                .find(|row| row.session_ref == *selected)
                            {
                                session.message_count = Some(count);
                            }
                        }
                        self.chat.apply_messages(messages);
                        self.chat.recompute_phase();
                        self.message = format!("loaded {count} session messages");
                    }
                    Err(error) => self.apply_chat_error(error, tx, true),
                }
            }
            TuiEvent::ChatDelta {
                request_id,
                generation,
                delta,
            } => {
                if self.chat_in_flight != Some(request_id) || self.generation != generation {
                    return Ok(());
                }
                self.chat.append_delta(&delta);
                self.chat.send_state = ChatSendState::Streaming { request_id };
            }
            TuiEvent::ChatDone {
                request_id,
                generation,
                finish_reason,
            } => {
                if self.chat_in_flight != Some(request_id) || self.generation != generation {
                    return Ok(());
                }
                self.chat_task = None;
                self.chat_in_flight = None;
                self.chat.send_state = ChatSendState::RefreshingAfterSend { request_id };
                self.message = format!("chat stream done: {finish_reason}; refreshing transcript");
                if let Some(session_ref) = self.chat.selected_session_ref.clone() {
                    self.request_chat_messages(session_ref, tx, "refreshing persisted transcript");
                }
            }
            TuiEvent::ChatSendError {
                request_id,
                generation,
                error,
                retry,
                may_have_committed,
            } => {
                if self.chat_in_flight != Some(request_id) || self.generation != generation {
                    return Ok(());
                }
                self.chat_task = None;
                self.chat_in_flight = None;
                if let Some(request) = retry {
                    self.chat.retry_non_stream = Some(request.non_stream());
                }
                self.chat.pending_interrupted = true;
                self.chat.send_state = ChatSendState::Error;
                let refresh_after = may_have_committed && self.chat.selected_session_ref.is_some();
                self.apply_chat_error(error, tx, refresh_after);
                if refresh_after {
                    if let Some(session_ref) = self.chat.selected_session_ref.clone() {
                        self.request_chat_messages(session_ref, tx, "refreshing after chat error");
                    }
                }
            }
            TuiEvent::ChatNonStreamFinished {
                request_id,
                generation,
                result,
            } => {
                if self.chat_in_flight != Some(request_id) || self.generation != generation {
                    return Ok(());
                }
                self.chat_task = None;
                self.chat_in_flight = None;
                match result {
                    Ok(text) => {
                        self.chat.pending_assistant = Some(text);
                        self.chat.send_state = ChatSendState::RefreshingAfterSend { request_id };
                        self.message =
                            "non-stream retry completed; refreshing persisted transcript"
                                .to_string();
                        if let Some(session_ref) = self.chat.selected_session_ref.clone() {
                            self.request_chat_messages(
                                session_ref,
                                tx,
                                "refreshing persisted transcript",
                            );
                        }
                    }
                    Err(error) => self.apply_chat_error(error, tx, true),
                }
            }
        }
        self.clamp_selection();
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent, tx: &TuiEventSender) -> miette::Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return Ok(());
        }

        let mode = std::mem::replace(&mut self.input_mode, InputMode::Normal);
        match mode {
            InputMode::Normal => self.handle_normal_key(key, tx)?,
            InputMode::EditingHome {
                mut value,
                mut cursor,
            } => match key.code {
                KeyCode::Enter => {
                    let next = value.trim().to_string();
                    self.save_home(next, tx)?;
                }
                KeyCode::Esc => self.message = "home edit canceled".to_string(),
                _ => {
                    edit_text_value(&mut value, &mut cursor, key.code);
                    self.input_mode = InputMode::EditingHome { value, cursor };
                }
            },
            InputMode::EditingDaemonHost {
                mut value,
                mut cursor,
            } => match key.code {
                KeyCode::Enter => {
                    let next = value.trim().to_string();
                    self.save_daemon_host(next, tx)?;
                }
                KeyCode::Esc => self.message = "daemon host edit canceled".to_string(),
                _ => {
                    edit_text_value(&mut value, &mut cursor, key.code);
                    self.input_mode = InputMode::EditingDaemonHost { value, cursor };
                }
            },
            InputMode::EditingDaemonPort {
                mut value,
                mut cursor,
            } => match key.code {
                KeyCode::Enter => {
                    let next = value.trim().to_string();
                    self.save_daemon_port(next, tx)?;
                }
                KeyCode::Esc => self.message = "daemon port edit canceled".to_string(),
                _ => {
                    edit_text_value(&mut value, &mut cursor, key.code);
                    self.input_mode = InputMode::EditingDaemonPort { value, cursor };
                }
            },
            InputMode::EditingDaemonToken {
                mut value,
                mut cursor,
            } => match key.code {
                KeyCode::Enter => self.save_daemon_token(value, tx),
                KeyCode::Esc => {
                    value.clear();
                    self.message = "daemon token edit canceled".to_string();
                }
                _ => {
                    edit_text_value(&mut value, &mut cursor, key.code);
                    self.input_mode = InputMode::EditingDaemonToken { value, cursor };
                }
            },
            InputMode::EditingFilter {
                kind,
                mut value,
                mut cursor,
            } => match key.code {
                KeyCode::Enter => {
                    self.save_filter(kind, value);
                }
                KeyCode::Esc => {
                    self.message = "filter edit canceled".to_string();
                }
                _ => {
                    edit_text_value(&mut value, &mut cursor, key.code);
                    self.input_mode = InputMode::EditingFilter {
                        kind,
                        value,
                        cursor,
                    };
                }
            },
            InputMode::EditingResourceFilter {
                mut value,
                mut cursor,
            } => match key.code {
                KeyCode::Enter => self.save_resource_filter(value),
                KeyCode::Esc => {
                    self.message = "resource filter edit canceled".to_string();
                }
                _ => {
                    edit_text_value(&mut value, &mut cursor, key.code);
                    self.input_mode = InputMode::EditingResourceFilter { value, cursor };
                }
            },
            InputMode::EditingProviderKey {
                provider,
                mut value,
            } => match key.code {
                KeyCode::Enter => {
                    self.begin_provider_action(provider, ProviderActionRequest::Set(value), tx);
                }
                KeyCode::Esc => {
                    value.clear();
                    self.message = "provider key input canceled".to_string();
                }
                KeyCode::Backspace => {
                    value.pop();
                    self.input_mode = InputMode::EditingProviderKey { provider, value };
                }
                KeyCode::Char(ch) => {
                    value.push(ch);
                    self.input_mode = InputMode::EditingProviderKey { provider, value };
                }
                _ => self.input_mode = InputMode::EditingProviderKey { provider, value },
            },
            InputMode::ConfirmRemove { provider } => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    self.begin_provider_action(provider, ProviderActionRequest::Remove, tx);
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.message = "provider key removal canceled".to_string();
                }
                _ => self.input_mode = InputMode::ConfirmRemove { provider },
            },
        }

        Ok(())
    }

    fn handle_normal_key(&mut self, key: KeyEvent, tx: &TuiEventSender) -> miette::Result<()> {
        if self.selected_menu_entry().item == MenuItem::Chat
            && self.focus == FocusPane::Detail
            && self.handle_chat_key(key, tx)?
        {
            return Ok(());
        }
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Esc => {
                if self.close_tail_view() {
                    self.message = "closed tail pane".to_string();
                } else {
                    self.focus = FocusPane::Menu;
                }
            }
            KeyCode::Tab | KeyCode::Right => {
                if self.selected_menu_entry().item == MenuItem::Training
                    && self.focus == FocusPane::Detail
                {
                    self.toggle_training_tab(tx);
                } else if self.selected_menu_entry().item == MenuItem::Resources
                    && self.focus == FocusPane::Detail
                {
                    self.resources.tab.toggle();
                    self.message = format!("Resources tab: {}", self.resources.tab.label());
                } else if self.selected_menu_entry().item == MenuItem::Chat
                    && self.focus == FocusPane::Detail
                {
                    self.chat.focus = self.chat.focus.next();
                    self.message = format!("Chat focus: {}", self.chat.focus.label());
                } else if matches!(
                    self.selected_menu_entry().item,
                    MenuItem::ProviderAuth
                        | MenuItem::Settings
                        | MenuItem::Resources
                        | MenuItem::Chat
                ) || self.current_navigator_kind().is_some()
                {
                    self.focus = FocusPane::Detail;
                    self.ensure_current_navigator_loaded(tx);
                    if self.selected_menu_entry().item == MenuItem::Resources {
                        self.ensure_resources_loaded(tx);
                    } else if self.selected_menu_entry().item == MenuItem::Chat {
                        self.ensure_chat_loaded(tx);
                    }
                }
            }
            KeyCode::Left => self.focus = FocusPane::Menu,
            KeyCode::Down => self.move_selection(1),
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Enter => self.activate_selected(tx)?,
            KeyCode::Char('r') => self.refresh_current_view(tx),
            KeyCode::Char('/') => self.begin_filter_edit(),
            KeyCode::Char('l') => self.request_current_tail(tx),
            KeyCode::Char('m') => self.request_session_messages(tx),
            KeyCode::Char('p') => self.request_train_metrics(tx),
            KeyCode::Char('o') => self.toggle_server_log_kind(),
            KeyCode::Char('k') => self.begin_provider_key_edit(),
            KeyCode::Char('x') => self.begin_provider_remove_confirm(),
            KeyCode::Char('c') => {
                if matches!(
                    self.selected_menu_entry().item,
                    MenuItem::Servers | MenuItem::Sessions
                ) {
                    self.begin_chat_from_current_navigator(tx);
                } else if self.selected_menu_entry().item == MenuItem::ProviderAuth {
                    self.begin_provider_action(
                        self.selected_provider(),
                        ProviderActionRequest::Check,
                        tx,
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_chat_key(&mut self, key: KeyEvent, tx: &TuiEventSender) -> miette::Result<bool> {
        if self.chat.phase == ChatPhase::Workspace
            && self.chat.focus == super::chat::ChatFocus::Composer
        {
            match key.code {
                KeyCode::Esc => {
                    if self.chat.send_state.is_in_flight() {
                        self.cancel_chat_task(tx);
                    } else {
                        self.chat.focus = super::chat::ChatFocus::Chooser;
                        self.message = "left composer; Esc again returns to menu".to_string();
                    }
                    return Ok(true);
                }
                KeyCode::Tab => {
                    self.chat.focus = self.chat.focus.next();
                    self.message = format!("Chat focus: {}", self.chat.focus.label());
                    return Ok(true);
                }
                KeyCode::Enter => {
                    if self.chat.send_state.is_idle() {
                        self.begin_chat_send(tx, true);
                    } else {
                        self.message = "chat send in progress; Enter is disabled".to_string();
                    }
                    return Ok(true);
                }
                KeyCode::Backspace
                | KeyCode::Delete
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::Char(_) => {
                    if self.chat.send_state.is_idle() {
                        edit_text_value(
                            &mut self.chat.composer,
                            &mut self.chat.composer_cursor,
                            key.code,
                        );
                    } else {
                        self.message = "composer is locked while chat sends".to_string();
                    }
                    return Ok(true);
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Esc => {
                if self.chat.send_state.is_in_flight() {
                    self.cancel_chat_task(tx);
                } else {
                    self.focus = FocusPane::Menu;
                }
                Ok(true)
            }
            KeyCode::Tab | KeyCode::Right => {
                self.chat.focus = self.chat.focus.next();
                self.message = format!("Chat focus: {}", self.chat.focus.label());
                Ok(true)
            }
            KeyCode::Up => {
                self.chat.move_selection(-1);
                Ok(true)
            }
            KeyCode::Down => {
                self.chat.move_selection(1);
                Ok(true)
            }
            KeyCode::Enter => {
                self.activate_chat_selection(tx);
                Ok(true)
            }
            KeyCode::Char('n') => {
                self.begin_chat_create_session(tx);
                Ok(true)
            }
            KeyCode::Char('s') => {
                self.chat.phase = ChatPhase::ChooseServer;
                self.chat.focus = super::chat::ChatFocus::Chooser;
                self.message = "choose a running server".to_string();
                Ok(true)
            }
            KeyCode::Char('a') => {
                self.chat.select_adapter_next();
                self.message = self
                    .chat
                    .selected_adapter_row()
                    .map(|row| {
                        format!(
                            "selected adapter {}; compatibility unverified",
                            row.short_ref
                        )
                    })
                    .unwrap_or_else(|| "adapter selection cleared".to_string());
                Ok(true)
            }
            KeyCode::Char('r') => {
                self.refresh_chat_view(tx);
                Ok(true)
            }
            KeyCode::Char('h') => {
                if self.chat.send_state.is_in_flight() {
                    self.message =
                        "chat send in progress; context change applies after it finishes"
                            .to_string();
                } else {
                    self.chat.cycle_context_mode();
                    self.message = format!(
                        "chat context for next send: {}",
                        self.chat.context_mode.label()
                    );
                }
                Ok(true)
            }
            KeyCode::Char('f') => {
                self.begin_chat_retry_non_stream(tx);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn activate_chat_selection(&mut self, tx: &TuiEventSender) {
        match self.chat.phase {
            ChatPhase::NoRunningServer => {
                self.message =
                    "no running server; Slice 3 does not start servers from TUI".to_string();
            }
            ChatPhase::ChooseServer => {
                self.chat.select_server_by_index(self.chat.selected_server);
                self.chat.phase = ChatPhase::ChooseSession;
                self.message = "server selected; choose or create a session".to_string();
            }
            ChatPhase::ChooseSession => {
                if self.chat.selected_session == 0 {
                    self.begin_chat_create_session(tx);
                } else {
                    self.chat
                        .select_session_by_index(self.chat.selected_session);
                    self.chat.recompute_phase();
                    if let Some(session_ref) = self.chat.selected_session_ref.clone() {
                        self.request_chat_messages(session_ref, tx, "loading session messages");
                    }
                }
            }
            ChatPhase::Workspace => {
                if self.chat.focus != super::chat::ChatFocus::Composer {
                    self.chat.focus = super::chat::ChatFocus::Composer;
                    self.message = "composer focused".to_string();
                } else if self.chat.send_state.is_idle() {
                    self.begin_chat_send(tx, true);
                }
            }
        }
    }

    fn begin_chat_from_current_navigator(&mut self, tx: &TuiEventSender) {
        match self.selected_menu_entry().item {
            MenuItem::Servers => {
                let Some(row) = self
                    .navigator
                    .state(NavigatorListKind::Servers)
                    .selected_row()
                    .cloned()
                else {
                    self.message = "select a server before opening Chat".to_string();
                    return;
                };
                let running = row
                    .raw
                    .get("running")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                if !running {
                    self.message = "server is not running; Chat needs a running server".to_string();
                    return;
                }
                self.chat.selected_server_ref = Some(row.item_ref.clone());
                self.chat.selected_session_ref = None;
                self.chat.selected_session = 0;
                self.chat.phase = ChatPhase::ChooseSession;
                self.selected_menu = self
                    .menu_entries()
                    .iter()
                    .position(|entry| entry.item == MenuItem::Chat)
                    .unwrap_or(self.selected_menu);
                self.focus = FocusPane::Detail;
                self.message = "opened Chat from selected server".to_string();
                self.request_chat_overview(tx, "loading chat sessions");
            }
            MenuItem::Sessions => {
                let Some(row) = self
                    .navigator
                    .state(NavigatorListKind::Sessions)
                    .selected_row()
                    .cloned()
                else {
                    self.message = "select a session before opening Chat".to_string();
                    return;
                };
                self.chat.selected_session_ref = Some(row.item_ref.clone());
                self.chat.selected_server_ref = None;
                self.chat.selected_session = 1;
                self.chat.phase = ChatPhase::ChooseServer;
                self.selected_menu = self
                    .menu_entries()
                    .iter()
                    .position(|entry| entry.item == MenuItem::Chat)
                    .unwrap_or(self.selected_menu);
                self.focus = FocusPane::Detail;
                self.message = "opened Chat from selected session".to_string();
                self.request_chat_overview(tx, "loading running servers for session");
            }
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.focus == FocusPane::Detail
            && self.selected_menu_entry().item == MenuItem::ProviderAuth
            && !self.auth_rows.is_empty()
        {
            self.selected_provider =
                move_index(self.selected_provider, self.auth_rows.len(), delta);
        } else if self.focus == FocusPane::Detail
            && self.selected_menu_entry().item == MenuItem::Settings
        {
            let entries = self.settings_entries();
            self.selected_setting = move_index(self.selected_setting, entries.len(), delta);
        } else if self.focus == FocusPane::Detail
            && self.selected_menu_entry().item == MenuItem::Chat
        {
            self.chat.move_selection(delta);
        } else if self.focus == FocusPane::Detail {
            if let Some(kind) = self.current_navigator_kind() {
                self.navigator.state_mut(kind).move_selection(delta);
            } else {
                let entries = self.menu_entries();
                self.selected_menu = move_index(self.selected_menu, entries.len(), delta);
            }
        } else {
            let entries = self.menu_entries();
            self.selected_menu = move_index(self.selected_menu, entries.len(), delta);
        }
    }

    fn activate_selected(&mut self, tx: &TuiEventSender) -> miette::Result<()> {
        let entry = self.selected_menu_entry();
        if !entry.enabled {
            self.message = format!("{} is planned for Slice 2", entry.label);
            return Ok(());
        }
        match entry.item {
            MenuItem::StartDaemon => self.begin_start_daemon(tx),
            MenuItem::ProviderAuth => {
                self.focus = FocusPane::Detail;
                self.begin_provider_action(
                    self.selected_provider(),
                    ProviderActionRequest::Check,
                    tx,
                );
            }
            MenuItem::Settings => {
                if self.focus == FocusPane::Detail {
                    self.begin_selected_setting_edit();
                } else {
                    self.focus = FocusPane::Detail;
                    self.message = "select a setting row and press Enter to edit".to_string();
                }
            }
            MenuItem::Dashboard => {}
            MenuItem::Chat => {
                self.focus = FocusPane::Detail;
                self.ensure_chat_loaded(tx);
            }
            MenuItem::Resources => {
                self.focus = FocusPane::Detail;
                self.ensure_resources_loaded(tx);
            }
            MenuItem::Models
            | MenuItem::Adapters
            | MenuItem::Datasets
            | MenuItem::Servers
            | MenuItem::Sessions
            | MenuItem::Training => {
                self.focus = FocusPane::Detail;
                if let Some(kind) = self.current_navigator_kind() {
                    if matches!(
                        self.navigator.state(kind).load_state,
                        NavigatorLoadState::Idle
                    ) || self.navigator.state(kind).rows.is_empty()
                    {
                        self.request_navigator_list(kind, tx, format!("loading {}", kind.label()));
                    } else {
                        self.request_selected_detail(kind, tx);
                    }
                }
            }
        }
        Ok(())
    }

    fn begin_start_daemon(&mut self, tx: &TuiEventSender) {
        if !self.can_start_daemon() {
            self.message = match self.mode {
                AppMode::Bootstrap(BootstrapReason::AuthRequired) => {
                    "daemon is reachable; edit token instead of starting another daemon".to_string()
                }
                AppMode::Bootstrap(BootstrapReason::ConfigError) => {
                    "fix daemon URL config before starting".to_string()
                }
                AppMode::Operator => "daemon is already reachable".to_string(),
                _ => "daemon start is already in progress".to_string(),
            };
            return;
        }

        let bind = match daemon_start_bind_from_url(&self.daemon_url.url) {
            Ok(bind) => bind,
            Err(error) => {
                self.daemon_action = DaemonActionState::StartFailed {
                    message: error.clone(),
                    stdout_log: Some(self.inspection.stdout_log_path.clone()),
                    stderr_log: Some(self.inspection.stderr_log_path.clone()),
                };
                self.message = error;
                return;
            }
        };

        let request_id = self.next_request_id();
        let generation = self.generation;
        self.start_in_flight = Some(request_id);
        self.daemon_action = DaemonActionState::Starting {
            request_id,
            phase: StartPhase::PollingHealthz,
            warning: bind.warning.clone(),
        };
        self.message = "starting detached daemon; UI remains live".to_string();

        let options = DetachedDaemonOptions {
            home: Some(self.home.clone()),
            host: Some(bind.host),
            port: Some(bind.port),
            allow_unsafe_bind: false,
        };
        let tx = tx.clone();
        tokio::spawn(async move {
            let result = start_daemon_detached(options)
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(TuiEvent::StartFinished {
                request_id,
                generation,
                result,
            });
        });
    }

    fn apply_navigator_list_result(
        &mut self,
        kind: NavigatorListKind,
        result: Result<Vec<NavigatorRow>, NavigatorError>,
    ) {
        match result {
            Ok(rows) => {
                let count = rows.len();
                let label = count_label(kind, &rows);
                self.navigator.state_mut(kind).apply_rows(rows);
                self.dashboard.apply_updates(vec![DashboardCountUpdate {
                    kind,
                    result: Ok(label),
                }]);
                self.message = format!("loaded {count} {}", kind.label());
            }
            Err(error) => self.apply_navigator_error(kind, error, false),
        }
    }

    fn apply_navigator_detail_result(
        &mut self,
        kind: NavigatorListKind,
        item_ref: String,
        result: Result<NavigatorDetail, NavigatorError>,
    ) {
        match result {
            Ok(detail) => {
                self.navigator
                    .state_mut(kind)
                    .detail_cache
                    .insert(item_ref.clone(), detail);
                self.navigator.state_mut(kind).load_state = NavigatorLoadState::Ready;
                self.message = format!("inspected {item_ref}");
            }
            Err(error) => self.apply_navigator_error(kind, error, true),
        }
    }

    fn apply_tail_result(
        &mut self,
        kind: NavigatorListKind,
        source: TailSource,
        result: Result<TailPane, NavigatorError>,
    ) {
        match result {
            Ok(tail) => {
                self.navigator.state_mut(kind).active_tail = Some(tail);
                self.navigator.state_mut(kind).load_state = NavigatorLoadState::Ready;
                self.message = format!("loaded {}", source.title());
            }
            Err(error) => {
                if error.is_not_found() {
                    self.navigator.state_mut(kind).active_tail = Some(TailPane {
                        source,
                        loaded_at: "not loaded".to_string(),
                        scroll_offset: 0,
                        truncated: false,
                        lines: Vec::new(),
                        error: Some(error.to_string()),
                    });
                    self.navigator.state_mut(kind).load_state = NavigatorLoadState::StaleItem {
                        message: error.to_string(),
                    };
                    self.message = "selected tail target disappeared; refresh the list".to_string();
                } else {
                    self.apply_navigator_error(kind, error, true);
                }
            }
        }
    }

    fn apply_navigator_error(
        &mut self,
        kind: NavigatorListKind,
        error: NavigatorError,
        selected_item: bool,
    ) {
        if error.is_auth_required() {
            self.daemon = DaemonSnapshot {
                state: DaemonConnectionState::AuthRequired,
                detail: error.to_string(),
                status: None,
                doctor: None,
            };
            self.update_mode();
            self.message = "daemon auth required for navigator data".to_string();
            return;
        }
        if error.is_down() {
            self.daemon = DaemonSnapshot::down(error.to_string());
            self.update_mode();
            self.message = "daemon became unreachable".to_string();
            return;
        }
        let stale = !self.navigator.state(kind).rows.is_empty();
        self.navigator.state_mut(kind).load_state = if error.is_not_found() && selected_item {
            NavigatorLoadState::StaleItem {
                message: format!("{error}; refresh to update the list"),
            }
        } else {
            NavigatorLoadState::Error {
                message: error.to_string(),
                stale,
            }
        };
        self.message = format!("{} read failed: {error}", kind.label());
    }

    fn apply_chat_error(
        &mut self,
        error: ChatError,
        tx: &TuiEventSender,
        refresh_if_down_or_committed: bool,
    ) {
        if error.is_auth_required() {
            self.daemon = DaemonSnapshot {
                state: DaemonConnectionState::AuthRequired,
                detail: error.to_string(),
                status: None,
                doctor: None,
            };
            self.update_mode();
            self.chat.last_error = Some(error.to_string());
            self.message = "daemon auth required for Chat".to_string();
            return;
        }
        if error.is_down() {
            self.chat.load_state = ChatLoadState::Error {
                message: error.to_string(),
                stale: !self.chat.transcript.is_empty(),
            };
            self.chat.last_error = Some(error.to_string());
            self.message = "chat request failed; confirming daemon health".to_string();
            self.request_refresh(tx, "confirming daemon health after chat failure");
            return;
        }
        match &error {
            ChatError::NotFound(message) => {
                self.chat.load_state = ChatLoadState::StaleSelection {
                    message: format!("{message}; refresh and reselect"),
                };
                self.message = "selected chat session or server disappeared".to_string();
            }
            ChatError::Conflict { kind, message } => {
                self.chat.last_error = Some(chat_conflict_guidance(*kind, message));
                self.message = chat_conflict_guidance(*kind, message);
            }
            ChatError::StreamUnsupported(message) => {
                self.chat.last_error = Some(format!(
                    "{message}; press f for an explicit non-stream retry"
                ));
                self.message =
                    "streaming unavailable; press f to retry non-stream manually".to_string();
            }
            ChatError::ServerProxyFailed(message) => {
                self.chat.last_error = Some(format!(
                    "{message}; target server may be unreachable, check server health/logs"
                ));
                self.message = "target server unreachable; check server logs".to_string();
            }
            _ => {
                self.chat.last_error = Some(error.to_string());
                self.message = format!("chat failed: {error}");
            }
        }
        if refresh_if_down_or_committed && self.chat.selected_session_ref.is_some() {
            if let Some(session_ref) = self.chat.selected_session_ref.clone() {
                self.request_chat_messages(session_ref, tx, "refreshing chat after error");
            }
        }
    }

    fn begin_selected_setting_edit(&mut self) {
        match self.selected_setting_entry().item {
            SettingsItem::RuntimeHome => self.begin_home_edit(),
            SettingsItem::DaemonHost => self.begin_daemon_host_edit(),
            SettingsItem::DaemonPort => self.begin_daemon_port_edit(),
            SettingsItem::DaemonToken => self.begin_daemon_token_edit(),
            SettingsItem::ConfigPath | SettingsItem::RuntimeDir | SettingsItem::LogDir => {
                self.message = "selected setting is read-only".to_string();
            }
        }
    }

    fn begin_home_edit(&mut self) {
        let value = self.home.display().to_string();
        let cursor = value.chars().count();
        self.input_mode = InputMode::EditingHome { value, cursor };
        self.selected_menu = self
            .menu_entries()
            .iter()
            .position(|entry| entry.item == MenuItem::Settings)
            .unwrap_or(self.selected_menu);
        self.focus = FocusPane::Detail;
        self.message = "enter switches TUI home for this session; esc cancels".to_string();
    }

    fn begin_daemon_host_edit(&mut self) {
        let value =
            daemon_url_host(&self.daemon_url.url).unwrap_or_else(|| "127.0.0.1".to_string());
        let cursor = value.chars().count();
        self.input_mode = InputMode::EditingDaemonHost { value, cursor };
        self.selected_menu = self
            .menu_entries()
            .iter()
            .position(|entry| entry.item == MenuItem::Settings)
            .unwrap_or(self.selected_menu);
        self.focus = FocusPane::Detail;
        self.message = "enter saves daemon host into daemon.url preference".to_string();
    }

    fn begin_daemon_port_edit(&mut self) {
        let value = daemon_url_port(&self.daemon_url.url)
            .unwrap_or(DEFAULT_DAEMON_PORT)
            .to_string();
        let cursor = value.chars().count();
        self.input_mode = InputMode::EditingDaemonPort { value, cursor };
        self.selected_menu = self
            .menu_entries()
            .iter()
            .position(|entry| entry.item == MenuItem::Settings)
            .unwrap_or(self.selected_menu);
        self.focus = FocusPane::Detail;
        self.message = "enter saves daemon port into daemon.url preference".to_string();
    }

    fn begin_daemon_token_edit(&mut self) {
        let value = self.session_token.clone().unwrap_or_default();
        let cursor = value.chars().count();
        self.input_mode = InputMode::EditingDaemonToken { value, cursor };
        self.selected_menu = self
            .menu_entries()
            .iter()
            .position(|entry| entry.item == MenuItem::Settings)
            .unwrap_or(self.selected_menu);
        self.focus = FocusPane::Detail;
        self.message = "token is current-session only and never written to config".to_string();
    }

    fn begin_provider_key_edit(&mut self) {
        let provider = self.selected_provider();
        self.focus = FocusPane::Detail;
        self.selected_menu = self
            .menu_entries()
            .iter()
            .position(|entry| entry.item == MenuItem::ProviderAuth)
            .unwrap_or(self.selected_menu);
        self.input_mode = InputMode::EditingProviderKey {
            provider,
            value: String::new(),
        };
        self.message = "provider key input is masked and stored only in Keychain".to_string();
    }

    fn begin_provider_remove_confirm(&mut self) {
        let provider = self.selected_provider();
        self.focus = FocusPane::Detail;
        self.selected_menu = self
            .menu_entries()
            .iter()
            .position(|entry| entry.item == MenuItem::ProviderAuth)
            .unwrap_or(self.selected_menu);
        self.input_mode = InputMode::ConfirmRemove { provider };
        self.message = format!("confirm removal for {}", provider.display_name());
    }

    fn begin_provider_action(
        &mut self,
        provider: Provider,
        request: ProviderActionRequest,
        tx: &TuiEventSender,
    ) {
        if self.provider_action_in_flight.is_some() {
            self.message = "provider auth action already in progress".to_string();
            return;
        }
        let request_id = self.next_request_id();
        let pending = match request {
            ProviderActionRequest::Check => "check",
            ProviderActionRequest::Set(_) => "set",
            ProviderActionRequest::Remove => "remove",
        };
        self.provider_action_in_flight = Some((request_id, provider));
        self.set_provider_state(provider, ProviderAuthDisplayState::Pending(pending));
        self.message = format!("{} {} pending", provider.display_name(), pending);
        let tx = tx.clone();
        tokio::spawn(async move {
            let join_result =
                tokio::task::spawn_blocking(move || run_provider_action(provider, request)).await;
            let result = join_result.unwrap_or_else(|error| ProviderActionResult {
                state: ProviderAuthDisplayState::CheckFailed(error.to_string()),
                message: format!("{} auth action failed: {error}", provider.display_name()),
            });
            let _ = tx.send(TuiEvent::ProviderActionFinished {
                request_id,
                provider,
                result,
            });
        });
    }

    fn save_home(&mut self, value: String, tx: &TuiEventSender) -> miette::Result<()> {
        let next_home = expand_home_path(&value);
        if next_home.as_os_str().is_empty() {
            return Err(miette::miette!("home path cannot be empty"));
        }
        self.bump_generation();
        let manager = DaemonManager::new(Some(&next_home)).into_diagnostic()?;
        let inspection = manager.status().into_diagnostic()?;
        let (config, config_error) = load_config_with_error(&next_home);
        self.home = next_home;
        self.config_path = config_path(&self.home);
        self.inspection = inspection;
        self.config = config;
        self.config_error = config_error;
        self.daemon = DaemonSnapshot::idle();
        self.auth_rows = provider_env_rows();
        self.resources = ResourceState::default();
        self.chat.reset_runtime();
        self.resolve_daemon_endpoint();
        self.message = format!("switched TUI home to {}", self.home.display());
        self.request_refresh(tx, "refreshing daemon after home change");
        Ok(())
    }

    fn save_daemon_host(&mut self, value: String, tx: &TuiEventSender) -> miette::Result<()> {
        let host = value.trim();
        if host.is_empty() {
            return Err(miette::miette!("daemon host cannot be empty"));
        }
        let port = daemon_url_port(&self.daemon_url.url).unwrap_or(DEFAULT_DAEMON_PORT);
        self.save_daemon_url_parts(host, port, tx)
    }

    fn save_daemon_port(&mut self, value: String, tx: &TuiEventSender) -> miette::Result<()> {
        let port = value
            .trim()
            .parse::<u16>()
            .map_err(|error| miette::miette!("daemon port must be 1-65535: {error}"))?;
        let host = daemon_url_host(&self.daemon_url.url).unwrap_or_else(|| "127.0.0.1".to_string());
        self.save_daemon_url_parts(&host, port, tx)
    }

    fn save_daemon_url_parts(
        &mut self,
        host: &str,
        port: u16,
        tx: &TuiEventSender,
    ) -> miette::Result<()> {
        let value = format!("http://{}:{port}", host_for_url_input(host));
        self.bump_generation();
        self.flag_daemon_url = Some(value.clone());
        self.config.daemon.url = Some(value.clone());
        self.config.save(&self.home).into_diagnostic()?;
        self.resolve_daemon_endpoint();
        self.message = format!("saved daemon host/port and applied to this TUI session: {value}");
        self.request_refresh(tx, "refreshing daemon after host/port change");
        Ok(())
    }

    fn save_daemon_token(&mut self, value: String, tx: &TuiEventSender) {
        self.bump_generation();
        let trimmed = value.trim().to_string();
        self.session_token = (!trimmed.is_empty()).then_some(trimmed);
        self.resolve_daemon_endpoint();
        self.message = if self.session_token.is_some() {
            "daemon token set for this TUI session only".to_string()
        } else {
            "session daemon token cleared".to_string()
        };
        self.request_refresh(tx, "refreshing daemon after token change");
    }

    fn apply_refresh(&mut self, data: RefreshData) {
        self.inspection = data.inspection;
        self.config = data.config;
        self.config_error = data.config_error;
        self.daemon_url = data.daemon_url;
        self.daemon_token = data.daemon_token;
        self.daemon = data.daemon;
        if let Some(counts) = data.dashboard_counts {
            self.dashboard.apply_updates(counts);
        }
        self.update_mode();
        self.message = "refreshed daemon state".to_string();
    }

    fn resolve_daemon_endpoint(&mut self) {
        let env_url = env_string(DAEMON_URL_ENV_VAR);
        self.daemon_url = resolve_daemon_url(DaemonUrlInputs {
            flag_url: self.flag_daemon_url.as_deref(),
            env_url: env_url.as_deref(),
            config_url: self.config.daemon.url.as_deref(),
            metadata: self.inspection.process.as_ref(),
        });
        if self.config_error.is_none() {
            self.config_error = self.daemon_url.config_error.clone();
        }
        self.daemon_token = resolve_tui_daemon_token(
            self.flag_token.as_deref(),
            env_string(DAEMON_TOKEN_ENV_VAR).as_deref(),
            self.session_token.as_deref(),
        );
        self.update_mode();
    }

    fn update_mode(&mut self) {
        self.mode = if self.config_error.is_some() {
            AppMode::Bootstrap(BootstrapReason::ConfigError)
        } else {
            match self.daemon.state {
                DaemonConnectionState::Ready => AppMode::Operator,
                DaemonConnectionState::AuthRequired => {
                    AppMode::Bootstrap(BootstrapReason::AuthRequired)
                }
                DaemonConnectionState::Timeout => {
                    AppMode::Bootstrap(BootstrapReason::DaemonTimeout)
                }
                DaemonConnectionState::DaemonProtocolError => {
                    AppMode::Bootstrap(BootstrapReason::ProtocolError)
                }
                DaemonConnectionState::DaemonError => {
                    AppMode::Bootstrap(BootstrapReason::ProtocolError)
                }
                DaemonConnectionState::Down => AppMode::Bootstrap(BootstrapReason::DaemonDown),
            }
        };
        self.clamp_selection();
    }

    fn set_provider_state(&mut self, provider: Provider, state: ProviderAuthDisplayState) {
        if let Some(row) = self
            .auth_rows
            .iter_mut()
            .find(|row| row.provider == provider)
        {
            row.state = state;
        }
    }

    fn clamp_selection(&mut self) {
        let menu_len = self.menu_entries().len();
        self.selected_menu = self.selected_menu.min(menu_len.saturating_sub(1));
        self.selected_provider = self
            .selected_provider
            .min(self.auth_rows.len().saturating_sub(1));
        self.selected_setting = self
            .selected_setting
            .min(self.settings_entries().len().saturating_sub(1));
        if !matches!(
            self.selected_menu_entry().item,
            MenuItem::ProviderAuth | MenuItem::Settings | MenuItem::Resources | MenuItem::Chat
        ) && self.current_navigator_kind().is_none()
        {
            self.focus = FocusPane::Menu;
        }
    }

    fn next_request_id(&mut self) -> u64 {
        self.request_counter = self.request_counter.saturating_add(1);
        self.request_counter
    }

    fn bump_generation(&mut self) {
        self.generation = self.generation.saturating_add(1);
        self.start_in_flight = None;
        self.refresh_in_flight = None;
        self.navigator_in_flight = None;
        self.detail_in_flight = None;
        self.tail_in_flight = None;
        self.resource_in_flight = None;
        self.chat_in_flight = None;
        if let Some(handle) = self.chat_task.take() {
            handle.abort();
        }
        self.daemon_action = DaemonActionState::Idle;
    }
}

async fn run_chat_stream_task(
    client: ChatClient,
    request: ChatSendRequest,
    request_for_retry: ChatSendRequest,
    request_id: u64,
    generation: u64,
    tx: TuiEventSender,
) {
    let response = match client.post_stream(&request).await {
        Ok(response) => response,
        Err(error) => {
            let retry = error
                .is_stream_unsupported()
                .then_some(request_for_retry.non_stream());
            let _ = tx.send(TuiEvent::ChatSendError {
                request_id,
                generation,
                error,
                retry,
                may_have_committed: false,
            });
            return;
        }
    };
    let mut response = response;
    let mut decoder = SseDecoder::default();
    loop {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(error) => {
                let _ = tx.send(TuiEvent::ChatSendError {
                    request_id,
                    generation,
                    error: ChatError::Down(format!("failed to read chat stream: {error}")),
                    retry: None,
                    may_have_committed: true,
                });
                return;
            }
        };
        let events = match decoder.push(&chunk) {
            Ok(events) => events,
            Err(error) => {
                let _ = tx.send(TuiEvent::ChatSendError {
                    request_id,
                    generation,
                    error,
                    retry: None,
                    may_have_committed: true,
                });
                return;
            }
        };
        for event in events {
            match event {
                ChatStreamEvent::Delta(delta) => {
                    let _ = tx.send(TuiEvent::ChatDelta {
                        request_id,
                        generation,
                        delta,
                    });
                }
                ChatStreamEvent::Done(finish_reason) => {
                    let _ = tx.send(TuiEvent::ChatDone {
                        request_id,
                        generation,
                        finish_reason,
                    });
                    return;
                }
                ChatStreamEvent::Error(message) => {
                    let _ = tx.send(TuiEvent::ChatSendError {
                        request_id,
                        generation,
                        error: ChatError::Server(message),
                        retry: None,
                        may_have_committed: true,
                    });
                    return;
                }
            }
        }
    }
    if let Err(error) = decoder.finish(false) {
        let _ = tx.send(TuiEvent::ChatSendError {
            request_id,
            generation,
            error,
            retry: None,
            may_have_committed: true,
        });
    }
}

fn chat_conflict_guidance(kind: ChatConflictKind, message: &str) -> String {
    match kind {
        ChatConflictKind::SessionBusy => format!("{message}; retry after this session is idle"),
        ChatConflictKind::ServerStopped => {
            "server stopped; choose another running server".to_string()
        }
        ChatConflictKind::MultipleRunningServers => {
            "multiple running servers; choose one explicitly".to_string()
        }
        ChatConflictKind::NoRunningServer => {
            "no running server; Slice 3 does not start servers from TUI".to_string()
        }
        ChatConflictKind::CompactionRequired => {
            "session compaction required; manual compact is deferred".to_string()
        }
        ChatConflictKind::CompactionFailed => {
            "session compaction failed; check server health and logs".to_string()
        }
        ChatConflictKind::Other => message.to_string(),
    }
}

fn timestamp_label() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("{seconds}s")
}

pub(super) async fn run_tui(command: TuiCommand) -> miette::Result<()> {
    let mut app = TuiApp::new(command)?;
    let mut terminal = TerminalSession::enter()?;
    let (tx, mut rx): (TuiEventSender, TuiEventReceiver) = mpsc::unbounded_channel();
    let mut last_refresh = Instant::now();
    app.request_refresh(&tx, "checking daemon");

    loop {
        while let Ok(event) = rx.try_recv() {
            app.handle_tui_event(event, &tx)?;
        }

        terminal.draw(&app)?;
        if app.should_quit {
            break;
        }

        if event::poll(EVENT_POLL_INTERVAL).into_diagnostic()? {
            let event = event::read().into_diagnostic()?;
            app.handle_event(event, &tx)?;
        }

        if last_refresh.elapsed() >= AUTO_REFRESH_INTERVAL {
            app.request_refresh(&tx, "auto refresh");
            last_refresh = Instant::now();
        }
    }

    Ok(())
}

pub(super) fn mask_secret(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        "*".repeat(value.chars().count().min(64))
    }
}

fn input_line(label: &str, value: &str, cursor: usize, masked: bool) -> InputLine {
    InputLine {
        label: label.to_string(),
        value: value.to_string(),
        cursor: cursor.min(value.chars().count()),
        masked,
    }
}

async fn collect_refresh(inputs: RefreshInputs) -> Result<RefreshData, String> {
    let manager = DaemonManager::new(Some(&inputs.home)).map_err(|error| error.to_string())?;
    let inspection = manager.status().map_err(|error| error.to_string())?;
    let (config, mut config_error) = load_config_with_error(&inputs.home);
    let daemon_url = resolve_daemon_url(DaemonUrlInputs {
        flag_url: inputs.flag_daemon_url.as_deref(),
        env_url: env_string(DAEMON_URL_ENV_VAR).as_deref(),
        config_url: config.daemon.url.as_deref(),
        metadata: inspection.process.as_ref(),
    });
    if config_error.is_none() {
        config_error = daemon_url.config_error.clone();
    }
    let daemon_token = resolve_tui_daemon_token(
        inputs.flag_token.as_deref(),
        env_string(DAEMON_TOKEN_ENV_VAR).as_deref(),
        inputs.session_token.as_deref(),
    );
    let client = DaemonClient::new(
        daemon_url.url.clone(),
        daemon_token.token.clone(),
        daemon_token.source,
    )
    .map_err(|error| error.to_string())?;
    let daemon = client.refresh_auto().await;
    let dashboard_counts = if daemon.state == DaemonConnectionState::Ready {
        Some(client.dashboard_counts().await)
    } else {
        None
    };
    Ok(RefreshData {
        inspection,
        config,
        config_error,
        daemon_url,
        daemon_token,
        daemon,
        dashboard_counts,
    })
}

#[derive(Debug, Clone)]
struct RefreshInputs {
    home: PathBuf,
    flag_daemon_url: Option<String>,
    flag_token: Option<String>,
    session_token: Option<String>,
}

fn load_config_with_error(home: &Path) -> (TentgentConfig, Option<String>) {
    match TentgentConfig::load(home) {
        Ok(config) => (config, None),
        Err(error) => (TentgentConfig::default(), Some(error.to_string())),
    }
}

fn resolve_tui_daemon_token(
    flag_token: Option<&str>,
    env_token: Option<&str>,
    session_token: Option<&str>,
) -> TuiDaemonToken {
    if let Some(token) = clean(session_token) {
        return TuiDaemonToken {
            token: Some(token.to_string()),
            source: TuiTokenSource::Session,
        };
    }
    let resolved = resolve_daemon_token(flag_token, env_token);
    let source = match resolved.source {
        DaemonTokenSource::Flag => TuiTokenSource::Flag,
        DaemonTokenSource::Env => TuiTokenSource::Env,
        DaemonTokenSource::None => TuiTokenSource::None,
    };
    TuiDaemonToken {
        token: resolved.token,
        source,
    }
}

fn provider_env_rows() -> Vec<ProviderAuthRow> {
    Provider::ALL
        .into_iter()
        .map(|provider| {
            let status = env_key_status(provider);
            ProviderAuthRow {
                provider,
                state: provider_state_from_env_status(status),
            }
        })
        .collect()
}

fn provider_state_from_env_status(status: KeyStatus) -> ProviderAuthDisplayState {
    if status.env_present {
        ProviderAuthDisplayState::EnvPresent
    } else {
        ProviderAuthDisplayState::EnvMissingKeychainNotChecked
    }
}

fn provider_state_from_checked_status(status: KeyStatus) -> ProviderAuthDisplayState {
    if status.env_present {
        ProviderAuthDisplayState::EnvPresent
    } else if status.keychain_present {
        ProviderAuthDisplayState::KeychainPresentChecked
    } else {
        ProviderAuthDisplayState::KeychainMissingChecked
    }
}

fn run_provider_action(provider: Provider, request: ProviderActionRequest) -> ProviderActionResult {
    match request {
        ProviderActionRequest::Check => {
            let auth = match AuthManager::new() {
                Ok(auth) => auth,
                Err(error) => {
                    return ProviderActionResult {
                        state: ProviderAuthDisplayState::CheckFailed(error.to_string()),
                        message: format!(
                            "failed to initialize local auth manager for {}",
                            provider.display_name()
                        ),
                    }
                }
            };
            match auth.local_key_status(provider) {
                Ok(status) => ProviderActionResult {
                    state: provider_state_from_checked_status(status),
                    message: format!("checked {} keychain state", provider.display_name()),
                },
                Err(error) => ProviderActionResult {
                    state: ProviderAuthDisplayState::CheckFailed(error.to_string()),
                    message: format!("{} auth check failed: {error}", provider.display_name()),
                },
            }
        }
        ProviderActionRequest::Set(secret) => {
            let secret = secret.trim().to_string();
            if secret.is_empty() {
                return ProviderActionResult {
                    state: provider_state_from_env_status(env_key_status(provider)),
                    message: format!("{} key was empty; nothing saved", provider.display_name()),
                };
            }
            let auth = match AuthManager::new() {
                Ok(auth) => auth,
                Err(error) => {
                    return ProviderActionResult {
                        state: ProviderAuthDisplayState::CheckFailed(error.to_string()),
                        message: format!(
                            "failed to initialize local auth manager for {}",
                            provider.display_name()
                        ),
                    }
                }
            };
            match auth.set_key(provider, &secret) {
                Ok(()) => {
                    let env_status = env_key_status(provider);
                    let state = if env_status.env_present {
                        ProviderAuthDisplayState::EnvPresent
                    } else {
                        ProviderAuthDisplayState::KeychainPresentChecked
                    };
                    let env_note = env_status
                        .env_present
                        .then_some("; env overrides keychain")
                        .unwrap_or("");
                    ProviderActionResult {
                        state,
                        message: format!(
                            "saved {} keychain entry{env_note}",
                            provider.display_name()
                        ),
                    }
                }
                Err(error) => ProviderActionResult {
                    state: ProviderAuthDisplayState::CheckFailed(error.to_string()),
                    message: format!("{} key save failed: {error}", provider.display_name()),
                },
            }
        }
        ProviderActionRequest::Remove => {
            let auth = match AuthManager::new() {
                Ok(auth) => auth,
                Err(error) => {
                    return ProviderActionResult {
                        state: ProviderAuthDisplayState::CheckFailed(error.to_string()),
                        message: format!(
                            "failed to initialize local auth manager for {}",
                            provider.display_name()
                        ),
                    }
                }
            };
            match auth.remove_key(provider) {
                Ok(removed) => {
                    let env_status = env_key_status(provider);
                    ProviderActionResult {
                        state: if env_status.env_present {
                            ProviderAuthDisplayState::EnvPresent
                        } else {
                            ProviderAuthDisplayState::KeychainMissingChecked
                        },
                        message: if removed {
                            format!("removed {} keychain entry", provider.display_name())
                        } else {
                            format!("no {} keychain entry was present", provider.display_name())
                        },
                    }
                }
                Err(error) => ProviderActionResult {
                    state: ProviderAuthDisplayState::CheckFailed(error.to_string()),
                    message: format!("{} key removal failed: {error}", provider.display_name()),
                },
            }
        }
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

fn edit_text_value(value: &mut String, cursor: &mut usize, code: KeyCode) {
    *cursor = (*cursor).min(value.chars().count());
    match code {
        KeyCode::Left => *cursor = cursor.saturating_sub(1),
        KeyCode::Right => *cursor = (*cursor + 1).min(value.chars().count()),
        KeyCode::Home => *cursor = 0,
        KeyCode::End => *cursor = value.chars().count(),
        KeyCode::Backspace => {
            if *cursor > 0 {
                let index = byte_index_at_char(value, *cursor - 1);
                value.remove(index);
                *cursor -= 1;
            }
        }
        KeyCode::Delete => {
            if *cursor < value.chars().count() {
                let index = byte_index_at_char(value, *cursor);
                value.remove(index);
            }
        }
        KeyCode::Char(ch) => {
            let index = byte_index_at_char(value, *cursor);
            value.insert(index, ch);
            *cursor += 1;
        }
        _ => {}
    }
}

fn byte_index_at_char(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(value.len())
}

fn env_string(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn clean(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn token_source_setting_label(source: TuiTokenSource) -> String {
    match source {
        TuiTokenSource::Session => "session token present".to_string(),
        TuiTokenSource::Flag => "--token flag".to_string(),
        TuiTokenSource::Env => "TENTGENT_DAEMON_TOKEN".to_string(),
        TuiTokenSource::None => "none".to_string(),
    }
}

fn shell_single_quote(path: &Path) -> String {
    let value = path.display().to_string();
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn expand_home_path(value: &str) -> PathBuf {
    let trimmed = value.trim();
    if trimmed == "~" {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home);
        }
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(trimmed)
}

fn daemon_url_host(daemon_url: &str) -> Option<String> {
    Url::parse(daemon_url)
        .ok()
        .and_then(|url| url.host_str().map(ToOwned::to_owned))
}

fn daemon_url_port(daemon_url: &str) -> Option<u16> {
    Url::parse(daemon_url).ok().and_then(|url| url.port())
}

fn host_for_url_input(host: &str) -> String {
    let trimmed = host.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        trimmed.to_string()
    } else if trimmed.contains(':') {
        format!("[{trimmed}]")
    } else {
        trimmed.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DaemonStartBind {
    host: String,
    port: u16,
    warning: Option<String>,
}

fn daemon_start_bind_from_url(daemon_url: &str) -> Result<DaemonStartBind, String> {
    let parsed = Url::parse(daemon_url)
        .map_err(|error| format!("invalid daemon URL `{daemon_url}`: {error}"))?;
    if parsed.scheme() != "http" {
        return Err(format!(
            "TUI can only start a local daemon from an http loopback URL; got `{}`",
            parsed.scheme()
        ));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| format!("daemon URL `{daemon_url}` is missing a host"))?
        .to_string();
    if !is_loopback_host(&host) {
        return Err(format!(
            "TUI start is limited to loopback hosts in Slice 1.1; `{host}` must be started from CLI"
        ));
    }
    let warning = parsed
        .port()
        .is_none()
        .then(|| format!("daemon URL has no explicit port; start will use {DEFAULT_DAEMON_PORT}"));
    Ok(DaemonStartBind {
        host,
        port: parsed.port().unwrap_or(DEFAULT_DAEMON_PORT),
        warning,
    })
}

fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<IpAddr>()
        .map(|addr| addr.is_loopback())
        .unwrap_or(false)
}

#[cfg(test)]
impl TuiApp {
    pub(super) fn test_app(home: PathBuf) -> Self {
        let config = TentgentConfig::default();
        let inspection = stopped_inspection(&home);
        let daemon_url = resolve_daemon_url(DaemonUrlInputs {
            flag_url: Some("http://127.0.0.1:18791"),
            env_url: None,
            config_url: None,
            metadata: None,
        });
        Self {
            home: home.clone(),
            config_path: config_path(&home),
            config,
            config_error: None,
            inspection,
            daemon_url,
            daemon_token: resolve_tui_daemon_token(None, None, None),
            daemon: DaemonSnapshot::idle(),
            auth_rows: provider_env_rows(),
            navigator: NavigatorState::default(),
            resources: ResourceState::default(),
            chat: ChatState::default(),
            dashboard: DashboardState::default(),
            mode: AppMode::Bootstrap(BootstrapReason::DaemonDown),
            focus: FocusPane::Menu,
            selected_menu: 0,
            selected_provider: 0,
            selected_setting: 0,
            input_mode: InputMode::Normal,
            daemon_action: DaemonActionState::Idle,
            message: String::new(),
            should_quit: false,
            refresh_in_flight: None,
            start_in_flight: None,
            provider_action_in_flight: None,
            navigator_in_flight: None,
            detail_in_flight: None,
            tail_in_flight: None,
            resource_in_flight: None,
            chat_in_flight: None,
            chat_task: None,
            generation: 0,
            request_counter: 0,
            flag_daemon_url: Some("http://127.0.0.1:18791".to_string()),
            flag_token: None,
            session_token: None,
        }
    }
}

#[cfg(test)]
fn stopped_inspection(home: &Path) -> DaemonInspection {
    DaemonInspection {
        home_dir: home.to_path_buf(),
        runtime_dir: home.join("runtime"),
        log_dir: home.join("logs"),
        process_path: home.join("runtime/daemon.toml"),
        pid_path: home.join("runtime/tentgent.pid"),
        stdout_log_path: home.join("logs/daemon.stdout.log"),
        stderr_log_path: home.join("logs/daemon.stderr.log"),
        running: false,
        process: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_key_masking_does_not_echo_secret() {
        assert_eq!(mask_secret("abc123"), "******");
        assert!(!mask_secret("abc123").contains("abc123"));
    }

    #[test]
    fn env_only_auth_rows_do_not_claim_keychain_was_checked() {
        for row in provider_env_rows() {
            assert!(matches!(
                row.state,
                ProviderAuthDisplayState::EnvPresent
                    | ProviderAuthDisplayState::EnvMissingKeychainNotChecked
            ));
        }
    }

    #[test]
    fn tui_start_action_builds_shared_detached_options_from_resolved_url() {
        let home = PathBuf::from("/tmp/tentgent-tui-home");
        let app = TuiApp::test_app(home.clone());

        let options = app.detached_start_options().expect("start options");
        assert_eq!(options.home.as_deref(), Some(home.as_path()));
        assert_eq!(options.host.as_deref(), Some("127.0.0.1"));
        assert_eq!(options.port, Some(18791));
        assert!(!options.allow_unsafe_bind);
        assert_eq!(
            app.start_command(),
            "tentgent daemon start --home '/tmp/tentgent-tui-home' --host 127.0.0.1 --port 18791"
        );
    }

    #[test]
    fn start_target_parsing_uses_loopback_http_and_ignores_path_query() {
        let bind = daemon_start_bind_from_url("http://localhost:18791/v1/status?x=1")
            .expect("loopback target");

        assert_eq!(bind.host, "localhost");
        assert_eq!(bind.port, 18791);
        assert_eq!(bind.warning, None);
    }

    #[test]
    fn start_target_defaults_missing_port_to_daemon_port_with_warning() {
        let bind = daemon_start_bind_from_url("http://127.0.0.1").expect("loopback target");

        assert_eq!(bind.port, DEFAULT_DAEMON_PORT);
        assert!(bind.warning.expect("warning").contains("no explicit port"));
    }

    #[test]
    fn start_target_rejects_https_and_non_loopback() {
        assert!(daemon_start_bind_from_url("https://127.0.0.1:8790")
            .expect_err("https is not a local start target")
            .contains("http loopback"));
        assert!(daemon_start_bind_from_url("http://0.0.0.0:8790")
            .expect_err("wildcard is not safe")
            .contains("loopback"));
        assert!(daemon_start_bind_from_url("http://example.com:8790")
            .expect_err("remote is not local")
            .contains("loopback"));
    }

    #[test]
    fn auth_required_mode_does_not_enable_start_daemon() {
        let mut app = TuiApp::test_app(PathBuf::from("/tmp/tentgent-tui-auth"));
        app.daemon = DaemonSnapshot {
            state: DaemonConnectionState::AuthRequired,
            detail: "token required".to_string(),
            status: None,
            doctor: None,
        };
        app.update_mode();

        assert_eq!(app.mode, AppMode::Bootstrap(BootstrapReason::AuthRequired));
        assert!(!app.can_start_daemon());
        let start = app
            .menu_entries()
            .into_iter()
            .find(|entry| entry.item == MenuItem::StartDaemon)
            .expect("start entry");
        assert!(!start.enabled);
    }

    #[test]
    fn operator_menu_keeps_settings_and_provider_auth_available() {
        let mut app = TuiApp::test_app(PathBuf::from("/tmp/tentgent-tui-operator-menu"));
        app.daemon = DaemonSnapshot {
            state: DaemonConnectionState::Ready,
            detail: "ready".to_string(),
            status: None,
            doctor: None,
        };
        app.update_mode();

        let entries = app.menu_entries();
        for item in [MenuItem::ProviderAuth, MenuItem::Settings] {
            let entry = entries
                .iter()
                .find(|entry| entry.item == item)
                .expect("operator setup entry");
            assert!(entry.enabled);
        }
        assert!(!entries.iter().any(|entry| entry.label == "Daemon host"));
        assert!(!entries.iter().any(|entry| entry.label == "Daemon port"));
        assert!(entries
            .iter()
            .any(|entry| entry.item == MenuItem::Resources && entry.enabled));
    }

    #[test]
    fn resources_menu_is_operator_only() {
        let mut app = TuiApp::test_app(PathBuf::from("/tmp/tentgent-tui-resources-menu"));

        assert!(!app
            .menu_entries()
            .iter()
            .any(|entry| entry.item == MenuItem::Resources));
        assert!(!app
            .menu_entries()
            .iter()
            .any(|entry| entry.item == MenuItem::Chat));

        app.daemon = DaemonSnapshot {
            state: DaemonConnectionState::Ready,
            detail: "ready".to_string(),
            status: None,
            doctor: None,
        };
        app.update_mode();

        assert!(app
            .menu_entries()
            .iter()
            .any(|entry| entry.item == MenuItem::Resources));
        assert!(app
            .menu_entries()
            .iter()
            .any(|entry| entry.item == MenuItem::Chat));
        let chat_index = app
            .menu_entries()
            .iter()
            .position(|entry| entry.item == MenuItem::Chat)
            .expect("chat menu");
        let dashboard_index = app
            .menu_entries()
            .iter()
            .position(|entry| entry.item == MenuItem::Dashboard)
            .expect("dashboard menu");
        assert_eq!(chat_index, dashboard_index + 1);
    }

    #[test]
    fn config_error_mode_keeps_settings_enabled_and_start_disabled() {
        let mut app = TuiApp::test_app(PathBuf::from("/tmp/tentgent-tui-config"));
        app.config_error = Some("invalid daemon.url".to_string());
        app.update_mode();

        assert_eq!(app.mode, AppMode::Bootstrap(BootstrapReason::ConfigError));
        let entries = app.menu_entries();
        let start = entries
            .iter()
            .find(|entry| entry.item == MenuItem::StartDaemon)
            .expect("start entry");
        let settings = entries
            .iter()
            .find(|entry| entry.item == MenuItem::Settings)
            .expect("settings entry");
        assert!(!start.enabled);
        assert!(settings.enabled);
    }

    #[tokio::test]
    async fn host_and_port_edits_update_daemon_url_preference_and_session_override() {
        let home = unique_home("host-port-edit");
        let mut app = TuiApp::test_app(home.clone());
        let (tx, _rx) = mpsc::unbounded_channel();

        app.save_daemon_host("localhost".to_string(), &tx)
            .expect("save host");
        assert_eq!(
            app.config.daemon.url.as_deref(),
            Some("http://localhost:18791")
        );
        assert_eq!(
            app.flag_daemon_url.as_deref(),
            Some("http://localhost:18791")
        );
        assert_eq!(app.daemon_url.url, "http://localhost:18791");

        app.save_daemon_port("19000".to_string(), &tx)
            .expect("save port");
        assert_eq!(
            app.config.daemon.url.as_deref(),
            Some("http://localhost:19000")
        );
        assert_eq!(
            app.flag_daemon_url.as_deref(),
            Some("http://localhost:19000")
        );
        assert_eq!(app.daemon_url.url, "http://localhost:19000");

        let saved = TentgentConfig::load(&home).expect("saved config");
        assert_eq!(saved.daemon.url.as_deref(), Some("http://localhost:19000"));
    }

    #[test]
    fn generation_bump_invalidates_in_flight_refresh_and_start() {
        let mut app = TuiApp::test_app(PathBuf::from("/tmp/tentgent-tui-generation"));
        app.refresh_in_flight = Some(1);
        app.start_in_flight = Some(2);
        app.chat_in_flight = Some(3);
        app.daemon_action = DaemonActionState::Starting {
            request_id: 2,
            phase: StartPhase::PollingHealthz,
            warning: None,
        };

        app.bump_generation();

        assert_eq!(app.generation, 1);
        assert_eq!(app.refresh_in_flight, None);
        assert_eq!(app.start_in_flight, None);
        assert_eq!(app.chat_in_flight, None);
        assert_eq!(app.daemon_action, DaemonActionState::Idle);
    }

    #[test]
    fn chat_overview_without_running_server_enters_blocked_state() {
        let mut app = TuiApp::test_app(PathBuf::from("/tmp/tentgent-tui-chat-empty"));
        app.daemon = DaemonSnapshot {
            state: DaemonConnectionState::Ready,
            detail: "ready".to_string(),
            status: None,
            doctor: None,
        };
        app.update_mode();

        app.chat.apply_overview(Vec::new(), Vec::new(), Vec::new());

        assert_eq!(app.chat.phase, ChatPhase::NoRunningServer);
        assert!(app.chat.selected_server_ref.is_none());
    }

    #[test]
    fn chat_send_lifecycle_prevents_double_submit() {
        let mut app = TuiApp::test_app(PathBuf::from("/tmp/tentgent-tui-chat-send"));
        app.chat.phase = ChatPhase::Workspace;
        app.chat.selected_server_ref = Some("server".to_string());
        app.chat.selected_session_ref = Some("session".to_string());
        app.chat.context_mode = ChatContextMode::Last10;
        app.chat.composer = "hello".to_string();

        let request = app.chat_send_request(7, true).expect("request");
        assert_eq!(request.request_id, 7);
        assert_eq!(request.context_mode, ChatContextMode::Last10);
        assert_eq!(request.max_session_messages, 10);
        app.chat.start_pending_send(7, request.prompt);
        app.chat_in_flight = Some(7);
        let (tx, _rx) = mpsc::unbounded_channel();

        app.begin_chat_send(&tx, true);

        assert_eq!(app.message, "chat send already in progress");
        assert!(app.chat.send_state.is_in_flight());
    }

    #[test]
    fn chat_context_toggle_is_local_and_respects_send_state() {
        let mut app = TuiApp::test_app(PathBuf::from("/tmp/tentgent-tui-chat-context"));
        app.chat.phase = ChatPhase::Workspace;
        app.chat.focus = ChatFocus::Chooser;
        app.chat.context_mode = ChatContextMode::None;
        let (tx, _rx) = mpsc::unbounded_channel();

        app.handle_chat_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE), &tx)
            .expect("handled");
        assert_eq!(app.chat.context_mode, ChatContextMode::Last2);

        app.chat.send_state = ChatSendState::Streaming { request_id: 5 };
        app.handle_chat_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE), &tx)
            .expect("handled");
        assert_eq!(app.chat.context_mode, ChatContextMode::Last2);
        assert!(app.message.contains("send in progress"));
    }

    #[test]
    fn chat_context_key_types_when_composer_is_focused() {
        let mut app = TuiApp::test_app(PathBuf::from("/tmp/tentgent-tui-chat-context-typing"));
        app.chat.phase = ChatPhase::Workspace;
        app.chat.focus = ChatFocus::Composer;
        app.chat.context_mode = ChatContextMode::Last2;
        let (tx, _rx) = mpsc::unbounded_channel();

        app.handle_chat_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE), &tx)
            .expect("handled");

        assert_eq!(app.chat.context_mode, ChatContextMode::Last2);
        assert_eq!(app.chat.composer, "h");
    }

    #[test]
    fn chat_done_refresh_replaces_pending_transcript() {
        let mut app = TuiApp::test_app(PathBuf::from("/tmp/tentgent-tui-chat-done"));
        app.chat.phase = ChatPhase::Workspace;
        app.chat.selected_session_ref = Some("session".to_string());
        app.chat.pending_user = Some("hello".to_string());
        app.chat.pending_assistant = Some("streamed".to_string());

        app.chat.apply_messages(ChatMessages {
            messages: vec![crate::cli::tui::chat::ChatMessageRow {
                index: Some(0),
                role: "assistant".to_string(),
                content: "persisted".to_string(),
                created_at: None,
                server_ref: None,
                adapter_ref: None,
            }],
            total_messages: 1,
            truncated: false,
        });

        assert!(app.chat.pending_user.is_none());
        assert!(app.chat.pending_assistant.is_none());
        assert_eq!(app.chat.transcript[0].content, "persisted");
    }

    #[test]
    fn stream_failure_exposes_manual_non_stream_retry_without_auto_send() {
        let mut app = TuiApp::test_app(PathBuf::from("/tmp/tentgent-tui-chat-fallback"));
        let (tx, _rx) = mpsc::unbounded_channel();
        app.chat_in_flight = Some(9);
        app.chat.send_state = ChatSendState::Streaming { request_id: 9 };
        app.generation = 4;
        let request = ChatSendRequest {
            request_id: 9,
            server_ref: "server".to_string(),
            session_ref: "session".to_string(),
            adapter_ref: None,
            prompt: "hello".to_string(),
            context_mode: ChatContextMode::Last10,
            max_session_messages: 10,
            stream: true,
        };

        app.handle_tui_event(
            TuiEvent::ChatSendError {
                request_id: 9,
                generation: 4,
                error: ChatError::StreamUnsupported("stream unsupported".to_string()),
                retry: Some(request.clone()),
                may_have_committed: false,
            },
            &tx,
        )
        .expect("event handled");

        assert_eq!(app.chat_in_flight, None);
        assert!(matches!(app.chat.send_state, ChatSendState::Error));
        assert_eq!(
            app.chat.retry_non_stream.as_ref().map(|retry| retry.stream),
            Some(false)
        );
        assert_eq!(
            app.chat
                .retry_non_stream
                .as_ref()
                .map(|retry| (retry.context_mode, retry.max_session_messages)),
            Some((ChatContextMode::Last10, 10))
        );
    }

    #[test]
    fn stale_refresh_result_is_ignored() {
        let mut app = TuiApp::test_app(PathBuf::from("/tmp/tentgent-tui-stale"));
        app.refresh_in_flight = Some(7);
        app.generation = 2;
        let (tx, _rx) = mpsc::unbounded_channel();
        let data = RefreshData {
            inspection: app.inspection.clone(),
            config: TentgentConfig::default(),
            config_error: None,
            daemon_url: app.daemon_url.clone(),
            daemon_token: app.daemon_token.clone(),
            daemon: DaemonSnapshot {
                state: DaemonConnectionState::Ready,
                detail: "stale ready".to_string(),
                status: None,
                doctor: None,
            },
            dashboard_counts: None,
        };

        app.handle_tui_event(
            TuiEvent::RefreshFinished {
                request_id: 7,
                generation: 1,
                result: Ok(data),
            },
            &tx,
        )
        .expect("event handled");

        assert_eq!(app.daemon.state, DaemonConnectionState::Down);
    }

    #[test]
    fn stale_resource_result_is_ignored_after_generation_change() {
        let mut app = TuiApp::test_app(PathBuf::from("/tmp/tentgent-tui-resource-stale"));
        app.resource_in_flight = Some(42);
        app.generation = 3;
        let (tx, _rx) = mpsc::unbounded_channel();
        let snapshot = collect_resource_snapshot(ResourceInputs::from_state(
            app.home.clone(),
            app.inspection.clone(),
            &app.navigator,
        ));

        app.handle_tui_event(
            TuiEvent::ResourceFinished {
                request_id: 42,
                generation: 2,
                result: Ok(snapshot),
            },
            &tx,
        )
        .expect("event handled");

        assert!(app.resources.snapshot.is_none());
        assert_eq!(app.resource_in_flight, Some(42));
    }

    #[tokio::test]
    async fn dashboard_refresh_does_not_trigger_resource_scan() {
        let mut app = TuiApp::test_app(PathBuf::from("/tmp/tentgent-tui-resource-dashboard"));
        let (tx, _rx) = mpsc::unbounded_channel();

        app.request_refresh(&tx, "dashboard refresh");

        assert!(app.refresh_in_flight.is_some());
        assert_eq!(app.resource_in_flight, None);
        assert!(matches!(app.resources.load_state, ResourceLoadState::Idle));
    }

    #[test]
    fn navigator_auth_error_enters_auth_required_bootstrap() {
        let mut app = TuiApp::test_app(PathBuf::from("/tmp/tentgent-tui-nav-auth"));

        app.apply_navigator_list_result(
            NavigatorListKind::Models,
            Err(NavigatorError::AuthRequired("token missing".to_string())),
        );

        assert_eq!(app.mode, AppMode::Bootstrap(BootstrapReason::AuthRequired));
        assert_eq!(app.daemon.state, DaemonConnectionState::AuthRequired);
    }

    #[test]
    fn navigator_404_marks_selected_item_stale_without_auth_transition() {
        let mut app = TuiApp::test_app(PathBuf::from("/tmp/tentgent-tui-nav-404"));
        app.daemon = DaemonSnapshot {
            state: DaemonConnectionState::Ready,
            detail: "ready".to_string(),
            status: None,
            doctor: None,
        };
        app.update_mode();

        app.apply_navigator_detail_result(
            NavigatorListKind::Models,
            "missing".to_string(),
            Err(NavigatorError::NotFound("missing".to_string())),
        );

        assert_eq!(app.mode, AppMode::Operator);
        assert!(matches!(
            app.navigator.state(NavigatorListKind::Models).load_state,
            NavigatorLoadState::StaleItem { .. }
        ));
    }

    #[test]
    fn filter_change_invalidates_in_flight_navigator_results() {
        let mut app = TuiApp::test_app(PathBuf::from("/tmp/tentgent-tui-filter"));
        app.navigator_in_flight = Some((9, NavigatorListKind::Models));
        app.detail_in_flight = Some((10, NavigatorListKind::Models, "model".to_string()));
        app.tail_in_flight = Some((11, NavigatorListKind::Servers));

        app.save_filter(NavigatorListKind::Models, "qwen".to_string());

        assert_eq!(app.navigator_in_flight, None);
        assert_eq!(app.detail_in_flight, None);
        assert_eq!(app.tail_in_flight, None);
        assert_eq!(
            app.navigator.state(NavigatorListKind::Models).filter,
            "qwen"
        );
    }

    #[test]
    fn training_tab_switch_stays_inside_training_navigator() {
        let mut app = TuiApp::test_app(PathBuf::from("/tmp/tentgent-tui-training"));
        app.daemon = DaemonSnapshot {
            state: DaemonConnectionState::Ready,
            detail: "ready".to_string(),
            status: None,
            doctor: None,
        };
        app.update_mode();
        app.selected_menu = app
            .menu_entries()
            .iter()
            .position(|entry| entry.item == MenuItem::Training)
            .expect("training entry");
        app.focus = FocusPane::Detail;

        app.navigator.training_tab.toggle();

        assert_eq!(
            app.current_navigator_kind(),
            Some(NavigatorListKind::TrainRuns)
        );
        assert_eq!(app.focus, FocusPane::Detail);
    }

    #[test]
    fn editing_token_prefers_session_source_and_never_persists() {
        let token = resolve_tui_daemon_token(Some("flag"), Some("env"), Some("session"));

        assert_eq!(token.source, TuiTokenSource::Session);
        assert_eq!(token.token.as_deref(), Some("session"));
    }

    #[test]
    fn text_editing_supports_cursor_insert_delete_and_arrows() {
        let mut value = "abcd".to_string();
        let mut cursor = 2;

        edit_text_value(&mut value, &mut cursor, KeyCode::Left);
        edit_text_value(&mut value, &mut cursor, KeyCode::Char('X'));
        assert_eq!(value, "aXbcd");
        assert_eq!(cursor, 2);

        edit_text_value(&mut value, &mut cursor, KeyCode::Right);
        edit_text_value(&mut value, &mut cursor, KeyCode::Backspace);
        assert_eq!(value, "aXcd");
        assert_eq!(cursor, 2);

        edit_text_value(&mut value, &mut cursor, KeyCode::Delete);
        assert_eq!(value, "aXd");
        assert_eq!(cursor, 2);
    }

    fn unique_home(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("tentgent-tui-{label}-{nanos}"))
    }
}
