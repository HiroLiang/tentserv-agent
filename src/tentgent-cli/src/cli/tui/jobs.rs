use std::time::Duration;

use reqwest::StatusCode;
use serde::Deserialize;

use super::{daemon_client::TuiTokenSource, navigator::NavigatorListKind};

const JOB_CONNECT_TIMEOUT: Duration = Duration::from_millis(700);

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub(super) struct TuiJobItem {
    pub(super) job_id: String,
    pub(super) kind: String,
    pub(super) label: String,
    pub(super) target_section: String,
    pub(super) target_ref: Option<String>,
    pub(super) status: String,
    pub(super) stage: String,
    pub(super) cancellable: bool,
    pub(super) refresh_targets: Vec<String>,
    pub(super) bytes_done: Option<u64>,
    pub(super) bytes_total: Option<u64>,
    pub(super) files_done: Option<u64>,
    pub(super) files_total: Option<u64>,
    pub(super) percent: Option<f64>,
    pub(super) speed_bytes_per_sec: Option<f64>,
    pub(super) eta_seconds: Option<f64>,
    pub(super) started_at: String,
    pub(super) updated_at: String,
    pub(super) finished_at: Option<String>,
    pub(super) artifact_ref: Option<String>,
    pub(super) artifact_path: Option<String>,
    pub(super) warning_summary: Option<String>,
    pub(super) result_summary: Option<String>,
    pub(super) error_summary: Option<String>,
}

impl TuiJobItem {
    pub(super) fn is_active(&self) -> bool {
        matches!(self.status.as_str(), "queued" | "running")
    }

