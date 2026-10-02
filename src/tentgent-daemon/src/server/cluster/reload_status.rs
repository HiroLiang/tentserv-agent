use serde::Serialize;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub(super) struct ReloadStatus(Arc<Mutex<ReloadObservation>>);

#[derive(Clone, Serialize)]
pub(super) struct ReloadObservation {
    pub status: &'static str,
    pub candidate_hash: Option<String>,
    pub diagnostic: Option<String>,
}

impl Default for ReloadObservation {
    fn default() -> Self {
        Self {
            status: "idle",
            candidate_hash: None,
            diagnostic: None,
        }
    }
}

impl ReloadStatus {
    pub(super) fn set(&self, status: &'static str, hash: Option<&str>, diagnostic: Option<String>) {
        if let Ok(mut value) = self.0.lock() {
            *value = ReloadObservation {
                status,
                candidate_hash: hash.map(str::to_string),
                diagnostic,
            };
        }
    }
    pub(super) fn snapshot(&self) -> ReloadObservation {
        self.0.lock().map(|value| value.clone()).unwrap_or_default()
    }
}
