//! Managed store maintenance domain objects.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedStoreKind {
    Models,
    Adapters,
    Datasets,
}

impl ManagedStoreKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Models => "models",
            Self::Adapters => "adapters",
            Self::Datasets => "datasets",
        }
    }
}

impl std::fmt::Display for ManagedStoreKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreStagingGarbageItem {
    pub store: ManagedStoreKind,
    pub path: PathBuf,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreGcOutcome {
    pub apply: bool,
    pub items: Vec<StoreStagingGarbageItem>,
    pub total_bytes: u64,
    pub removed_count: usize,
}
