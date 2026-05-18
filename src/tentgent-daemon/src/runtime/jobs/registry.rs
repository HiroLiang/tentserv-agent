use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::{
    store::{prune_terminal_jobs, JobStore},
    types::{JobArtifact, JobId, JobItem, JobKind, JobProgressUpdate, JobStatus, JobTarget},
};

#[derive(Debug, Clone)]
pub struct JobRegistry {
    inner: Arc<JobRegistryInner>,
}

#[derive(Debug)]
struct JobRegistryInner {
    jobs: Mutex<BTreeMap<JobId, JobItem>>,
    counter: AtomicU64,
    store: Option<JobStore>,
}

impl JobRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(JobRegistryInner {
                jobs: Mutex::new(BTreeMap::new()),
                counter: AtomicU64::new(0),
                store: None,
            }),
        }
    }

    pub fn from_runtime_dir(runtime_dir: impl AsRef<std::path::Path>) -> Self {
        Self::from_store(JobStore::from_runtime_dir(runtime_dir))
    }

    pub fn from_store(store: JobStore) -> Self {
        let now = now_string();
        let mut jobs = store.load();
        for job in jobs.values_mut() {
            if !job.status.is_terminal() {
                job.interrupt("daemon restarted before this job completed", now.clone());
            }
        }
        prune_terminal_jobs(&mut jobs);
        let registry = Self {
            inner: Arc::new(JobRegistryInner {
                jobs: Mutex::new(jobs),
                counter: AtomicU64::new(0),
                store: Some(store),
            }),
        };
        registry.persist_all_best_effort();
        registry
    }

    pub fn create(
        &self,
        kind: JobKind,
        label: impl Into<String>,
        target: Option<JobTarget>,
        refresh_targets: impl IntoIterator<Item = String>,
    ) -> JobItem {
        let job_id = self.next_job_id();
        let mut job = JobItem::queued(job_id, kind, label, now_string());
        job.target = target;
        job.refresh_targets = refresh_targets.into_iter().collect();
        self.insert(job.clone());
        job
    }

    pub fn insert(&self, job: JobItem) {
        {
            let mut jobs = self.inner.jobs.lock().expect("job registry lock");
            jobs.insert(job.job_id.clone(), job.clone());
        }
        self.persist_best_effort(&job);
    }

    pub fn list(&self) -> Vec<JobItem> {
        let jobs = self.inner.jobs.lock().expect("job registry lock");
        let mut jobs = jobs.values().cloned().collect::<Vec<_>>();
        jobs.sort_by(|left, right| {
            left.status
                .is_terminal()
                .cmp(&right.status.is_terminal())
                .then_with(|| right.timing.updated_at.cmp(&left.timing.updated_at))
                .then_with(|| right.job_id.cmp(&left.job_id))
        });
        jobs
    }

    pub fn get(&self, job_id: &JobId) -> Option<JobItem> {
        let jobs = self.inner.jobs.lock().expect("job registry lock");
        jobs.get(job_id).cloned()
    }

    pub fn start(&self, job_id: &JobId, stage: impl Into<String>) -> Option<JobItem> {
        self.mutate(job_id, |job| job.start(stage, now_string()))
    }

    pub fn update_progress(&self, job_id: &JobId, update: JobProgressUpdate) -> Option<JobItem> {
        self.mutate(job_id, |job| job.update_progress(update, now_string()))
    }

    pub fn succeed(
        &self,
        job_id: &JobId,
        artifact: Option<JobArtifact>,
        result_summary: impl Into<String>,
    ) -> Option<JobItem> {
        self.mutate(job_id, |job| {
            job.succeed(artifact, result_summary, now_string())
        })
    }

    pub fn fail(&self, job_id: &JobId, error_summary: impl Into<String>) -> Option<JobItem> {
        self.mutate(job_id, |job| job.fail(error_summary, now_string()))
    }

    pub fn interrupt_active(&self, error_summary: impl Into<String>) -> Vec<JobItem> {
        let now = now_string();
        let error_summary = error_summary.into();
        let mut jobs = self.inner.jobs.lock().expect("job registry lock");
        let mut changed = Vec::new();
        for job in jobs.values_mut() {
            if job.status == JobStatus::Queued || job.status == JobStatus::Running {
                job.interrupt(error_summary.clone(), now.clone());
                changed.push(job.clone());
            }
        }
        drop(jobs);
        for job in &changed {
            self.persist_best_effort(job);
        }
        changed
    }

    fn mutate(&self, job_id: &JobId, mutate: impl FnOnce(&mut JobItem)) -> Option<JobItem> {
        let changed = {
            let mut jobs = self.inner.jobs.lock().expect("job registry lock");
            let job = jobs.get_mut(job_id)?;
            mutate(job);
            job.clone()
        };
        self.persist_best_effort(&changed);
        Some(changed)
    }

    fn next_job_id(&self) -> JobId {
        let counter = self.inner.counter.fetch_add(1, Ordering::Relaxed);
        JobId::new(format!(
            "job-{}-{counter}",
            OffsetDateTime::now_utc().unix_timestamp_nanos()
        ))
    }

    fn persist_best_effort(&self, job: &JobItem) {
        if let Some(store) = &self.inner.store {
            let _ = store.persist(job);
        }
    }

    fn persist_all_best_effort(&self) {
        let Some(store) = &self.inner.store else {
            return;
        };
        let jobs = self.inner.jobs.lock().expect("job registry lock");
        let _ = store.persist_all(&jobs);
    }
}

