use std::{
    env,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use miette::IntoDiagnostic;
use reqwest::Url;
use tentgent_core::{
    auth::{AuthManager, KeySource, Provider},
    config::{
        config_path, resolve_daemon_token, resolve_daemon_url, validate_daemon_url,
        DaemonTokenResolution, DaemonUrlInputs, DaemonUrlResolution, TentgentConfig,
        DAEMON_URL_ENV_VAR,
    },
    daemon::{DaemonInspection, DaemonManager, DEFAULT_DAEMON_PORT},
};
use tentgent_http::security::DAEMON_TOKEN_ENV_VAR;

use crate::cli::{
    commands::TuiCommand,
    daemon::{start_daemon_detached, DetachedDaemonOptions},
};

use super::{
    daemon_client::{DaemonClient, DaemonSnapshot},
    terminal::TerminalSession,
};

const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(100);
const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Screen {
    Status,
    Settings,
}

impl Screen {
    pub(super) fn index(self) -> usize {
        match self {
            Self::Status => 0,
            Self::Settings => 1,
        }
    }
}

#[derive(Clone)]
pub(super) struct ProviderAuthRow {
    pub(super) provider: Provider,
    pub(super) env_present: bool,
    pub(super) keychain_present: bool,
    pub(super) effective_source: Option<&'static str>,
    pub(super) note: String,
}

pub(super) enum InputMode {
    Normal,
    EditingDaemonUrl { value: String },
    EditingProviderKey { provider: Provider, value: String },
    ConfirmRemove { provider: Provider },
}

pub(super) struct TuiApp {
    pub(super) home: PathBuf,
    pub(super) config_path: PathBuf,
    pub(super) config: TentgentConfig,
    pub(super) config_error: Option<String>,
    pub(super) inspection: DaemonInspection,
    pub(super) daemon_url: DaemonUrlResolution,
    pub(super) daemon_token: DaemonTokenResolution,
    pub(super) daemon: DaemonSnapshot,
    pub(super) auth_rows: Vec<ProviderAuthRow>,
    pub(super) screen: Screen,
    pub(super) selected_provider: usize,
    pub(super) input_mode: InputMode,
    pub(super) message: String,
    pub(super) should_quit: bool,
    flag_daemon_url: Option<String>,
    flag_token: Option<String>,
}

impl TuiApp {
    pub(super) async fn new(command: TuiCommand) -> miette::Result<Self> {
        let manager = DaemonManager::new(command.home.as_deref()).into_diagnostic()?;
        let inspection = manager.status().into_diagnostic()?;
        let home = inspection.home_dir.clone();
        let config_path = config_path(&home);
        let mut app = Self {
            home,
            config_path,
            config: TentgentConfig::default(),
            config_error: None,
            inspection,
            daemon_url: resolve_daemon_url(DaemonUrlInputs {
                flag_url: command.daemon_url.as_deref(),
                env_url: env_string(DAEMON_URL_ENV_VAR).as_deref(),
                config_url: None,
                metadata: None,
            }),
            daemon_token: resolve_daemon_token(
                command.token.as_deref(),
                env_string(DAEMON_TOKEN_ENV_VAR).as_deref(),
            ),
            daemon: DaemonSnapshot::idle(),
            auth_rows: Vec::new(),
            screen: Screen::Status,
            selected_provider: 0,
            input_mode: InputMode::Normal,
            message: "r refresh | s start daemon | tab switch | q quit".to_string(),
            should_quit: false,
            flag_daemon_url: command.daemon_url,
            flag_token: command.token,
        };
        app.refresh().await?;
        Ok(app)
    }

    pub(super) async fn refresh(&mut self) -> miette::Result<()> {
        let manager = DaemonManager::new(Some(&self.home)).into_diagnostic()?;
        self.inspection = manager.status().into_diagnostic()?;
        self.config = match TentgentConfig::load(&self.home) {
            Ok(config) => {
                self.config_error = None;
                config
            }
            Err(error) => {
                self.config_error = Some(error.to_string());
                TentgentConfig::default()
            }
        };
        self.resolve_daemon_endpoint();
        self.refresh_local_auth();
        let client = DaemonClient::new(
            self.daemon_url.url.clone(),
            self.daemon_token.token.clone(),
            self.daemon_token.source,
        )?;
        self.daemon = client.refresh().await;
        Ok(())
    }

    pub(super) async fn handle_event(&mut self, event: Event) -> miette::Result<()> {
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                self.handle_key(key).await?;
            }
        }
        Ok(())
    }

    pub(super) fn active_input_label(&self) -> Option<String> {
        match &self.input_mode {
            InputMode::Normal => None,
            InputMode::EditingDaemonUrl { value } => Some(format!("daemon url: {value}")),
            InputMode::EditingProviderKey { provider, value } => Some(format!(
                "{} key: {}",
                provider.display_name(),
                mask_secret(value)
            )),
            InputMode::ConfirmRemove { provider } => Some(format!(
                "remove {} keychain entry? y/N",
                provider.display_name()
            )),
        }
    }

    pub(super) fn selected_provider(&self) -> Provider {
        Provider::ALL[self.selected_provider.min(Provider::ALL.len() - 1)]
    }

    pub(super) fn detached_start_options(&self) -> miette::Result<DetachedDaemonOptions> {
        let bind = daemon_start_bind_from_url(&self.daemon_url.url)?;
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
            Err(error) => format!(
                "cannot build start command from daemon URL `{}`: {error}",
                self.daemon_url.url
            ),
        }
    }

    async fn handle_key(&mut self, key: KeyEvent) -> miette::Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return Ok(());
        }

        let mode = std::mem::replace(&mut self.input_mode, InputMode::Normal);
        match mode {
            InputMode::Normal => self.handle_normal_key(key).await?,
            InputMode::EditingDaemonUrl { mut value } => match key.code {
                KeyCode::Enter => {
                    let next = value.trim().to_string();
                    self.save_daemon_url(next).await?;
                }
                KeyCode::Esc => {
                    self.message = "daemon URL edit canceled".to_string();
                }
                KeyCode::Backspace => {
                    value.pop();
                    self.input_mode = InputMode::EditingDaemonUrl { value };
                }
                KeyCode::Char(ch) => {
                    value.push(ch);
                    self.input_mode = InputMode::EditingDaemonUrl { value };
                }
                _ => {}
            },
            InputMode::EditingProviderKey {
                provider,
                mut value,
            } => match key.code {
                KeyCode::Enter => {
                    self.set_provider_key(provider, value)?;
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
                _ => {}
            },
            InputMode::ConfirmRemove { provider } => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    self.remove_provider_key(provider)?;
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.message = "provider key removal canceled".to_string();
                }
                _ => self.input_mode = InputMode::ConfirmRemove { provider },
            },
        }

        Ok(())
    }

    async fn handle_normal_key(&mut self, key: KeyEvent) -> miette::Result<()> {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Tab | KeyCode::Right => self.screen = Screen::Settings,
            KeyCode::Left => self.screen = Screen::Status,
            KeyCode::Char('1') => self.screen = Screen::Status,
            KeyCode::Char('2') => self.screen = Screen::Settings,
            KeyCode::Char('r') => {
                self.message = "refreshing daemon status".to_string();
                self.refresh().await?;
            }
            KeyCode::Char('s') => self.start_daemon().await?,
            KeyCode::Char('u') => {
                self.screen = Screen::Settings;
                let value = self
                    .config
                    .daemon
                    .url
                    .clone()
                    .unwrap_or_else(|| self.daemon_url.url.clone());
                self.input_mode = InputMode::EditingDaemonUrl { value };
                self.message = "enter saves daemon URL preference; esc cancels".to_string();
            }
            KeyCode::Char('k') => {
                self.screen = Screen::Settings;
                self.input_mode = InputMode::EditingProviderKey {
                    provider: self.selected_provider(),
                    value: String::new(),
                };
                self.message =
                    "provider key input is masked and stored only in Keychain".to_string();
            }
            KeyCode::Char('x') => {
                self.screen = Screen::Settings;
                self.input_mode = InputMode::ConfirmRemove {
                    provider: self.selected_provider(),
                };
            }
            KeyCode::Down => {
                self.screen = Screen::Settings;
                self.selected_provider = (self.selected_provider + 1).min(Provider::ALL.len() - 1);
            }
            KeyCode::Up => {
                self.screen = Screen::Settings;
                self.selected_provider = self.selected_provider.saturating_sub(1);
            }
            _ => {}
        }
        Ok(())
    }

    async fn start_daemon(&mut self) -> miette::Result<()> {
        self.message = "starting detached daemon".to_string();
        let outcome = start_daemon_detached(self.detached_start_options()?).await?;
        self.message = if outcome.already_running {
            format!("daemon already running at {}", outcome.daemon_url)
        } else {
            format!("daemon started at {}", outcome.daemon_url)
        };
        if let Some(warning) = outcome.status_warning {
            self.message = format!("{}; {warning}", self.message);
        }
        self.refresh().await?;
        Ok(())
    }

    async fn save_daemon_url(&mut self, value: String) -> miette::Result<()> {
        validate_daemon_url(&value, "tui config").map_err(|error| miette::miette!("{error}"))?;
        self.config.daemon.url = Some(value.clone());
        self.config.save(&self.home).into_diagnostic()?;
        self.message = format!("saved daemon URL preference: {value}");
        self.refresh().await?;
        Ok(())
    }

    fn set_provider_key(&mut self, provider: Provider, secret: String) -> miette::Result<()> {
        if secret.trim().is_empty() {
            self.message = format!("{} key was empty; nothing saved", provider.display_name());
            return Ok(());
        }
        let auth = AuthManager::new().into_diagnostic()?;
        auth.set_key(provider, secret.trim()).into_diagnostic()?;
        self.refresh_local_auth();
        let env_note = if env_string(provider.env_var()).is_some() {
            "; env overrides keychain for the effective key"
        } else {
            ""
        };
        self.message = format!("saved {} keychain entry{env_note}", provider.display_name());
        Ok(())
    }

    fn remove_provider_key(&mut self, provider: Provider) -> miette::Result<()> {
        let auth = AuthManager::new().into_diagnostic()?;
        let removed = auth.remove_key(provider).into_diagnostic()?;
        self.refresh_local_auth();
        self.message = if removed {
            format!("removed {} keychain entry", provider.display_name())
        } else {
            format!("no {} keychain entry was present", provider.display_name())
        };
        Ok(())
    }

    fn resolve_daemon_endpoint(&mut self) {
        let env_url = env_string(DAEMON_URL_ENV_VAR);
        let env_token = env_string(DAEMON_TOKEN_ENV_VAR);
        self.daemon_url = resolve_daemon_url(DaemonUrlInputs {
            flag_url: self.flag_daemon_url.as_deref(),
            env_url: env_url.as_deref(),
            config_url: self.config.daemon.url.as_deref(),
            metadata: self.inspection.process.as_ref(),
        });
        if self.config_error.is_none() {
            self.config_error = self.daemon_url.config_error.clone();
        }
        self.daemon_token = resolve_daemon_token(self.flag_token.as_deref(), env_token.as_deref());
    }

    fn refresh_local_auth(&mut self) {
        self.auth_rows.clear();
        let auth = match AuthManager::new() {
            Ok(auth) => auth,
            Err(error) => {
                self.message = format!("failed to initialize local auth manager: {error}");
                return;
            }
        };
        for provider in Provider::ALL {
            match auth.local_key_status(provider) {
                Ok(status) => {
                    let effective_source = status.effective_source.map(|source| match source {
                        KeySource::Env => "env",
                        KeySource::Keychain => "keychain",
                    });
                    let note = if status.env_present {
                        "env overrides keychain".to_string()
                    } else if status.keychain_present {
                        "stored in keychain".to_string()
                    } else {
                        "missing".to_string()
                    };
                    self.auth_rows.push(ProviderAuthRow {
                        provider,
                        env_present: status.env_present,
                        keychain_present: status.keychain_present,
                        effective_source,
                        note,
                    });
                }
                Err(error) => self.auth_rows.push(ProviderAuthRow {
                    provider,
                    env_present: false,
                    keychain_present: false,
                    effective_source: None,
                    note: format!("auth status failed: {error}"),
                }),
            }
        }
    }
}

