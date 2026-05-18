use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
};

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::types::{JobId, JobItem};

const TERMINAL_RETENTION_COUNT: usize = 100;
const TERMINAL_RETENTION_HOURS: i64 = 24;

#[derive(Debug, Clone)]
pub struct JobStore {
    jobs_dir: PathBuf,
}

impl JobStore {
    pub fn from_runtime_dir(runtime_dir: impl AsRef<Path>) -> Self {
        Self {
            jobs_dir: runtime_dir.as_ref().join("jobs"),
        }
    }

    pub fn from_jobs_dir(jobs_dir: impl Into<PathBuf>) -> Self {
        Self {
            jobs_dir: jobs_dir.into(),
        }
    }

    pub fn jobs_dir(&self) -> &Path {
        &self.jobs_dir
    }

    pub fn load(&self) -> BTreeMap<JobId, JobItem> {
        let mut jobs = BTreeMap::new();
        let Ok(entries) = fs::read_dir(&self.jobs_dir) else {
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

    pub fn persist(&self, job: &JobItem) -> io::Result<()> {
        fs::create_dir_all(&self.jobs_dir)?;
        let path = self.job_path(&job.job_id);
        let temp_path = self.temp_job_path(&job.job_id);
        let bytes = serde_json::to_vec_pretty(job).map_err(io::Error::other)?;
        fs::write(&temp_path, bytes)?;
        fs::rename(temp_path, path)
    }

    pub fn persist_all(&self, jobs: &BTreeMap<JobId, JobItem>) -> io::Result<()> {
        if jobs.is_empty() && !self.jobs_dir.exists() {
            return Ok(());
        }

        fs::create_dir_all(&self.jobs_dir)?;
        let keep = jobs.keys().map(JobId::as_str).collect::<BTreeSet<_>>();
        for entry in fs::read_dir(&self.jobs_dir)? {
            let path = entry?.path();
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

        for job in jobs.values() {
            self.persist(job)?;
        }

        Ok(())
    }

    fn job_path(&self, job_id: &JobId) -> PathBuf {
        self.jobs_dir.join(format!("{}.json", job_id.as_str()))
    }

    fn temp_job_path(&self, job_id: &JobId) -> PathBuf {
        self.jobs_dir.join(format!("{}.json.tmp", job_id.as_str()))
    }
}

pub fn prune_terminal_jobs(jobs: &mut BTreeMap<JobId, JobItem>) {
    let threshold = OffsetDateTime::now_utc() - time::Duration::hours(TERMINAL_RETENTION_HOURS);
    let mut terminal = jobs
        .values()
        .filter(|job| job.status.is_terminal())
        .cloned()
        .collect::<Vec<_>>();
    terminal.sort_by(|left, right| right.timing.updated_at.cmp(&left.timing.updated_at));

    for (index, job) in terminal.iter().enumerate() {
        let older_than_retention = parse_time(&job.timing.updated_at)
            .map(|updated_at| updated_at < threshold)
            .unwrap_or(false);
        if index >= TERMINAL_RETENTION_COUNT || older_than_retention {
            jobs.remove(&job.job_id);
        }
    }
}

fn parse_time(value: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::jobs::{JobKind, JobStatus};

    #[test]
    fn store_persists_and_loads_jobs() {
        let root = unique_temp_dir("persist-load");
        let store = JobStore::from_jobs_dir(root.join("jobs"));
        let job = JobItem::queued(
            "job-test",
            JobKind::model_pull(),
            "Pull model",
            "2026-05-01T00:00:00Z",
        );

        store.persist(&job).expect("persist job");
        let loaded = store.load();

        assert_eq!(loaded.len(), 1);
        assert_eq!(
            loaded
                .get(&JobId::new("job-test"))
                .expect("loaded job")
                .label,
            "Pull model"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prune_terminal_jobs_keeps_active_jobs() {
        let mut jobs = BTreeMap::new();
        let mut failed = JobItem::queued(
            "job-failed",
            JobKind::model_pull(),
            "Failed",
            "2026-05-01T00:00:00Z",
        );
        failed.fail("failed", "2026-05-01T00:00:01Z");
        let running = JobItem::queued(
            "job-running",
            JobKind::model_pull(),
            "Running",
            "2026-05-01T00:00:00Z",
        );
        jobs.insert(failed.job_id.clone(), failed);
        jobs.insert(running.job_id.clone(), running);

        prune_terminal_jobs(&mut jobs);

        assert_eq!(
            jobs.get(&JobId::new("job-running"))
                .expect("running")
                .status,
            JobStatus::Queued
        );
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "tentgent-daemon-job-store-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ))
    }
}
