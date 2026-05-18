use serde::{Deserialize, Serialize};
use std::fmt;

pub const MAX_JOB_OUTPUT_LINES: usize = 200;
const MAX_JOB_OUTPUT_LINE_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JobId(String);

impl JobId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<String> for JobId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for JobId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JobKind(String);

impl JobKind {
    pub const MODEL_PULL: &'static str = "model_pull";
    pub const MODEL_IMPORT: &'static str = "model_import";
    pub const ADAPTER_PULL: &'static str = "adapter_pull";
    pub const ADAPTER_IMPORT: &'static str = "adapter_import";
    pub const DATASET_IMPORT: &'static str = "dataset_import";
    pub const DATASET_SYNTHESIS: &'static str = "dataset_synthesis";
    pub const DATASET_EVALUATION: &'static str = "dataset_evaluation";
    pub const LORA_TRAIN_RUN: &'static str = "lora_train_run";
    pub const SESSION_COMPACTION: &'static str = "session_compaction";

    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn model_pull() -> Self {
        Self::new(Self::MODEL_PULL)
    }

    pub fn model_import() -> Self {
        Self::new(Self::MODEL_IMPORT)
    }

    pub fn adapter_pull() -> Self {
        Self::new(Self::ADAPTER_PULL)
    }

    pub fn adapter_import() -> Self {
        Self::new(Self::ADAPTER_IMPORT)
    }

    pub fn dataset_import() -> Self {
        Self::new(Self::DATASET_IMPORT)
    }

    pub fn dataset_synthesis() -> Self {
        Self::new(Self::DATASET_SYNTHESIS)
    }

    pub fn dataset_evaluation() -> Self {
        Self::new(Self::DATASET_EVALUATION)
    }

    pub fn lora_train_run() -> Self {
        Self::new(Self::LORA_TRAIN_RUN)
    }

    pub fn session_compaction() -> Self {
        Self::new(Self::SESSION_COMPACTION)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for JobKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Interrupted,
    Canceled,
}

impl JobStatus {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Interrupted | Self::Canceled
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobTarget {
    pub section: String,
    pub reference: Option<String>,
    pub path: Option<String>,
}

impl JobTarget {
    pub fn new(section: impl Into<String>) -> Self {
        Self {
            section: section.into(),
            reference: None,
            path: None,
        }
    }

    pub fn with_reference(mut self, reference: impl Into<String>) -> Self {
        self.reference = Some(reference.into());
        self
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobArtifact {
    pub kind: String,
    pub reference: Option<String>,
    pub path: Option<String>,
}

impl JobArtifact {
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            reference: None,
            path: None,
        }
    }