pub(super) async fn run_tui(command: TuiCommand) -> miette::Result<()> {
    let mut app = TuiApp::new(command).await?;
    let mut terminal = TerminalSession::enter()?;
    let mut last_refresh = Instant::now();

    loop {
        terminal.draw(&app)?;
        if app.should_quit {
            break;
        }

        if event::poll(EVENT_POLL_INTERVAL).into_diagnostic()? {
            let event = event::read().into_diagnostic()?;
            app.handle_event(event).await?;
        }

        if last_refresh.elapsed() >= AUTO_REFRESH_INTERVAL {
            app.refresh().await?;
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

fn env_string(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn shell_single_quote(path: &Path) -> String {
    let value = path.display().to_string();
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DaemonStartBind {
    host: String,
    port: u16,
}

fn daemon_start_bind_from_url(daemon_url: &str) -> miette::Result<DaemonStartBind> {
    let parsed = Url::parse(daemon_url)
        .map_err(|error| miette::miette!("invalid daemon URL `{daemon_url}`: {error}"))?;
    if parsed.scheme() != "http" {
        return Err(miette::miette!(
            "TUI can only start the local HTTP daemon from an http URL; got `{}`",
            parsed.scheme()
        ));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| miette::miette!("daemon URL `{daemon_url}` is missing a host"))?
        .to_string();
    let port = parsed
        .port_or_known_default()
        .unwrap_or(DEFAULT_DAEMON_PORT);
    Ok(DaemonStartBind { host, port })
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
    fn tui_start_action_builds_shared_detached_options_from_resolved_url() {
        let home = PathBuf::from("/tmp/tentgent-tui-home");
        let app = TuiApp {
            home: home.clone(),
            config_path: config_path(&home),
            config: TentgentConfig::default(),
            config_error: None,
            inspection: stopped_inspection(&home),
            daemon_url: resolve_daemon_url(DaemonUrlInputs {
                flag_url: Some("http://127.0.0.1:18791"),
                env_url: None,
                config_url: None,
                metadata: None,
            }),
            daemon_token: resolve_daemon_token(None, None),
            daemon: DaemonSnapshot::idle(),
            auth_rows: Vec::new(),
            screen: Screen::Status,
            selected_provider: 0,
            input_mode: InputMode::Normal,
            message: String::new(),
            should_quit: false,
            flag_daemon_url: None,
            flag_token: None,
        };

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
    fn tui_start_action_rejects_https_daemon_url() {
        let error = daemon_start_bind_from_url("https://127.0.0.1:8790")
            .expect_err("https cannot be started as local daemon bind");

        assert!(error.to_string().contains("http URL"));
    }

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
}
