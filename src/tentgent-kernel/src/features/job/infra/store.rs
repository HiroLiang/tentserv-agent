use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
};

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::features::job::domain::{JobId, JobItem};

const TERMINAL_RETENTION_COUNT: usize = 100;
const TERMINAL_RETENTION_HOURS: i64 = 24;

#[derive(Debug, Clone)]
pub struct FileJobStore {
    jobs_dir: PathBuf,
}

impl FileJobStore {
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

    pub fn remove(&self, job_id: &JobId) -> io::Result<bool> {
        let path = self.job_path(job_id);
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_file(path)?;
        Ok(true)
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