    pub fn with_reference(mut self, reference: impl Into<String>) -> Self {
        self.reference = Some(reference.into());
        self
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct JobProgress {
    pub bytes_done: Option<u64>,
    pub bytes_total: Option<u64>,
    pub files_done: Option<u64>,
    pub files_total: Option<u64>,
    pub percent: Option<f64>,
    pub speed_bytes_per_sec: Option<f64>,
    pub eta_seconds: Option<f64>,
}

impl JobProgress {
    pub fn apply_patch(&mut self, patch: JobProgressPatch) {
        if let Some(value) = patch.bytes_done {
            self.bytes_done = Some(value);
        }
        if let Some(value) = patch.bytes_total {
            self.bytes_total = Some(value);
        }
        if let Some(value) = patch.files_done {
            self.files_done = Some(value);
        }
        if let Some(value) = patch.files_total {
            self.files_total = Some(value);
        }
        if let Some(value) = patch.speed_bytes_per_sec {
            self.speed_bytes_per_sec = Some(value);
        }
        if let Some(value) = patch.eta_seconds {
            self.eta_seconds = Some(value);
        }
        if let Some(value) = patch.percent {
            self.percent = Some(value);
        } else {
            self.percent = calculate_percent(
                self.bytes_done.or(self.files_done),
                self.bytes_total.or(self.files_total),
            );
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct JobProgressPatch {
    pub bytes_done: Option<u64>,
    pub bytes_total: Option<u64>,
    pub files_done: Option<u64>,
    pub files_total: Option<u64>,
    pub percent: Option<f64>,
    pub speed_bytes_per_sec: Option<f64>,
    pub eta_seconds: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStream {
    Stdout,
    Stderr,
    Event,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobOutputLine {
    pub stream: JobStream,
    pub line: String,
    pub timestamp: Option<String>,
}

impl JobOutputLine {
    pub fn new(stream: JobStream, line: impl Into<String>) -> Self {
        Self {
            stream,
            line: truncate_output_line(&line.into()),
            timestamp: None,
        }
    }

    pub fn with_timestamp(mut self, timestamp: impl Into<String>) -> Self {
        self.timestamp = Some(timestamp.into());
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobOutput {
    pub tail: Vec<JobOutputLine>,
    pub raw_log_path: Option<String>,
}

impl JobOutput {
    pub fn append(&mut self, line: JobOutputLine) {
        if self
            .tail
            .last()
            .map(|last| last.stream == line.stream && last.line == line.line)
            .unwrap_or(false)
        {
            return;
        }
        self.tail.push(line);
        if self.tail.len() > MAX_JOB_OUTPUT_LINES {
            let overflow = self.tail.len() - MAX_JOB_OUTPUT_LINES;
            self.tail.drain(0..overflow);
        }
    }

    pub fn set_raw_log_path(&mut self, path: impl Into<String>) {
        self.raw_log_path = Some(path.into());
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobTiming {
    pub queued_at: String,
    pub started_at: Option<String>,
    pub updated_at: String,
    pub finished_at: Option<String>,
}

impl JobTiming {
    pub fn queued(now: impl Into<String>) -> Self {
        let now = now.into();
        Self {
            queued_at: now.clone(),
            started_at: None,
            updated_at: now,
            finished_at: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct JobProgressUpdate {
    pub stage: Option<String>,
    pub progress: JobProgressPatch,
    pub output: Vec<JobOutputLine>,
    pub warning_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobItem {
    pub job_id: JobId,
    pub kind: JobKind,
    pub label: String,
    pub status: JobStatus,
    pub stage: String,
    pub cancellable: bool,
    pub target: Option<JobTarget>,
    pub artifact: Option<JobArtifact>,
    pub refresh_targets: Vec<String>,
    pub progress: JobProgress,
    pub output: JobOutput,
    pub timing: JobTiming,
    pub warning_summary: Option<String>,
    pub result_summary: Option<String>,
    pub error_summary: Option<String>,
}

impl JobItem {
    pub fn queued(
        job_id: impl Into<JobId>,
        kind: JobKind,
        label: impl Into<String>,
        now: impl Into<String>,
    ) -> Self {
        Self {
            job_id: job_id.into(),
            kind,
            label: label.into(),
            status: JobStatus::Queued,
            stage: "queued".to_string(),
            cancellable: false,
            target: None,
            artifact: None,
            refresh_targets: Vec::new(),
            progress: JobProgress::default(),
            output: JobOutput::default(),
            timing: JobTiming::queued(now),
            warning_summary: None,
            result_summary: None,
            error_summary: None,
        }
    }

    pub fn with_target(mut self, target: JobTarget) -> Self {
        self.target = Some(target);
        self
    }

    pub fn with_refresh_targets(mut self, targets: impl IntoIterator<Item = String>) -> Self {
        self.refresh_targets = targets.into_iter().collect();
        self
    }

    pub fn with_cancellable(mut self, cancellable: bool) -> Self {
        self.cancellable = cancellable;
        self
    }

    pub fn start(&mut self, stage: impl Into<String>, now: impl Into<String>) {
        if self.status.is_terminal() {
            return;
        }
        let now = now.into();
        self.status = JobStatus::Running;
        self.stage = stage.into();
        self.timing.started_at = Some(now.clone());
        self.timing.updated_at = now;
    }

    pub fn update_progress(&mut self, update: JobProgressUpdate, now: impl Into<String>) {
        if self.status.is_terminal() {
            return;
        }
        if self.status == JobStatus::Queued {
            self.status = JobStatus::Running;
        }
        if let Some(stage) = update.stage {
            self.stage = stage;
        }
        self.progress.apply_patch(update.progress);
        for line in update.output {
            self.output.append(line);
        }
        if let Some(warning) = update.warning_summary {
            self.warning_summary = Some(warning);
        }
        self.timing.updated_at = now.into();
    }

    pub fn succeed(
        &mut self,
        artifact: Option<JobArtifact>,
        result_summary: impl Into<String>,
        now: impl Into<String>,
    ) {
        if self.status.is_terminal() {
            return;
        }
        let now = now.into();
        self.status = JobStatus::Succeeded;
        self.stage = "succeeded".to_string();
        self.artifact = artifact;
        self.result_summary = Some(result_summary.into());
        self.timing.updated_at = now.clone();
        self.timing.finished_at = Some(now);
    }

    pub fn fail(&mut self, error_summary: impl Into<String>, now: impl Into<String>) {
        if self.status.is_terminal() {
            return;
        }
        let now = now.into();
        self.status = JobStatus::Failed;
        self.stage = "failed".to_string();
        self.error_summary = Some(error_summary.into());
        self.timing.updated_at = now.clone();
        self.timing.finished_at = Some(now);
    }

    pub fn interrupt(&mut self, error_summary: impl Into<String>, now: impl Into<String>) {
        if self.status.is_terminal() {
            return;
        }
        let now = now.into();
        self.status = JobStatus::Interrupted;
        self.stage = "interrupted".to_string();
        self.error_summary = Some(error_summary.into());
        self.timing.updated_at = now.clone();
        self.timing.finished_at = Some(now);
    }
}

fn calculate_percent(done: Option<u64>, total: Option<u64>) -> Option<f64> {
    match (done, total) {
        (Some(done), Some(total)) if total > 0 => Some((done as f64 / total as f64) * 100.0),
        _ => None,
    }
}

fn truncate_output_line(value: &str) -> String {
    if value.len() <= MAX_JOB_OUTPUT_LINE_BYTES {
        return value.to_string();
    }

    let mut end = 0;
    for (index, ch) in value.char_indices() {
        let next = index + ch.len_utf8();
        if next > MAX_JOB_OUTPUT_LINE_BYTES {
            break;
        }
        end = next;
    }
    format!("{}...", &value[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_status_terminal_states_are_explicit() {
        assert!(!JobStatus::Queued.is_terminal());
        assert!(!JobStatus::Running.is_terminal());
        assert!(JobStatus::Succeeded.is_terminal());
        assert!(JobStatus::Failed.is_terminal());
        assert!(JobStatus::Interrupted.is_terminal());
        assert!(JobStatus::Canceled.is_terminal());
    }

    #[test]
    fn progress_patch_calculates_percent_from_available_counts() {
        let mut progress = JobProgress::default();

        progress.apply_patch(JobProgressPatch {
            bytes_done: Some(25),
            bytes_total: Some(100),
            ..JobProgressPatch::default()
        });

        assert_eq!(progress.percent, Some(25.0));
    }

    #[test]
    fn output_tail_is_bounded() {
        let mut output = JobOutput::default();
        for index in 0..(MAX_JOB_OUTPUT_LINES + 5) {
            output.append(JobOutputLine::new(
                JobStream::Stdout,
                format!("line {index}"),
            ));
        }

        assert_eq!(output.tail.len(), MAX_JOB_OUTPUT_LINES);
        assert_eq!(output.tail[0].line, "line 5");
    }

    #[test]
    fn output_tail_skips_consecutive_duplicate_lines() {
        let mut output = JobOutput::default();

        output.append(JobOutputLine::new(JobStream::Event, "Downloading"));
        output.append(JobOutputLine::new(JobStream::Event, "Downloading"));
        output.append(JobOutputLine::new(JobStream::Event, "Download complete"));

        assert_eq!(output.tail.len(), 2);
        assert_eq!(output.tail[0].line, "Downloading");
        assert_eq!(output.tail[1].line, "Download complete");
    }

    #[test]
    fn job_lifecycle_keeps_product_artifact_separate_from_job_state() {
        let mut job = JobItem::queued("job-1", JobKind::model_pull(), "Pull model", "t0")
            .with_target(JobTarget::new("models").with_reference("repo/model"));

        job.start("pulling snapshot", "t1");
        job.update_progress(
            JobProgressUpdate {
                stage: Some("downloading".to_string()),
                progress: JobProgressPatch {
                    files_done: Some(1),
                    files_total: Some(2),
                    ..JobProgressPatch::default()
                },
                output: vec![JobOutputLine::new(
                    JobStream::Event,
                    "downloaded config.json",
                )],
                warning_summary: None,
            },
            "t2",
        );
        job.succeed(
            Some(JobArtifact::new("model").with_reference("abcdef123456")),
            "model imported",
            "t3",
        );

        assert_eq!(job.status, JobStatus::Succeeded);
        assert_eq!(job.progress.percent, Some(50.0));
        assert_eq!(
            job.artifact
                .as_ref()
                .and_then(|artifact| artifact.reference.as_deref()),
            Some("abcdef123456")
        );
        assert_eq!(
            job.target
                .as_ref()
                .and_then(|target| target.reference.as_deref()),
            Some("repo/model")
        );
    }
}
