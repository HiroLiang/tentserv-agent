use std::{
    env,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::Value;
use tentgent_core::{
    adapter::AdapterStorePaths, daemon::DaemonInspection, dataset::DatasetStorePaths,
    session::SessionStorePaths,
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use walkdir::WalkDir;

use super::super::display::format_bytes;
use super::navigator::{NavigatorListKind, NavigatorRow, NavigatorState};

pub(super) const MAX_RESOURCE_SCAN_ENTRIES: usize = 25_000;
pub(super) const MAX_RESOURCE_SCAN_DURATION: Duration = Duration::from_secs(2);
const PROCESS_PROBE_TIMEOUT: Duration = Duration::from_millis(500);
const DISK_PROBE_TIMEOUT: Duration = Duration::from_millis(500);
const LARGE_LOG_BYTES: u64 = 100 * 1024 * 1024;
const LARGE_CATEGORY_BYTES: u64 = 10 * 1024 * 1024 * 1024;
const LOW_DISK_BYTES: u64 = 5 * 1024 * 1024 * 1024;
const LOW_DISK_RATIO: f64 = 0.10;
const LONG_RUNNING_TRAIN_SECS: i64 = 6 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResourceTab {
    Storage,
    Processes,
    Warnings,
}

impl ResourceTab {
    pub(super) fn toggle(&mut self) {
        *self = match self {
            Self::Storage => Self::Processes,
            Self::Processes => Self::Warnings,
            Self::Warnings => Self::Storage,
        };
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Storage => "Storage",
            Self::Processes => "Processes",
            Self::Warnings => "Warnings",
        }
    }
}

impl Default for ResourceTab {
    fn default() -> Self {
        Self::Storage
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ResourceLoadState {
    Idle,
    Loading { request_id: u64 },
    Ready,
    Error { message: String, stale: bool },
}

impl ResourceLoadState {
    pub(super) fn label(&self) -> String {
        match self {
            Self::Idle => "not scanned".to_string(),
            Self::Loading { .. } => "scanning".to_string(),
            Self::Ready => "ready".to_string(),
            Self::Error { message, stale } => {
                if *stale {
                    format!("stale; {message}")
                } else {
                    message.clone()
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ResourceState {
    pub(super) snapshot: Option<ResourceSnapshot>,
    pub(super) tab: ResourceTab,
    pub(super) filter: String,
    pub(super) load_state: ResourceLoadState,
}

impl Default for ResourceState {
    fn default() -> Self {
        Self {
            snapshot: None,
            tab: ResourceTab::Storage,
            filter: String::new(),
            load_state: ResourceLoadState::Idle,
        }
    }
}

impl ResourceState {
    pub(super) fn set_filter(&mut self, filter: String) {
        self.filter = filter;
    }
}

#[derive(Debug, Clone)]
pub(super) struct ResourceSnapshot {
    pub(super) storage_rows: Vec<StorageRow>,
    pub(super) process_rows: Vec<ProcessRow>,
    pub(super) warnings: Vec<ResourceWarning>,
    pub(super) disk: DiskSummary,
    pub(super) partial: bool,
    pub(super) scan_duration_ms: u128,
    pub(super) scanned_files: usize,
    pub(super) skipped_unreadable: usize,
    pub(super) last_refreshed: String,
}

impl ResourceSnapshot {
    pub(super) fn storage_total_bytes(&self) -> u64 {
        self.storage_rows.iter().map(|row| row.total_bytes).sum()
    }

    pub(super) fn visible_storage_rows(&self, filter: &str) -> Vec<&StorageRow> {
        let filter = filter.trim().to_ascii_lowercase();
        self.storage_rows
            .iter()
            .filter(|row| filter.is_empty() || row.search_text().contains(&filter))
            .collect()
    }

    pub(super) fn visible_process_rows(&self, filter: &str) -> Vec<&ProcessRow> {
        let filter = filter.trim().to_ascii_lowercase();
        self.process_rows
            .iter()
            .filter(|row| filter.is_empty() || row.search_text().contains(&filter))
            .collect()
    }

    pub(super) fn visible_warnings(&self, filter: &str) -> Vec<&ResourceWarning> {
        let filter = filter.trim().to_ascii_lowercase();
        self.warnings
            .iter()
            .filter(|warning| filter.is_empty() || warning.search_text().contains(&filter))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub(super) struct StorageRow {
    pub(super) category: String,
    pub(super) path: PathBuf,
    pub(super) exists: bool,
    pub(super) total_bytes: u64,
    pub(super) file_count: usize,
    pub(super) scanned_files: usize,
    pub(super) skipped_unreadable: usize,
    pub(super) partial: bool,
    pub(super) largest_file: Option<ResourceFileSummary>,
}

impl StorageRow {
    fn search_text(&self) -> String {
        format!(
            "{} {} {} {}",
            self.category,
            self.path.display(),
            format_bytes(self.total_bytes),
            self.largest_file
                .as_ref()
                .map(|file| file.path.display().to_string())
                .unwrap_or_default()
        )
        .to_ascii_lowercase()
    }
}

#[derive(Debug, Clone)]
pub(super) struct ResourceFileSummary {
    pub(super) path: PathBuf,
    pub(super) bytes: u64,
}

#[derive(Debug, Clone)]
pub(super) struct ProcessRow {
    pub(super) source: String,
    pub(super) ref_label: String,
    pub(super) pid: Option<u32>,
    pub(super) state: String,
    pub(super) rss_kib: Option<u64>,
    pub(super) cpu_percent: Option<f32>,
    pub(super) identity: ProcessIdentity,
    pub(super) port_or_source: String,
    pub(super) detail: String,
}

impl ProcessRow {
    fn search_text(&self) -> String {
        format!(
            "{} {} {} {} {} {}",
            self.source,
            self.ref_label,
            self.pid
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "-".to_string()),
            self.state,
            self.identity.label(),
            self.detail
        )
        .to_ascii_lowercase()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ProcessIdentity {
    Verified,
    ExistsUnverified,
    Missing,
    Unavailable,
}

impl ProcessIdentity {
    pub(super) fn label(&self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::ExistsUnverified => "pid exists / identity unverified",
            Self::Missing => "missing",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct DiskSummary {
    pub(super) path: PathBuf,
    pub(super) total_bytes: Option<u64>,
    pub(super) available_bytes: Option<u64>,
    pub(super) used_percent: Option<f64>,
    pub(super) state: DiskState,
    pub(super) detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DiskState {
    Healthy,
    Low,
    Unknown,
}

#[derive(Debug, Clone)]
pub(super) struct ResourceWarning {
    pub(super) level: WarningLevel,
    pub(super) source: String,
    pub(super) message: String,
    pub(super) detail: String,
}

impl ResourceWarning {
    fn search_text(&self) -> String {
        format!(
            "{} {} {} {}",
            self.level.label(),
            self.source,
            self.message,
            self.detail
        )
        .to_ascii_lowercase()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WarningLevel {
    Warn,
    Info,
}

impl WarningLevel {
    pub(super) fn label(&self) -> &'static str {
        match self {
            Self::Warn => "warn",
            Self::Info => "info",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ResourceInputs {
    home: PathBuf,
    inspection: DaemonInspection,
    server_rows: Vec<NavigatorRow>,
    train_run_rows: Vec<NavigatorRow>,
}

impl ResourceInputs {
    pub(super) fn from_state(
        home: PathBuf,
        inspection: DaemonInspection,
        navigator: &NavigatorState,
    ) -> Self {
        Self {
            home,
            inspection,
            server_rows: navigator.state(NavigatorListKind::Servers).rows.clone(),
            train_run_rows: navigator.state(NavigatorListKind::TrainRuns).rows.clone(),
        }
    }
}

pub(super) fn collect_resource_snapshot(inputs: ResourceInputs) -> ResourceSnapshot {
    let started = Instant::now();
    let mut budget = ScanBudget::new(MAX_RESOURCE_SCAN_ENTRIES, MAX_RESOURCE_SCAN_DURATION);
    let storage_rows = storage_categories(&inputs.home, &inputs.inspection)
        .into_iter()
        .map(|category| scan_storage_row(category, &mut budget))
        .collect::<Vec<_>>();
    let process_rows = collect_process_rows(&inputs);
    let disk = disk_summary(&inputs.home);
    let mut warnings = build_warnings(&storage_rows, &process_rows, &disk, &inputs.train_run_rows);

    if budget.hit_limit() {
        warnings.push(ResourceWarning {
            level: WarningLevel::Info,
            source: "scanner".to_string(),
            message: "resource scan budget reached".to_string(),
            detail: "storage totals are partial".to_string(),
        });
    }

    ResourceSnapshot {
        partial: storage_rows.iter().any(|row| row.partial) || budget.hit_limit(),
        scan_duration_ms: started.elapsed().as_millis(),
        scanned_files: storage_rows.iter().map(|row| row.scanned_files).sum(),
        skipped_unreadable: storage_rows.iter().map(|row| row.skipped_unreadable).sum(),
        storage_rows,
        process_rows,
        warnings,
        disk,
        last_refreshed: now_label(),
    }
}

#[derive(Debug, Clone)]
struct StorageCategory {
    label: &'static str,
    path: PathBuf,
}

fn storage_categories(home: &Path, inspection: &DaemonInspection) -> Vec<StorageCategory> {
    vec![
        StorageCategory {
            label: "models",
            path: env_path("TENTGENT_MODELS_DIR").unwrap_or_else(|| home.join("models")),
        },
        StorageCategory {
            label: "adapters",
            path: AdapterStorePaths::resolve_with_home(Some(home))
                .map(|paths| paths.adapters_dir)
                .unwrap_or_else(|_| home.join("adapters")),
        },
        StorageCategory {
            label: "datasets",
            path: DatasetStorePaths::resolve_with_home(Some(home))
                .map(|paths| paths.datasets_dir)
                .unwrap_or_else(|_| home.join("datasets")),
        },
        StorageCategory {
            label: "sessions",
            path: SessionStorePaths::resolve(Some(home))
                .map(|paths| paths.sessions_dir)
                .unwrap_or_else(|_| home.join("sessions")),
        },
        StorageCategory {
            label: "servers",
            path: home.join("servers"),
        },
        StorageCategory {
            label: "logs",
            path: inspection.log_dir.clone(),
        },
        StorageCategory {
            label: "runtime",
            path: inspection.runtime_dir.clone(),
        },
        StorageCategory {
            label: "training",
            path: env_path("TENTGENT_TRAIN_DIR").unwrap_or_else(|| home.join("train")),
        },
    ]
}

fn scan_storage_row(category: StorageCategory, budget: &mut ScanBudget) -> StorageRow {
    let path = category.path;
    let exists = path.exists();
    let mut row = StorageRow {
        category: category.label.to_string(),
        path: path.clone(),
        exists,
        total_bytes: 0,
        file_count: 0,
        scanned_files: 0,
        skipped_unreadable: 0,
        partial: false,
        largest_file: None,
    };
    if !exists {
        return row;
    }
    if budget.exhausted() {
        row.partial = true;
        return row;
    }

    for entry in WalkDir::new(&path).follow_links(false).into_iter() {
        if budget.exhausted() {
            row.partial = true;
            break;
        }
        budget.record_entry();
        row.scanned_files = row.scanned_files.saturating_add(1);
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                row.skipped_unreadable = row.skipped_unreadable.saturating_add(1);
                continue;
            }
        };
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => {
                row.skipped_unreadable = row.skipped_unreadable.saturating_add(1);
                continue;
            }
        };
        let bytes = metadata.len();
        row.file_count = row.file_count.saturating_add(1);
        row.total_bytes = row.total_bytes.saturating_add(bytes);
        if row
            .largest_file
            .as_ref()
            .map(|largest| bytes > largest.bytes)
            .unwrap_or(true)
        {
            row.largest_file = Some(ResourceFileSummary {
                path: entry.path().to_path_buf(),
                bytes,
            });
        }
    }
    row
}

#[derive(Debug, Clone)]
struct ScanBudget {
    max_entries: usize,
    deadline: Instant,
    entries: usize,
    hit_limit: bool,
}

impl ScanBudget {
    fn new(max_entries: usize, duration: Duration) -> Self {
        Self {
            max_entries,
            deadline: Instant::now() + duration,
            entries: 0,
            hit_limit: false,
        }
    }

    fn record_entry(&mut self) {
        self.entries = self.entries.saturating_add(1);
    }

    fn exhausted(&mut self) -> bool {
        if self.entries >= self.max_entries || Instant::now() >= self.deadline {
            self.hit_limit = true;
            return true;
        }
        false
    }

    fn hit_limit(&self) -> bool {
        self.hit_limit || self.entries >= self.max_entries || Instant::now() >= self.deadline
    }
}

fn collect_process_rows(inputs: &ResourceInputs) -> Vec<ProcessRow> {
    let mut rows = Vec::new();
    rows.push(daemon_process_row(&inputs.inspection));
    rows.extend(inputs.server_rows.iter().map(server_process_row));
    rows.extend(inputs.train_run_rows.iter().map(train_run_process_row));
    rows
}

fn daemon_process_row(inspection: &DaemonInspection) -> ProcessRow {
    let pid = inspection.process.as_ref().map(|process| process.pid);
    let mut row = process_row_from_probe("daemon", "daemon", pid, Some("tentgent"));
    row.state = if inspection.running {
        "running".to_string()
    } else {
        "stopped".to_string()
    };
    row.port_or_source = inspection
        .process
        .as_ref()
        .map(|process| format!("{}:{}", process.host, process.port))
        .unwrap_or_else(|| "-".to_string());
    row.detail = inspection.process_path.display().to_string();
    row
}

fn server_process_row(row: &NavigatorRow) -> ProcessRow {
    let pid = nested_u64(&row.raw, &["process", "pid"])
        .or_else(|| u64_field(&row.raw, "pid"))
        .and_then(|value| u32::try_from(value).ok());
    let mut process = process_row_from_probe("server", &row.short_ref, pid, None);
    process.state = bool_field(&row.raw, "running")
        .map(|running| if running { "running" } else { "stopped" })
        .unwrap_or_else(|| string_field(&row.raw, "state").unwrap_or("unknown"))
        .to_string();
    process.port_or_source = u64_field(&row.raw, "port")
        .map(|port| port.to_string())
        .unwrap_or_else(|| {
            string_field(&row.raw, "runtime_kind")
                .unwrap_or("-")
                .to_string()
        });
    process.detail = first_nonempty(&[
        string_field(&row.raw, "server_ref"),
        string_field(&row.raw, "model_ref"),
        string_field(&row.raw, "provider_model"),
    ]);
    process
}

fn train_run_process_row(row: &NavigatorRow) -> ProcessRow {
    let pid = u64_field(&row.raw, "pid").and_then(|value| u32::try_from(value).ok());
    let mut process = process_row_from_probe("train", &row.short_ref, pid, None);
    process.state = string_field(&row.raw, "status")
        .unwrap_or("unknown")
        .to_string();
    process.port_or_source = string_field(&row.raw, "backend").unwrap_or("-").to_string();
    process.detail = first_nonempty(&[
        string_field(&row.raw, "run_ref"),
        string_field(&row.raw, "phase"),
        string_field(&row.raw, "plan_ref"),
    ]);
    process
}

fn process_row_from_probe(
    source: &str,
    ref_label: &str,
    pid: Option<u32>,
    expected_identity: Option<&str>,
) -> ProcessRow {
    let Some(pid) = pid else {
        return ProcessRow {
            source: source.to_string(),
            ref_label: ref_label.to_string(),
            pid: None,
            state: "unknown".to_string(),
            rss_kib: None,
            cpu_percent: None,
            identity: ProcessIdentity::Missing,
            port_or_source: "-".to_string(),
            detail: "no pid metadata".to_string(),
        };
    };
    match probe_process(pid, expected_identity) {
        ProcessProbe::Found {
            rss_kib,
            cpu_percent,
            identity,
            command,
        } => ProcessRow {
            source: source.to_string(),
            ref_label: ref_label.to_string(),
            pid: Some(pid),
            state: "unknown".to_string(),
            rss_kib,
            cpu_percent,
            identity,
            port_or_source: "-".to_string(),
            detail: command,
        },
        ProcessProbe::Missing => ProcessRow {
            source: source.to_string(),
            ref_label: ref_label.to_string(),
            pid: Some(pid),
            state: "unknown".to_string(),
            rss_kib: None,
            cpu_percent: None,
            identity: ProcessIdentity::Missing,
            port_or_source: "-".to_string(),
            detail: "pid not found".to_string(),
        },
        ProcessProbe::Unavailable(message) => ProcessRow {
            source: source.to_string(),
            ref_label: ref_label.to_string(),
            pid: Some(pid),
            state: "unknown".to_string(),
            rss_kib: None,
            cpu_percent: None,
            identity: ProcessIdentity::Unavailable,
            port_or_source: "-".to_string(),
            detail: message,
        },
    }
}

#[derive(Debug, Clone)]
enum ProcessProbe {
    Found {
        rss_kib: Option<u64>,
        cpu_percent: Option<f32>,
        identity: ProcessIdentity,
        command: String,
    },
    Missing,
    Unavailable(String),
}

fn probe_process(pid: u32, expected_identity: Option<&str>) -> ProcessProbe {
    #[cfg(unix)]
    {
        let output = match run_command_with_timeout(
            "ps",
            &["-o", "pid=,rss=,pcpu=,comm=", "-p", &pid.to_string()],
            PROCESS_PROBE_TIMEOUT,
        ) {
            Ok(output) => output,
            Err(CommandProbeError::Status) => return ProcessProbe::Missing,
            Err(error) => return ProcessProbe::Unavailable(error.to_string()),
        };
        parse_ps_output(&output, expected_identity).unwrap_or(ProcessProbe::Missing)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        let _ = expected_identity;
        ProcessProbe::Unavailable("process probe unavailable on this platform".to_string())
    }
}

fn parse_ps_output(output: &str, expected_identity: Option<&str>) -> Option<ProcessProbe> {
    let line = output.lines().find(|line| !line.trim().is_empty())?;
    let mut parts = line.split_whitespace();
    let _pid = parts.next()?;
    let rss_kib = parts.next().and_then(|value| value.parse::<u64>().ok());
    let cpu_percent = parts.next().and_then(|value| value.parse::<f32>().ok());
    let command = parts.collect::<Vec<_>>().join(" ");
    let identity = if let Some(expected) = expected_identity {
        if command.contains(expected) {
            ProcessIdentity::Verified
        } else {
            ProcessIdentity::ExistsUnverified
        }
    } else {
        ProcessIdentity::ExistsUnverified
    };
    Some(ProcessProbe::Found {
        rss_kib,
        cpu_percent,
        identity,
        command,
    })
}

fn disk_summary(home: &Path) -> DiskSummary {
    #[cfg(unix)]
    {
        match run_command_with_timeout(
            "df",
            &["-Pk", &home.display().to_string()],
            DISK_PROBE_TIMEOUT,
        ) {
            Ok(output) => parse_df_output(home, &output),
            Err(error) => DiskSummary {
                path: home.to_path_buf(),
                total_bytes: None,
                available_bytes: None,
                used_percent: None,
                state: DiskState::Unknown,
                detail: error.to_string(),
            },
        }
    }
    #[cfg(not(unix))]
    {
        DiskSummary {
            path: home.to_path_buf(),
            total_bytes: None,
            available_bytes: None,
            used_percent: None,
            state: DiskState::Unknown,
            detail: "disk probe unavailable on this platform".to_string(),
        }
    }
}

fn parse_df_output(home: &Path, output: &str) -> DiskSummary {
    let Some(line) = output.lines().filter(|line| !line.trim().is_empty()).nth(1) else {
        return DiskSummary {
            path: home.to_path_buf(),
            total_bytes: None,
            available_bytes: None,
            used_percent: None,
            state: DiskState::Unknown,
            detail: "df output did not include a filesystem row".to_string(),
        };
    };
    let columns = line.split_whitespace().collect::<Vec<_>>();
    if columns.len() < 5 {
        return DiskSummary {
            path: home.to_path_buf(),
            total_bytes: None,
            available_bytes: None,
            used_percent: None,
            state: DiskState::Unknown,
            detail: "df output was not parseable".to_string(),
        };
    }
    let total_bytes = columns
        .get(1)
        .and_then(|value| value.parse::<u64>().ok())
        .map(|kib| kib.saturating_mul(1024));
    let available_bytes = columns
        .get(3)
        .and_then(|value| value.parse::<u64>().ok())
        .map(|kib| kib.saturating_mul(1024));
    let used_percent = columns
        .get(4)
        .and_then(|value| value.trim_end_matches('%').parse::<f64>().ok());
    let state = classify_disk(total_bytes, available_bytes);
    DiskSummary {
        path: home.to_path_buf(),
        total_bytes,
        available_bytes,
        used_percent,
        state,
        detail: columns[0].to_string(),
    }
}

fn classify_disk(total_bytes: Option<u64>, available_bytes: Option<u64>) -> DiskState {
    let (Some(total), Some(available)) = (total_bytes, available_bytes) else {
        return DiskState::Unknown;
    };
    if available < LOW_DISK_BYTES || (available as f64 / total.max(1) as f64) < LOW_DISK_RATIO {
        DiskState::Low
    } else {
        DiskState::Healthy
    }
}

#[derive(Debug, Clone)]
enum CommandProbeError {
    Spawn(String),
    Timeout,
    Status,
    Read(String),
}

impl std::fmt::Display for CommandProbeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(error) => write!(formatter, "probe spawn failed: {error}"),
            Self::Timeout => formatter.write_str("probe timed out"),
            Self::Status => formatter.write_str("probe returned non-zero status"),
            Self::Read(error) => write!(formatter, "probe output read failed: {error}"),
        }
    }
}

fn run_command_with_timeout(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<String, CommandProbeError> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| CommandProbeError::Spawn(error.to_string()))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut output = String::new();
                if let Some(mut stdout) = child.stdout.take() {
                    stdout
                        .read_to_string(&mut output)
                        .map_err(|error| CommandProbeError::Read(error.to_string()))?;
                }
                return if status.success() {
                    Ok(output)
                } else {
                    Err(CommandProbeError::Status)
                };
            }
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(CommandProbeError::Timeout);
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => return Err(CommandProbeError::Spawn(error.to_string())),
        }
    }
}

fn build_warnings(
    storage_rows: &[StorageRow],
    process_rows: &[ProcessRow],
    disk: &DiskSummary,
    train_run_rows: &[NavigatorRow],
) -> Vec<ResourceWarning> {
    let mut warnings = Vec::new();
    if disk.state == DiskState::Low {
        warnings.push(ResourceWarning {
            level: WarningLevel::Warn,
            source: "disk".to_string(),
            message: "runtime filesystem is low on free space".to_string(),
            detail: format!(
                "available {} of {}",
                format_bytes(disk.available_bytes.unwrap_or(0)),
                format_bytes(disk.total_bytes.unwrap_or(0))
            ),
        });
    } else if disk.state == DiskState::Unknown {
        warnings.push(ResourceWarning {
            level: WarningLevel::Info,
            source: "disk".to_string(),
            message: "disk free space unavailable".to_string(),
            detail: disk.detail.clone(),
        });
    }

    for row in storage_rows {
        if row.total_bytes > LARGE_CATEGORY_BYTES {
            warnings.push(ResourceWarning {
                level: WarningLevel::Warn,
                source: row.category.clone(),
                message: "storage category exceeds 10 GiB".to_string(),
                detail: format!(
                    "{} at {}",
                    format_bytes(row.total_bytes),
                    row.path.display()
                ),
            });
        }
        if row.category == "logs" {
            if let Some(file) = &row.largest_file {
                if file.bytes > LARGE_LOG_BYTES {
                    warnings.push(ResourceWarning {
                        level: WarningLevel::Warn,
                        source: "logs".to_string(),
                        message: "large log file exceeds 100 MiB".to_string(),
                        detail: format!("{} {}", format_bytes(file.bytes), file.path.display()),
                    });
                }
            }
        }
    }

    for row in process_rows {
        if row.pid.is_some() && row.identity == ProcessIdentity::Missing {
            warnings.push(ResourceWarning {
                level: WarningLevel::Warn,
                source: row.source.clone(),
                message: "pid metadata exists but process is missing".to_string(),
                detail: format!(
                    "{} pid {}",
                    row.ref_label,
                    row.pid
                        .map(|pid| pid.to_string())
                        .unwrap_or_else(|| "-".to_string())
                ),
            });
        }
    }

    for row in train_run_rows {
        if let Some(warning) = train_run_age_warning(row) {
            warnings.push(warning);
        }
    }

    warnings
}

fn train_run_age_warning(row: &NavigatorRow) -> Option<ResourceWarning> {
    let active = matches!(
        string_field(&row.raw, "status"),
        Some("running" | "starting" | "queued")
    ) || bool_field(&row.raw, "process_running").unwrap_or(false)
        || bool_field(&row.raw, "stale").unwrap_or(false);
    if !active {
        return None;
    }
    if bool_field(&row.raw, "stale").unwrap_or(false) {
        return Some(ResourceWarning {
            level: WarningLevel::Warn,
            source: "training".to_string(),
            message: "train run is marked stale".to_string(),
            detail: row.item_ref.clone(),
        });
    }
    let started_at = string_field(&row.raw, "started_at")?;
    let age_secs = age_seconds(started_at)?;
    (age_secs > LONG_RUNNING_TRAIN_SECS).then(|| ResourceWarning {
        level: WarningLevel::Warn,
        source: "training".to_string(),
        message: "train run has been live for more than 6h".to_string(),
        detail: format!("{} age {}h", row.item_ref, age_secs / 3600),
    })
}

fn age_seconds(timestamp: &str) -> Option<i64> {
    let then = OffsetDateTime::parse(timestamp, &Rfc3339).ok()?;
    let now = OffsetDateTime::now_utc();
    Some((now - then).whole_seconds())
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

fn string_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn bool_field(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

fn u64_field(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

fn nested_u64(value: &Value, path: &[&str]) -> Option<u64> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_u64()
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
#[path = "resource_tests.rs"]
mod resource_tests;
