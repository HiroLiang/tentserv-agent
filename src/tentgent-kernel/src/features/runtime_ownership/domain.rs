use std::{fmt, path::PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::features::{
    cluster::domain::{ClusterRef, ClusterRouteKey},
    resource_coordination::{new_operation_id, ResourceKey, ResourceKind},
    runtime::infra::ModelRuntimeCapability,
    server::domain::ServerRuntimeProfileSelection,
};

pub const RUNTIME_OWNERSHIP_SCHEMA_VERSION: u32 = 1;
pub const RUNTIME_OWNERSHIP_DIRNAME: &str = "ownership";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RuntimeExecutionIdentity {
    ModelBound {
        model_ref: String,
        capability: ModelRuntimeCapability,
        profile: RuntimeProfileIdentity,
    },
    Unbound {
        capability: ModelRuntimeCapability,
        profile: RuntimeProfileIdentity,
    },
}

impl RuntimeExecutionIdentity {
    pub fn model_bound(
        model_ref: impl Into<String>,
        capability: ModelRuntimeCapability,
        profile: Option<&ServerRuntimeProfileSelection>,
    ) -> Self {
        Self::ModelBound {
            model_ref: model_ref.into(),
            capability,
            profile: RuntimeProfileIdentity::from_selection(profile),
        }
    }

    pub fn unbound(
        capability: ModelRuntimeCapability,
        profile: Option<&ServerRuntimeProfileSelection>,
    ) -> Self {
        Self::Unbound {
            capability,
            profile: RuntimeProfileIdentity::from_selection(profile),
        }
    }

    pub const fn capability(&self) -> ModelRuntimeCapability {
        match self {
            Self::ModelBound { capability, .. } | Self::Unbound { capability, .. } => *capability,
        }
    }

    pub fn model_ref(&self) -> Option<&str> {
        match self {
            Self::ModelBound { model_ref, .. } => Some(model_ref),
            Self::Unbound { .. } => None,
        }
    }

    pub fn canonical_text(&self) -> String {
        match self {
            Self::ModelBound {
                model_ref,
                capability,
                profile,
            } => format!(
                "model:{model_ref}|capability:{capability}|profile:{}",
                profile.label()
            ),
            Self::Unbound {
                capability,
                profile,
            } => format!(
                "unbound|capability:{capability}|profile:{}",
                profile.label()
            ),
        }
    }

    pub fn physical_key(&self) -> ResourceKey {
        ResourceKey::new(
            ResourceKind::PhysicalRuntime,
            hash_text(&self.canonical_text()),
        )
    }

    pub fn transition_keys(&self) -> Vec<ResourceKey> {
        let mut keys = vec![ResourceKey::maintenance()];
        if let Some(model_ref) = self.model_ref() {
            keys.push(ResourceKey::new(ResourceKind::Model, model_ref));
            keys.push(ResourceKey::new(
                ResourceKind::ModelCapability,
                format!("{model_ref}|{}", self.capability()),
            ));
        }
        keys.push(self.physical_key());
        keys
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RuntimeProfileIdentity {
    pub profile_id: String,
    pub profile_version: u32,
}

impl RuntimeProfileIdentity {
    pub fn from_selection(selection: Option<&ServerRuntimeProfileSelection>) -> Self {
        selection
            .map(|selection| Self {
                profile_id: selection.profile_id.clone(),
                profile_version: selection.profile_version,
            })
            .unwrap_or_else(|| Self {
                profile_id: "default".to_string(),
                profile_version: 1,
            })
    }

    pub fn label(&self) -> String {
        format!("{}@{}", self.profile_id, self.profile_version)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessInstanceIdentity {
    pub pid: u32,
    pub token: String,
}

impl ProcessInstanceIdentity {
    pub fn current(token: impl Into<String>) -> Self {
        Self {
            pid: std::process::id(),
            token: token.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouteClaimState {
    Active,
    Retiring,
}

impl RouteClaimState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Retiring => "retiring",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouteClaimOperation {
    Acquire,
    Retire,
    Release,
    Reconcile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteGenerationClaim {
    pub schema_version: u32,
    pub owner_id: String,
    pub state: RouteClaimState,
    pub server_ref: String,
    pub cluster_ref: ClusterRef,
    pub route: ClusterRouteKey,
    pub definition_hash: String,
    pub target: RuntimeExecutionIdentity,
    pub process: ProcessInstanceIdentity,
    pub operation: RouteClaimOperation,
    pub acquired_at: String,
    pub updated_at: String,
}

impl RouteGenerationClaim {
    pub fn new(
        server_ref: impl Into<String>,
        cluster_ref: ClusterRef,
        route: ClusterRouteKey,
        definition_hash: impl Into<String>,
        target: RuntimeExecutionIdentity,
        process: ProcessInstanceIdentity,
    ) -> Self {
        let server_ref = server_ref.into();
        let definition_hash = definition_hash.into();
        let owner_id = route_owner_id(&server_ref, &cluster_ref, route, &definition_hash, &target);
        let now = now_text();
        Self {
            schema_version: RUNTIME_OWNERSHIP_SCHEMA_VERSION,
            owner_id,
            state: RouteClaimState::Active,
            server_ref,
            cluster_ref,
            route,
            definition_hash,
            target,
            process,
            operation: RouteClaimOperation::Acquire,
            acquired_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn claim_key(&self) -> ResourceKey {
        ResourceKey::new(ResourceKind::RouteClaim, &self.owner_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeGenerationState {
    Starting,
    Ready,
    Closing,
}

impl RuntimeGenerationState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Closing => "closing",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeGenerationOperation {
    Start,
    Ready,
    Close,
    Reconcile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeGenerationEndpoint {
    pub host: String,
    pub port: u16,
    pub pid: u32,
    pub process_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeGenerationLaunchTarget {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeGenerationHealth {
    Matching {
        status: String,
        endpoint: RuntimeGenerationEndpoint,
    },
    Mismatch {
        description: String,
    },
    Unavailable {
        description: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeLaunchPolicyRecord {
    pub idle_keep_alive_seconds: String,
    pub model_idle_timeout_seconds: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeGenerationRecord {
    pub schema_version: u32,
    pub runtime_key: String,
    pub generation_id: String,
    pub identity: RuntimeExecutionIdentity,
    pub launcher: ProcessInstanceIdentity,
    pub process_token: String,
    pub state: RuntimeGenerationState,
    #[serde(default)]
    pub launch_target: Option<RuntimeGenerationLaunchTarget>,
    #[serde(default)]
    pub endpoint: Option<RuntimeGenerationEndpoint>,
    #[serde(default)]
    pub diagnostic: Option<String>,
    pub policy: RuntimeLaunchPolicyRecord,
    pub operation: RuntimeGenerationOperation,
    pub started_at: String,
    pub updated_at: String,
}

impl RuntimeGenerationRecord {
    pub fn starting(identity: RuntimeExecutionIdentity, policy: RuntimeLaunchPolicyRecord) -> Self {
        let now = now_text();
        let process_token = new_process_token();
        Self {
            schema_version: RUNTIME_OWNERSHIP_SCHEMA_VERSION,
            runtime_key: identity.physical_key().identity,
            generation_id: new_operation_id(),
            identity,
            launcher: ProcessInstanceIdentity::current(process_token.clone()),
            process_token,
            state: RuntimeGenerationState::Starting,
            launch_target: None,
            endpoint: None,
            diagnostic: None,
            policy,
            operation: RuntimeGenerationOperation::Start,
            started_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn adopted(
        identity: RuntimeExecutionIdentity,
        policy: RuntimeLaunchPolicyRecord,
        endpoint: RuntimeGenerationEndpoint,
    ) -> Self {
        let now = now_text();
        Self {
            schema_version: RUNTIME_OWNERSHIP_SCHEMA_VERSION,
            runtime_key: identity.physical_key().identity,
            generation_id: new_operation_id(),
            identity,
            launcher: ProcessInstanceIdentity::current(endpoint.process_token.clone()),
            process_token: endpoint.process_token.clone(),
            state: RuntimeGenerationState::Ready,
            launch_target: Some(RuntimeGenerationLaunchTarget {
                host: endpoint.host.clone(),
                port: endpoint.port,
            }),
            endpoint: Some(endpoint),
            diagnostic: None,
            policy,
            operation: RuntimeGenerationOperation::Ready,
            started_at: now.clone(),
            updated_at: now,
        }
    }
}

pub fn sanitize_runtime_diagnostic(value: impl AsRef<str>) -> String {
    const MAX_CHARS: usize = 512;
    let normalized = value
        .as_ref()
        .replace(['\r', '\n'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.chars().count() <= MAX_CHARS {
        return normalized;
    }
    let mut truncated = normalized.chars().take(MAX_CHARS).collect::<String>();
    truncated.push_str("...");
    truncated
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnershipOperationRecord {
    pub schema_version: u32,
    pub operation_id: String,
    pub operation: String,
    pub pid: u32,
    #[serde(default)]
    pub resource_keys: Vec<ResourceKey>,
    pub started_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeOwnershipLayout {
    pub root: PathBuf,
    pub claims: PathBuf,
    pub generations: PathBuf,
    pub operations: PathBuf,
    pub quarantine: PathBuf,
}

impl RuntimeOwnershipLayout {
    pub fn from_runtime_dir(runtime_dir: &std::path::Path) -> Self {
        let root = runtime_dir.join(RUNTIME_OWNERSHIP_DIRNAME);
        Self {
            claims: root.join("claims"),
            generations: root.join("generations"),
            operations: root.join("operations"),
            quarantine: root.join("quarantine"),
            root,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeOwnershipIssue {
    pub kind: String,
    pub record: String,
    pub description: String,
    pub recoverable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeOwnershipSummary {
    pub route_claim_count: usize,
    pub active_generation_count: usize,
    pub active_operation_count: usize,
    pub stale_record_count: usize,
    pub malformed_record_count: usize,
    pub status: RuntimeOwnershipStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeOwnershipStatus {
    Healthy,
    Attention,
    Blocked,
}

impl fmt::Display for RuntimeOwnershipStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Healthy => "healthy",
            Self::Attention => "attention",
            Self::Blocked => "blocked",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeOwnershipInspection {
    pub claims: Vec<RouteGenerationClaim>,
    pub generations: Vec<RuntimeGenerationRecord>,
    pub operations: Vec<OwnershipOperationRecord>,
    pub issues: Vec<RuntimeOwnershipIssue>,
    pub summary: RuntimeOwnershipSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeOwnershipScope {
    Global,
    Cluster {
        cluster_ref: ClusterRef,
    },
    Server {
        server_ref: String,
        runtime_identities: Vec<RuntimeExecutionIdentity>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeOwnershipView {
    #[serde(flatten)]
    pub summary: RuntimeOwnershipSummary,
    pub scope: RuntimeOwnershipScopeView,
    pub claims: Vec<RuntimeOwnershipClaimView>,
    pub generations: Vec<RuntimeOwnershipGenerationView>,
    pub issues: Vec<RuntimeOwnershipIssueView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeOwnershipScopeView {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeOwnershipClaimView {
    pub state: RouteClaimState,
    pub server_ref: String,
    pub cluster_ref: String,
    pub route: String,
    pub definition_hash: String,
    pub target: RuntimeExecutionIdentity,
    pub operation: RouteClaimOperation,
    pub acquired_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeOwnershipGenerationView {
    pub state: RuntimeGenerationState,
    pub identity: RuntimeExecutionIdentity,
    pub policy: RuntimeLaunchPolicyRecord,
    pub operation: RuntimeGenerationOperation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
    pub started_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeOwnershipIssueView {
    pub kind: String,
    pub description: String,
    pub recoverable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeReconcileRequest {
    pub apply: bool,
    pub purge_quarantine: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeReconcileAction {
    pub kind: String,
    pub record: String,
    pub action: String,
    pub applied: bool,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeReconcileResult {
    pub before: RuntimeOwnershipSummary,
    pub after: RuntimeOwnershipSummary,
    pub actions: Vec<RuntimeReconcileAction>,
}

pub fn route_owner_id(
    server_ref: &str,
    cluster_ref: &ClusterRef,
    route: ClusterRouteKey,
    definition_hash: &str,
    target: &RuntimeExecutionIdentity,
) -> String {
    hash_text(&format!(
        "server:{server_ref}|cluster:{cluster_ref}|route:{route}|definition:{definition_hash}|target:{}",
        target.canonical_text()
    ))
}

pub fn new_process_token() -> String {
    hash_text(&new_operation_id())
}

pub fn now_text() -> String {
    use time::{format_description::well_known::Rfc3339, OffsetDateTime};
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}

fn hash_text(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
