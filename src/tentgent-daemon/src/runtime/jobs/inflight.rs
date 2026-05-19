use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use tokio::task::AbortHandle;

use super::JobId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InFlightJobKind {
    AsyncTask,
    BlockingTask,
}

impl InFlightJobKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AsyncTask => "async_task",
            Self::BlockingTask => "blocking_task",
        }
    }
}

#[derive(Debug, Clone)]
pub struct InFlightJob {
    pub job_id: JobId,
    pub kind: InFlightJobKind,
}

#[derive(Debug, Clone, Default)]
pub struct InFlightJobRegistry {
    inner: Arc<Mutex<BTreeMap<JobId, InFlightJobEntry>>>,
}

#[derive(Debug)]
struct InFlightJobEntry {
    kind: InFlightJobKind,
    abort: AbortHandle,
}

impl InFlightJobRegistry {
    pub fn register(&self, job_id: JobId, kind: InFlightJobKind, abort: AbortHandle) {
        let mut jobs = self.inner.lock().expect("in-flight job registry lock");
        jobs.insert(job_id, InFlightJobEntry { kind, abort });
    }

    pub fn remove(&self, job_id: &JobId) -> Option<InFlightJob> {
        let mut jobs = self.inner.lock().expect("in-flight job registry lock");
        let entry = jobs.remove(job_id)?;
        Some(InFlightJob {
            job_id: job_id.clone(),
            kind: entry.kind,
        })
    }

    pub fn list(&self) -> Vec<InFlightJob> {
        let jobs = self.inner.lock().expect("in-flight job registry lock");
        jobs.iter()
            .map(|(job_id, entry)| InFlightJob {
                job_id: job_id.clone(),
                kind: entry.kind,
            })
            .collect()
    }

    pub fn abort(&self, job_id: &JobId) -> bool {
        let entry = {
            let mut jobs = self.inner.lock().expect("in-flight job registry lock");
            jobs.remove(job_id)
        };
        let Some(entry) = entry else {
            return false;
        };
        entry.abort.abort();
        true
    }

    pub fn abort_all(&self) -> Vec<InFlightJob> {
        let entries = {
            let mut jobs = self.inner.lock().expect("in-flight job registry lock");
            std::mem::take(&mut *jobs)
        };
        let mut aborted = Vec::new();
        for (job_id, entry) in entries {
            entry.abort.abort();
            aborted.push(InFlightJob {
                job_id,
                kind: entry.kind,
            });
        }
        aborted
    }

    pub fn is_active(&self, job_id: &JobId) -> bool {
        let jobs = self.inner.lock().expect("in-flight job registry lock");
        jobs.contains_key(job_id)
    }
}