    pub(super) fn refresh_kinds(&self) -> Vec<NavigatorListKind> {
        self.refresh_targets
            .iter()
            .filter_map(|target| match target.as_str() {
                "models" => Some(NavigatorListKind::Models),
                "adapters" => Some(NavigatorListKind::Adapters),
                "datasets" => Some(NavigatorListKind::Datasets),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub(super) struct JobState {
    pub(super) jobs: Vec<TuiJobItem>,
    pub(super) selected: usize,
    pub(super) load_state: JobLoadState,
}

impl Default for JobState {
    fn default() -> Self {
        Self {
            jobs: Vec::new(),
            selected: 0,
            load_state: JobLoadState::Idle,
        }
    }
}

impl JobState {
    pub(super) fn active_jobs(&self) -> Vec<&TuiJobItem> {
        self.jobs.iter().filter(|job| job.is_active()).collect()
    }

    pub(super) fn move_selection(&mut self, delta: isize) {
        self.selected = move_index(self.selected, self.jobs.len(), delta);
    }

    pub(super) fn apply_jobs(&mut self, jobs: Vec<TuiJobItem>) -> Vec<TuiJobItem> {
        let previous_active = self
            .jobs
            .iter()
            .filter(|job| job.is_active())
            .map(|job| job.job_id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let completed = jobs
            .iter()
            .filter(|job| !job.is_active() && previous_active.contains(&job.job_id))
            .cloned()
            .collect::<Vec<_>>();
        self.jobs = jobs;
        self.selected = self.selected.min(self.jobs.len().saturating_sub(1));
        self.load_state = JobLoadState::Ready;
        completed
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum JobLoadState {
    Idle,
    Loading { request_id: u64 },
    Ready,
    Error { message: String, stale: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum JobError {
    AuthRequired(String),
    Down(String),
    Timeout(String),
    Protocol(String),
    Http { status: u16, message: String },
}

impl std::fmt::Display for JobError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AuthRequired(message)
            | Self::Down(message)
            | Self::Timeout(message)
            | Self::Protocol(message) => write!(formatter, "{message}"),
            Self::Http { status, message } => write!(formatter, "HTTP {status}: {message}"),
        }
    }
}

pub(super) struct JobClient {
    base_url: String,
    token: Option<String>,
    client: reqwest::Client,
}

impl JobClient {
    pub(super) fn new(
        base_url: String,
        token: Option<String>,
        _token_source: TuiTokenSource,
    ) -> miette::Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(JOB_CONNECT_TIMEOUT)
            .build()
            .map_err(|error| miette::miette!("failed to build job client: {error}"))?;
        Ok(Self {
            base_url,
            token,
            client,
        })
    }

    pub(super) async fn list_jobs(&self) -> Result<Vec<TuiJobItem>, JobError> {
        let mut builder = self.client.get(self.endpoint("/v1/jobs"));
        if let Some(token) = self.token.as_deref() {
            builder = builder.bearer_auth(token);
        }
        let response = builder.send().await.map_err(|error| {
            if error.is_timeout() {
                JobError::Timeout(format!("/v1/jobs timed out: {error}"))
            } else {
                JobError::Down(format!("/v1/jobs failed: {error}"))
            }
        })?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|error| JobError::Protocol(format!("failed to read /v1/jobs: {error}")))?;
        if !status.is_success() {
            return Err(job_error_from_status(status, &text));
        }
        let response: JobsResponse = serde_json::from_str(&text)
            .map_err(|error| JobError::Protocol(format!("invalid /v1/jobs JSON: {error}")))?;
        Ok(response.jobs)
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.base_url.trim_end_matches('/'), path)
    }
}

#[derive(Debug, Deserialize)]
struct JobsResponse {
    jobs: Vec<TuiJobItem>,
}

#[derive(Debug, Deserialize)]
pub(super) struct JobResponse {
    pub(super) job: TuiJobItem,
}

pub(super) fn job_progress_label(job: &TuiJobItem) -> String {
    if let Some(percent) = job.percent {
        let done = format_units(job.bytes_done, job.bytes_total, "B")
            .or_else(|| format_units(job.files_done, job.files_total, "files"))
            .unwrap_or_default();
        format!("{percent:.0}% {done}")
    } else if let Some(done) = job.bytes_done {
        format!("{}B", done)
    } else if let Some(done) = job.files_done {
        format!("{done} files")
    } else {
        job.stage.clone()
    }
}

pub(super) fn sanitize_job_summary(job: &TuiJobItem) -> String {
    job.error_summary
        .as_deref()
        .or(job.result_summary.as_deref())
        .or(job.warning_summary.as_deref())
        .unwrap_or(&job.stage)
        .chars()
        .take(240)
        .collect()
}

pub(super) fn parse_job_response(text: &str) -> Result<TuiJobItem, JobError> {
    serde_json::from_str::<JobResponse>(text)
        .map(|response| response.job)
        .map_err(|error| JobError::Protocol(format!("invalid job response JSON: {error}")))
}

fn job_error_from_status(status: StatusCode, text: &str) -> JobError {
    let message = error_message(text).unwrap_or_else(|| format!("/v1/jobs returned {status}"));
    if status == StatusCode::UNAUTHORIZED {
        JobError::AuthRequired(message)
    } else {
        JobError::Http {
            status: status.as_u16(),
            message,
        }
    }
}

fn error_message(text: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(serde_json::Value::as_str)
                .or_else(|| value.get("error").and_then(serde_json::Value::as_str))
                .map(ToOwned::to_owned)
        })
        .or_else(|| (!text.trim().is_empty()).then(|| text.trim().to_string()))
}

fn format_units(done: Option<u64>, total: Option<u64>, unit: &str) -> Option<String> {
    match (done, total) {
        (Some(done), Some(total)) => Some(format!("{done}/{total} {unit}")),
        (Some(done), None) => Some(format!("{done} {unit}")),
        _ => None,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_progress_does_not_fake_percent() {
        let job = TuiJobItem {
            job_id: "job".to_string(),
            kind: "model_pull".to_string(),
            label: "org/model".to_string(),
            target_section: "models".to_string(),
            target_ref: None,
            status: "running".to_string(),
            stage: "downloading".to_string(),
            cancellable: false,
            refresh_targets: vec!["models".to_string()],
            bytes_done: Some(42),
            bytes_total: None,
            files_done: None,
            files_total: None,
            percent: None,
            speed_bytes_per_sec: None,
            eta_seconds: None,
            started_at: "2026-05-04T00:00:00Z".to_string(),
            updated_at: "2026-05-04T00:00:00Z".to_string(),
            finished_at: None,
            artifact_ref: None,
            artifact_path: None,
            warning_summary: None,
            result_summary: None,
            error_summary: None,
        };
        assert_eq!(job_progress_label(&job), "42B");
        assert!(!job_progress_label(&job).contains('%'));
    }
}
