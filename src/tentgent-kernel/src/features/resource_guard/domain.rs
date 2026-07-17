use std::fmt;

use serde::{Deserialize, Serialize};

use crate::features::{
    cluster::domain::ClusterDefinition,
    model::domain::ModelCapability,
    resource_coordination::{ResourceBusy, ResourcePermit},
};
use crate::foundation::error::{KernelError, KernelResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceOperation {
    DeleteModel {
        model_ref: String,
    },
    RemoveModelCapability {
        model_ref: String,
        capability: ModelCapability,
    },
    ReplaceModelCapabilities {
        model_ref: String,
        removed_capabilities: Vec<ModelCapability>,
    },
    DeleteAdapter {
        adapter_ref: String,
        base_model_ref: Option<String>,
        capability: Option<ModelCapability>,
    },
    RebindAdapter {
        adapter_ref: String,
        old_base_model_ref: Option<String>,
        new_base_model_ref: Option<String>,
        capability: Option<ModelCapability>,
    },
    DeleteDataset {
        dataset_ref: String,
    },
    DeleteTrainPlan {
        plan_ref: String,
    },
    DeleteCluster {
        cluster_ref: String,
    },
    ReplaceCluster {
        cluster_ref: String,
        definition: ClusterDefinition,
    },
    DeleteServerSpec {
        server_ref: String,
    },
}

impl ResourceOperation {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::DeleteModel { .. } => "delete-model",
            Self::RemoveModelCapability { .. } => "remove-model-capability",
            Self::ReplaceModelCapabilities { .. } => "replace-model-capabilities",
            Self::DeleteAdapter { .. } => "delete-adapter",
            Self::RebindAdapter { .. } => "rebind-adapter",
            Self::DeleteDataset { .. } => "delete-dataset",
            Self::DeleteTrainPlan { .. } => "delete-train-plan",
            Self::DeleteCluster { .. } => "delete-cluster",
            Self::ReplaceCluster { .. } => "replace-cluster",
            Self::DeleteServerSpec { .. } => "delete-server-spec",
        }
    }

    pub fn resource_ref(&self) -> &str {
        match self {
            Self::DeleteModel { model_ref }
            | Self::RemoveModelCapability { model_ref, .. }
            | Self::ReplaceModelCapabilities { model_ref, .. } => model_ref,
            Self::DeleteAdapter { adapter_ref, .. } | Self::RebindAdapter { adapter_ref, .. } => {
                adapter_ref
            }
            Self::DeleteDataset { dataset_ref } => dataset_ref,
            Self::DeleteTrainPlan { plan_ref } => plan_ref,
            Self::DeleteCluster { cluster_ref } | Self::ReplaceCluster { cluster_ref, .. } => {
                cluster_ref
            }
            Self::DeleteServerSpec { server_ref } => server_ref,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceGuardCode {
    #[serde(rename = "model_in_use")]
    ModelInUse,
    #[serde(rename = "capability_in_use")]
    CapabilityInUse,
    #[serde(rename = "adapter_in_use")]
    AdapterInUse,
    #[serde(rename = "dataset_in_use")]
    DatasetInUse,
    #[serde(rename = "train_plan_in_use")]
    TrainPlanInUse,
    #[serde(rename = "cluster_in_use")]
    ClusterInUse,
    #[serde(rename = "server_in_use")]
    ServerInUse,
}

impl ResourceGuardCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ModelInUse => "model_in_use",
            Self::CapabilityInUse => "capability_in_use",
            Self::AdapterInUse => "adapter_in_use",
            Self::DatasetInUse => "dataset_in_use",
            Self::TrainPlanInUse => "train_plan_in_use",
            Self::ClusterInUse => "cluster_in_use",
            Self::ServerInUse => "server_in_use",
        }
    }
}

impl fmt::Display for ResourceGuardCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceBlockerCode {
    ModelInUse,
    CapabilityInUse,
    AdapterInUse,
    AdapterRebindInUse,
    DatasetInUse,
    ClusterInUse,
    ServerRunning,
    TrainRunActive,
    RuntimeResourceOwned,
    OwnershipUnreadable,
    ClusterRouteUpdateBlocked,
}

