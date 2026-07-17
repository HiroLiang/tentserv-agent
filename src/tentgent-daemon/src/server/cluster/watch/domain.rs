use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub(super) struct DefinitionWatchSchedule {
    pub metadata_interval: Duration,
    pub strong_hash_interval: Duration,
}

impl Default for DefinitionWatchSchedule {
    fn default() -> Self {
        Self {
            metadata_interval: Duration::from_secs(1),
            strong_hash_interval: Duration::from_secs(30),
        }
    }
}