impl Default for JobRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn now_string() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::jobs::{JobProgressPatch, JobStream};

    #[test]
    fn registry_creates_lists_and_updates_jobs() {
        let registry = JobRegistry::new();
        let job = registry.create(
            JobKind::model_pull(),
            "Pull model",
            Some(JobTarget::new("models").with_reference("repo/model")),
            ["models".to_string()],
        );

        registry.start(&job.job_id, "pulling");
        let updated = registry
            .update_progress(
                &job.job_id,
                JobProgressUpdate {
                    stage: Some("downloading".to_string()),
                    progress: JobProgressPatch {
                        bytes_done: Some(1),
                        bytes_total: Some(2),
                        ..JobProgressPatch::default()
                    },
                    output: vec![super::super::types::JobOutputLine::new(
                        JobStream::Event,
                        "downloaded",
                    )],
                    warning_summary: None,
                },
            )
            .expect("updated job");
        registry.succeed(
            &job.job_id,
            Some(JobArtifact::new("model").with_reference("abcdef123456")),
            "done",
        );

        assert_eq!(updated.progress.percent, Some(50.0));
        let jobs = registry.list();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].status, JobStatus::Succeeded);
        assert_eq!(
            jobs[0]
                .artifact
                .as_ref()
                .and_then(|artifact| artifact.reference.as_deref()),
            Some("abcdef123456")
        );
    }

    #[test]
    fn registry_interrupts_only_active_jobs() {
        let registry = JobRegistry::new();
        let running = registry.create(JobKind::model_pull(), "Pull model", None, Vec::new());
        let failed = registry.create(JobKind::adapter_pull(), "Pull adapter", None, Vec::new());

        registry.start(&running.job_id, "running");
        registry.fail(&failed.job_id, "failed first");

        let interrupted = registry.interrupt_active("daemon restarted");

        assert_eq!(interrupted.len(), 1);
        assert_eq!(
            registry.get(&running.job_id).expect("running job").status,
            JobStatus::Interrupted
        );
        assert_eq!(
            registry.get(&failed.job_id).expect("failed job").status,
            JobStatus::Failed
        );
    }

    #[test]
    fn registry_loads_persisted_jobs_and_interrupts_active_ones() {
        let root = unique_temp_dir("load-interrupt");
        let store = JobStore::from_jobs_dir(root.join("jobs"));
        let mut running = JobItem::queued(
            "job-running",
            JobKind::model_pull(),
            "Pull model",
            "2026-05-01T00:00:00Z",
        );
        running.start("running", "2026-05-01T00:00:01Z");
        store.persist(&running).expect("persist running job");

        let registry = JobRegistry::from_store(store);
        let loaded = registry
            .get(&JobId::new("job-running"))
            .expect("loaded job");

        assert_eq!(loaded.status, JobStatus::Interrupted);
        assert_eq!(
            loaded.error_summary.as_deref(),
            Some("daemon restarted before this job completed")
        );

        let _ = std::fs::remove_dir_all(root);
    }

    fn unique_temp_dir(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "tentgent-daemon-job-registry-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ))
    }
}
