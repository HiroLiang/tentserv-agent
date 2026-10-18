use std::{
    fmt,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};

use super::ports::ResourceCoordinationLease;

pub const DEFAULT_COORDINATION_TIMEOUT: Duration = Duration::from_secs(2);
pub const DEFAULT_COORDINATION_ATTEMPTS: u32 = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceCoordinationCode {
    ResourceBusy,
    ResourceStateUnstable,
}

impl ResourceCoordinationCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ResourceBusy => "resource-busy",
            Self::ResourceStateUnstable => "resource-state-unstable",
        }
    }
}

impl fmt::Display for ResourceCoordinationCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceKind {
    Maintenance,
    Model,
    ModelCapability,
    Adapter,
    Dataset,
    TrainPlan,
    TrainRun,
    TrainingSlot,
    Cluster,
    Server,
    RouteClaim,
    PhysicalRuntime,
}

impl ResourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Maintenance => "maintenance",
            Self::Model => "model",
            Self::ModelCapability => "model-capability",
            Self::Adapter => "adapter",
            Self::Dataset => "dataset",
            Self::TrainPlan => "train-plan",
            Self::TrainRun => "train-run",
            Self::TrainingSlot => "training-slot",
            Self::Cluster => "cluster",
            Self::Server => "server",
            Self::RouteClaim => "route-claim",
            Self::PhysicalRuntime => "physical-runtime",
        }
    }
}

impl fmt::Display for ResourceKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceKey {
    pub kind: ResourceKind,
    pub identity: String,
}

impl ResourceKey {
    pub fn new(kind: ResourceKind, identity: impl Into<String>) -> Self {
        Self {
            kind,
            identity: identity.into(),
        }
    }

    pub fn maintenance() -> Self {
        Self::new(ResourceKind::Maintenance, "runtime-home")
    }

    pub fn label(&self) -> String {
        format!("{}:{}", self.kind, self.identity)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceLockMode {
    Shared,
    Exclusive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceLockRequest {
    pub operation_id: String,
    pub operation: String,
    pub locks: Vec<(ResourceKey, ResourceLockMode)>,
    pub timeout: Duration,
    pub max_attempts: u32,
}

impl ResourceLockRequest {
    pub fn new(operation: impl Into<String>, locks: Vec<(ResourceKey, ResourceLockMode)>) -> Self {
        Self {
            operation_id: new_operation_id(),
            operation: operation.into(),
            locks,
            timeout: DEFAULT_COORDINATION_TIMEOUT,
            max_attempts: DEFAULT_COORDINATION_ATTEMPTS,
        }
    }

    pub fn with_limits(mut self, timeout: Duration, max_attempts: u32) -> Self {
        self.timeout = timeout;
        self.max_attempts = max_attempts.max(1);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceHolder {
    pub operation_id: String,
    pub operation: String,
    pub pid: u32,
    pub mode: ResourceLockMode,
    pub observed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceBusy {
    pub code: ResourceCoordinationCode,
    pub operation_id: String,
    pub key: ResourceKey,
    pub attempts: u32,
    pub waited_millis: u64,
    pub holders: Vec<ResourceHolder>,
    pub retry_after_millis: u64,
    pub description: String,
}

pub struct ResourcePermit {
    operation_id: String,
    coordination_root: PathBuf,
    keys: Vec<ResourceKey>,
    locks: Vec<(ResourceKey, ResourceLockMode)>,
    lease: Option<Box<dyn ResourceCoordinationLease>>,
}

impl ResourcePermit {
    pub(crate) fn new(
        operation_id: String,
        coordination_root: PathBuf,
        locks: Vec<(ResourceKey, ResourceLockMode)>,
        lease: Box<dyn ResourceCoordinationLease>,
    ) -> Self {
        let keys = locks.iter().map(|(key, _)| key.clone()).collect();
        Self {
            operation_id,
            coordination_root,
            keys,
            locks,
            lease: Some(lease),
        }
    }

    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub fn keys(&self) -> &[ResourceKey] {
        &self.keys
    }

    /// The lock namespace in which this permit was acquired.
    ///
    /// Borrowers must match this against their coordinator's root as well as
    /// checking the required keys and modes. Matching keys alone cannot grant
    /// authority over a different runtime home's resources.
    pub fn coordination_root(&self) -> &Path {
        &self.coordination_root
    }

    /// Checks an actually acquired key and mode without acquiring or upgrading.
    /// An exclusive lock also covers a shared requirement, but not vice versa.
    pub fn covers(&self, key: &ResourceKey, mode: ResourceLockMode) -> bool {
        self.locks.iter().any(|(held_key, held_mode)| {
            held_key == key && (*held_mode == mode || *held_mode == ResourceLockMode::Exclusive)
        })
    }

    pub fn covers_all(&self, requirements: &[(ResourceKey, ResourceLockMode)]) -> bool {
        requirements
            .iter()
            .all(|(key, mode)| self.covers(key, *mode))
    }

    pub fn release(mut self) {
        self.lease.take();
    }
}

impl fmt::Debug for ResourcePermit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResourcePermit")
            .field("operation_id", &self.operation_id)
            .field("coordination_root", &self.coordination_root)
            .field("locks", &self.locks)
            .finish_non_exhaustive()
    }
}

pub fn new_operation_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let sequence = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}-{nanos}-{sequence}", std::process::id())
}
