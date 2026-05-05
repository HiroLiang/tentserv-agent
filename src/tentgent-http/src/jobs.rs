use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

const TERMINAL_RETENTION_COUNT: usize = 100;
const TERMINAL_RETENTION_HOURS: i64 = 24;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct JobResponse {
    pub(crate) job: JobItem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct JobsResponse {
    pub(crate) jobs: Vec<JobItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct JobItem {
    pub(crate) job_id: String,
    pub(crate) kind: String,
    pub(crate) label: String,
    pub(crate) target_section: String,
    pub(crate) target_ref: Option<String>,
    pub(crate) status: JobStatus,
    pub(crate) stage: String,
    pub(crate) cancellable: bool,
    pub(crate) refresh_targets: Vec<String>,
    pub(crate) bytes_done: Option<u64>,
    pub(crate) bytes_total: Option<u64>,
    pub(crate) files_done: Option<u64>,
    pub(crate) files_total: Option<u64>,
    pub(crate) percent: Option<f64>,
    pub(crate) speed_bytes_per_sec: Option<f64>,
    pub(crate) eta_seconds: Option<f64>,
    pub(crate) started_at: String,
    pub(crate) updated_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) artifact_ref: Option<String>,
    pub(crate) artifact_path: Option<String>,
    pub(crate) warning_summary: Option<String>,
    pub(crate) result_summary: Option<String>,
    pub(crate) error_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Interrupted,
    Canceled,
}

impl JobStatus {
    pub(crate) fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Interrupted | Self::Canceled
        )
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct JobProgressUpdate {
    pub(crate) stage: Option<String>,
    pub(crate) bytes_done: Option<u64>,
    pub(crate) bytes_total: Option<u64>,
    pub(crate) files_done: Option<u64>,
    pub(crate) files_total: Option<u64>,
    pub(crate) warning_summary: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct JobRegistry {
    inner: Arc<JobRegistryInner>,
}

#[derive(Debug)]
struct JobRegistryInner {
    jobs_dir: PathBuf,
    jobs: Mutex<BTreeMap<String, JobItem>>,
    counter: AtomicU64,
}

impl JobRegistry {
    pub(crate) fn new(runtime_dir: impl AsRef<Path>) -> Self {
        let jobs_dir = runtime_dir.as_ref().join("jobs");
        let now = now_string();
        let mut jobs = load_jobs(&jobs_dir);
        for job in jobs.values_mut() {
            if !job.status.is_terminal() {
                job.status = JobStatus::Interrupted;
                job.stage = "interrupted by daemon restart".to_string();
                job.updated_at = now.clone();
                job.finished_at = Some(now.clone());
                job.error_summary = Some("daemon restarted before this job completed".to_string());
            }
        }
        prune_terminal_jobs(&mut jobs);
        let registry = Self {
            inner: Arc::new(JobRegistryInner {
                jobs_dir,
                jobs: Mutex::new(jobs),
                counter: AtomicU64::new(0),
            }),
        };
        registry.persist_all_best_effort();
        registry
    }

    pub(crate) fn create(
        &self,
        kind: impl Into<String>,
        label: impl Into<String>,
        target_section: impl Into<String>,
        refresh_targets: impl IntoIterator<Item = String>,
    ) -> JobItem {
        let now = now_string();
        let job_id = format!(
            "job-{}-{}",
            OffsetDateTime::now_utc().unix_timestamp_nanos(),
            self.inner.counter.fetch_add(1, Ordering::Relaxed)
        );
        let job = JobItem {
            job_id: job_id.clone(),
            kind: kind.into(),
            label: label.into(),
            target_section: target_section.into(),
            target_ref: None,
            status: JobStatus::Queued,
            stage: "queued".to_string(),
            cancellable: false,
            refresh_targets: refresh_targets.into_iter().collect(),
            bytes_done: None,
            bytes_total: None,
            files_done: None,
            files_total: None,
            percent: None,
            speed_bytes_per_sec: None,
            eta_seconds: None,
            started_at: now.clone(),
            updated_at: now,
            finished_at: None,
            artifact_ref: None,
            artifact_path: None,
            warning_summary: None,
            result_summary: None,
            error_summary: None,
        };
        self.replace_job(job.clone());
        job
    }

    pub(crate) fn start(&self, job_id: &str, stage: impl Into<String>) {
        self.mutate(job_id, |job| {
            if job.status.is_terminal() {
                return;
            }
            job.status = JobStatus::Running;
            job.stage = stage.into();
            job.updated_at = now_string();
        });
    }

    pub(crate) fn update_progress(&self, job_id: &str, update: JobProgressUpdate) {
        self.mutate(job_id, |job| {
            if job.status.is_terminal() {
                return;
            }
            if job.status == JobStatus::Queued {
                job.status = JobStatus::Running;
            }
            if let Some(stage) = update.stage {
                job.stage = truncate(&stage, 160);
            }
            job.bytes_done = update.bytes_done.or(job.bytes_done);
            job.bytes_total = update.bytes_total.or(job.bytes_total);
            job.files_done = update.files_done.or(job.files_done);
            job.files_total = update.files_total.or(job.files_total);
            job.percent = calculate_percent(
                job.bytes_done.or(job.files_done),
                job.bytes_total.or(job.files_total),
            );
            job.speed_bytes_per_sec = None;
            job.eta_seconds = None;
            if let Some(warning) = update.warning_summary {
                job.warning_summary = Some(truncate(&warning, 800));
            }
            job.updated_at = now_string();
        });
    }

    pub(crate) fn succeed(
        &self,
        job_id: &str,
        target_ref: Option<String>,
        artifact_ref: Option<String>,
        artifact_path: Option<String>,
        result_summary: impl Into<String>,
    ) {
        self.mutate(job_id, |job| {
            if job.status.is_terminal() {
                return;
            }
            let now = now_string();
            job.status = JobStatus::Succeeded;
            job.stage = "succeeded".to_string();
            job.target_ref = target_ref;
            job.artifact_ref = artifact_ref;
            job.artifact_path = artifact_path.map(|value| truncate(&value, 1200));
            job.result_summary = Some(truncate(&result_summary.into(), 1200));
            job.updated_at = now.clone();
            job.finished_at = Some(now);
        });
    }

    pub(crate) fn fail(&self, job_id: &str, message: impl Into<String>) {
        self.mutate(job_id, |job| {
            if job.status.is_terminal() {
                return;
            }
            let now = now_string();
            job.status = JobStatus::Failed;
            job.stage = "failed".to_string();
            job.error_summary = Some(truncate(&message.into(), 1200));
            job.updated_at = now.clone();
            job.finished_at = Some(now);
        });
    }

    pub(crate) fn list(&self) -> Vec<JobItem> {
        let jobs = self.inner.jobs.lock().expect("job registry lock");
        let mut jobs = jobs.values().cloned().collect::<Vec<_>>();
        jobs.sort_by(|left, right| {
            left.status
                .is_terminal()
                .cmp(&right.status.is_terminal())
                .then_with(|| right.updated_at.cmp(&left.updated_at))
        });
        jobs
    }

    pub(crate) fn get(&self, job_id: &str) -> Option<JobItem> {
        let jobs = self.inner.jobs.lock().expect("job registry lock");
        jobs.get(job_id).cloned()
    }

    fn replace_job(&self, job: JobItem) {
        {
            let mut jobs = self.inner.jobs.lock().expect("job registry lock");
            jobs.insert(job.job_id.clone(), job.clone());
            prune_terminal_jobs(&mut jobs);
        }
        let _ = persist_job(&self.inner.jobs_dir, &job);
    }

    fn mutate(&self, job_id: &str, mutate: impl FnOnce(&mut JobItem)) {
        let changed = {
            let mut jobs = self.inner.jobs.lock().expect("job registry lock");
            let Some(job) = jobs.get_mut(job_id) else {
                return;
            };
            mutate(job);
            job.clone()
        };
        let _ = persist_job(&self.inner.jobs_dir, &changed);
    }

    fn persist_all_best_effort(&self) {
        let jobs = self.list();
        if let Err(_error) = fs::create_dir_all(&self.inner.jobs_dir) {
            return;
        }
        let keep = jobs
            .iter()
            .map(|job| job.job_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        if let Ok(entries) = fs::read_dir(&self.inner.jobs_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|value| value.to_str()) != Some("json") {
                    continue;
                }
                let keep_file = path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .map(|stem| keep.contains(stem))
                    .unwrap_or(false);
                if !keep_file {
                    let _ = fs::remove_file(path);
                }
            }
        }
        for job in jobs {
            let _ = persist_job(&self.inner.jobs_dir, &job);
        }
    }
}

fn load_jobs(jobs_dir: &Path) -> BTreeMap<String, JobItem> {
    let mut jobs = BTreeMap::new();
    let Ok(entries) = fs::read_dir(jobs_dir) else {
        return jobs;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(job) = serde_json::from_slice::<JobItem>(&bytes) else {
            continue;
        };
        jobs.insert(job.job_id.clone(), job);
    }
    jobs
}

fn prune_terminal_jobs(jobs: &mut BTreeMap<String, JobItem>) {
    let threshold = OffsetDateTime::now_utc() - time::Duration::hours(TERMINAL_RETENTION_HOURS);
    let mut terminal = jobs
        .values()
        .filter(|job| job.status.is_terminal())
        .cloned()
        .collect::<Vec<_>>();
    terminal.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));

    for (index, job) in terminal.iter().enumerate() {
        let older_than_retention = parse_time(&job.updated_at)
            .map(|updated_at| updated_at < threshold)
            .unwrap_or(false);
        if index >= TERMINAL_RETENTION_COUNT || older_than_retention {
            jobs.remove(&job.job_id);
        }
    }
}

