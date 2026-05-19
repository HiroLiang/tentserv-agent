use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use time::OffsetDateTime;

use crate::{
    features::job::{
        domain::{
            JobArtifact, JobId, JobItem, JobKind, JobProgressUpdate, JobStatus, JobTarget,
            JobWorkspaceSummary,
        },
        ports::{JobCatalogPort, JobCreateRecord},
    },
    foundation::error::KernelResult,
};

use super::{
    error::job_store_error,
    store::{prune_terminal_jobs, FileJobStore},
    time::now_string,
};

#[derive(Debug, Clone)]
pub struct JobRegistry {
    inner: Arc<JobRegistryInner>,
}

#[derive(Debug)]
struct JobRegistryInner {
    jobs: Mutex<BTreeMap<JobId, JobItem>>,
    counter: AtomicU64,
    store: Option<FileJobStore>,
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
        Self::from_store(FileJobStore::from_runtime_dir(runtime_dir))
    }

    pub fn from_store(store: FileJobStore) -> Self {
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

    pub fn update_workspace(
        &self,
        job_id: &JobId,
        workspace: JobWorkspaceSummary,
    ) -> Option<JobItem> {
        self.mutate(job_id, |job| job.update_workspace(workspace, now_string()))
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

    pub fn cancel(&self, job_id: &JobId, reason: impl Into<String>) -> Option<JobItem> {
        self.mutate(job_id, |job| job.cancel(reason, now_string()))
    }

    pub fn delete_terminal(&self, job_id: &JobId) -> Option<JobItem> {
        let removed = {
            let mut jobs = self.inner.jobs.lock().expect("job registry lock");
            let job = jobs.get(job_id)?;
            if !job.status.is_terminal() {
                return None;
            }
            jobs.remove(job_id)
        };
        if let Some(store) = &self.inner.store {
            let _ = store.remove(job_id);
        }
        removed
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

impl JobCatalogPort for JobRegistry {
    fn create_job(&self, record: JobCreateRecord) -> KernelResult<JobItem> {
        Ok(self.create(
            record.kind,
            record.label,
            record.target,
            record.refresh_targets,
        ))
    }

    fn insert_job(&self, job: JobItem) -> KernelResult<()> {
        self.insert(job);
        Ok(())
    }

    fn list_jobs(&self) -> KernelResult<Vec<JobItem>> {
        Ok(self.list())
    }

    fn inspect_job(&self, job_id: &JobId) -> KernelResult<Option<JobItem>> {
        Ok(self.get(job_id))
    }

    fn start_job(&self, job_id: &JobId, stage: String) -> KernelResult<Option<JobItem>> {
        Ok(self.start(job_id, stage))
    }

    fn update_job_progress(
        &self,
        job_id: &JobId,
        update: JobProgressUpdate,
    ) -> KernelResult<Option<JobItem>> {
        Ok(self.update_progress(job_id, update))
    }

    fn update_job_workspace(
        &self,
        job_id: &JobId,
        workspace: JobWorkspaceSummary,
    ) -> KernelResult<Option<JobItem>> {
        Ok(self.update_workspace(job_id, workspace))
    }

    fn succeed_job(
        &self,
        job_id: &JobId,
        artifact: Option<JobArtifact>,
        result_summary: String,
    ) -> KernelResult<Option<JobItem>> {
        Ok(self.succeed(job_id, artifact, result_summary))
    }

    fn fail_job(&self, job_id: &JobId, error_summary: String) -> KernelResult<Option<JobItem>> {
        Ok(self.fail(job_id, error_summary))
    }

    fn interrupt_active_jobs(&self, error_summary: String) -> KernelResult<Vec<JobItem>> {
        Ok(self.interrupt_active(error_summary))
    }

    fn cancel_job(&self, job_id: &JobId, reason: String) -> KernelResult<Option<JobItem>> {
        Ok(self.cancel(job_id, reason))
    }

    fn delete_terminal_job(&self, job_id: &JobId) -> KernelResult<Option<JobItem>> {
        let job = self.get(job_id);
        if let Some(job) = &job {
            if !job.status.is_terminal() {
                return Err(job_store_error(format!(
                    "delete active job record `{job_id}` failed: job is active"
                )));
            }
        }
        Ok(self.delete_terminal(job_id))
    }
}