impl ResourceBlockerCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ModelInUse => "model-in-use",
            Self::CapabilityInUse => "capability-in-use",
            Self::AdapterInUse => "adapter-in-use",
            Self::AdapterRebindInUse => "adapter-rebind-in-use",
            Self::DatasetInUse => "dataset-in-use",
            Self::ClusterInUse => "cluster-in-use",
            Self::ServerRunning => "server-running",
            Self::TrainRunActive => "train-run-active",
            Self::RuntimeResourceOwned => "runtime-resource-owned",
            Self::OwnershipUnreadable => "ownership-unreadable",
            Self::ClusterRouteUpdateBlocked => "cluster-route-update-blocked",
        }
    }
}

impl fmt::Display for ResourceBlockerCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceBlocker {
    pub kind: String,
    pub code: ResourceBlockerCode,
    pub reference: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<String>,
}

impl ResourceBlocker {
    pub fn sort_key(&self) -> (&str, &str, &str, &str, &str, &str) {
        (
            &self.kind,
            self.owner.as_deref().unwrap_or(""),
            &self.reference,
            self.route.as_deref().unwrap_or(""),
            self.field.as_deref().unwrap_or(""),
            self.capability.as_deref().unwrap_or(""),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceGuardRejection {
    pub code: ResourceGuardCode,
    pub operation: String,
    pub resource: String,
    pub description: String,
    pub blockers: Vec<ResourceBlocker>,
}

#[derive(Debug)]
pub struct ResourceMutationPermit {
    operation: ResourceOperation,
    coordination: ResourcePermit,
}

impl ResourceMutationPermit {
    pub(crate) fn new(operation: ResourceOperation, coordination: ResourcePermit) -> Self {
        Self {
            operation,
            coordination,
        }
    }

    pub fn operation(&self) -> &ResourceOperation {
        &self.operation
    }

    pub fn operation_id(&self) -> &str {
        self.coordination.operation_id()
    }
}

#[derive(Debug)]
pub enum ResourceMutationAuthorization {
    Permitted(ResourceMutationPermit),
    Rejected(ResourceGuardRejection),
    Busy(ResourceBusy),
}

#[derive(Debug)]
pub enum ResourceMutationOutcome<T> {
    Applied(T),
    Blocked(ResourceGuardRejection),
    Busy(ResourceBusy),
}

impl<T> ResourceMutationOutcome<T> {
    pub fn applied(self) -> Option<T> {
        match self {
            Self::Applied(value) => Some(value),
            Self::Blocked(_) | Self::Busy(_) => None,
        }
    }

    pub fn into_compat_result(self) -> KernelResult<T> {
        match self {
            Self::Applied(value) => Ok(value),
            Self::Blocked(rejection) => Err(KernelError::ResourceOperationBlocked {
                operation: rejection.operation,
                resource: rejection.resource,
                blockers: rejection
                    .blockers
                    .iter()
                    .map(|blocker| {
                        let capability = blocker
                            .capability
                            .as_deref()
                            .map(|value| format!("model capability `{value}`: "))
                            .unwrap_or_default();
                        format!(
                            "{capability}{} {} ({})",
                            blocker.kind, blocker.reference, blocker.reason
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            }),
            Self::Busy(busy) => Err(KernelError::ResourceCoordinationUnavailable(format!(
                "{}: {}; retry after {} ms",
                busy.key.label(),
                busy.description,
                busy.retry_after_millis
            ))),
        }
    }
}

pub(crate) fn blocker(
    operation: &ResourceOperation,
    kind: impl Into<String>,
    code: ResourceBlockerCode,
    reference: impl Into<String>,
    reason: impl Into<String>,
) -> ResourceBlocker {
    ResourceBlocker {
        kind: kind.into(),
        code,
        reference: reference.into(),
        reason: reason.into(),
        resource_kind: None,
        resource_ref: Some(operation.resource_ref().to_string()),
        operation: Some(operation.code().to_string()),
        capability: None,
        route: None,
        field: None,
        owner: None,
        next_actions: Vec::new(),
    }
}