fn persist_job(jobs_dir: &Path, job: &JobItem) -> io::Result<()> {
    fs::create_dir_all(jobs_dir)?;
    let path = jobs_dir.join(format!("{}.json", job.job_id));
    let temp_path = jobs_dir.join(format!("{}.json.tmp", job.job_id));
    let bytes = serde_json::to_vec_pretty(job).map_err(io::Error::other)?;
    fs::write(&temp_path, bytes)?;
    fs::rename(temp_path, path)
}

fn now_string() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn parse_time(value: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).ok()
}

fn calculate_percent(done: Option<u64>, total: Option<u64>) -> Option<f64> {
    let done = done?;
    let total = total?;
    if total == 0 {
        return None;
    }
    Some(((done as f64 / total as f64) * 100.0).clamp(0.0, 100.0))
}

fn truncate(value: &str, max: usize) -> String {
    let mut chars = value.chars();
    let head = chars.by_ref().take(max).collect::<String>();
    if chars.next().is_some() {
        format!("{head}...")
    } else {
        head
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn terminal_jobs_do_not_mutate_back_to_running() {
        let registry = JobRegistry::new(unique_dir("terminal"));
        let job = registry.create("model_pull", "org/model", "models", ["models".to_string()]);
        registry.fail(&job.job_id, "failed once");
        registry.start(&job.job_id, "running again");
        let job = registry.get(&job.job_id).expect("job");

        assert_eq!(job.status, JobStatus::Failed);
        assert_eq!(job.stage, "failed");
    }

    #[test]
    fn active_jobs_are_interrupted_on_registry_reload() {
        let dir = unique_dir("reload");
        let registry = JobRegistry::new(&dir);
        let job = registry.create("model_pull", "org/model", "models", ["models".to_string()]);
        registry.start(&job.job_id, "downloading");

        let reloaded = JobRegistry::new(&dir);
        let job = reloaded.get(&job.job_id).expect("job");
        assert_eq!(job.status, JobStatus::Interrupted);
        assert!(job.error_summary.expect("summary").contains("restarted"));
    }

    #[test]
    fn progress_with_unknown_total_has_no_fake_percent() {
        let registry = JobRegistry::new(unique_dir("unknown-total"));
        let job = registry.create("model_pull", "org/model", "models", ["models".to_string()]);
        registry.update_progress(
            &job.job_id,
            JobProgressUpdate {
                stage: Some("download".to_string()),
                bytes_done: Some(128),
                bytes_total: None,
                ..JobProgressUpdate::default()
            },
        );
        let job = registry.get(&job.job_id).expect("job");

        assert_eq!(job.bytes_done, Some(128));
        assert!(job.percent.is_none());
    }

    fn unique_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("tentgent-job-test-{label}-{nanos}"))
    }
}
