//! Cluster use case ports.

use std::path::PathBuf;

use crate::features::cluster::domain::{
    ClusterDefinition, ClusterInspection, ClusterRef, ClusterRemoveOutcome, ClusterStoreLayout,
    ClusterSummary,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterListRequest {
    pub layout: RuntimeLayoutInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterListResult {
    pub layout: RuntimeLayout,
    pub store: ClusterStoreLayout,
    pub clusters: Vec<ClusterSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterInspectRequest {
    pub layout: RuntimeLayoutInput,
    pub cluster_ref: ClusterRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterInspectResult {
    pub layout: RuntimeLayout,
    pub store: ClusterStoreLayout,
    pub inspection: ClusterInspection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterApplyFileRequest {
    pub layout: RuntimeLayoutInput,
    pub source_path: PathBuf,
    pub force_unsafe_source: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterApplyDefinitionRequest {
    pub layout: RuntimeLayoutInput,
    pub cluster_ref: ClusterRef,
    pub definition: ClusterDefinition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterApplyResult {
    pub layout: RuntimeLayout,
    pub store: ClusterStoreLayout,
    pub inspection: ClusterInspection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterValidateFileRequest {
    pub layout: RuntimeLayoutInput,
    pub source_path: PathBuf,
    pub force_unsafe_source: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterValidateResult {
    pub layout: RuntimeLayout,
    pub store: ClusterStoreLayout,
    pub definition: ClusterDefinition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterRemoveRequest {
    pub layout: RuntimeLayoutInput,
    pub cluster_ref: ClusterRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterRemoveResult {
    pub layout: RuntimeLayout,
    pub store: ClusterStoreLayout,
    pub outcome: ClusterRemoveOutcome,
}

/// Use-case boundary for stored cluster definitions.
pub trait ClusterSpecUseCase {
    /// Lists stored clusters.
    fn list_clusters(&self, request: ClusterListRequest) -> KernelResult<ClusterListResult>;

    /// Inspects one stored cluster.
    fn inspect_cluster(&self, request: ClusterInspectRequest)
        -> KernelResult<ClusterInspectResult>;

    /// Parses, validates, and stores a cluster definition file.
    fn apply_cluster_file(
        &self,
        request: ClusterApplyFileRequest,
    ) -> KernelResult<ClusterApplyResult>;

    /// Validates a cluster definition file without writing it.
    fn validate_cluster_file(
        &self,
        request: ClusterValidateFileRequest,
    ) -> KernelResult<ClusterValidateResult>;

    /// Validates and stores an already parsed cluster definition.
    fn apply_cluster_definition(
        &self,
        request: ClusterApplyDefinitionRequest,
    ) -> KernelResult<ClusterApplyResult>;

    /// Removes one stored cluster.
    fn remove_cluster(&self, request: ClusterRemoveRequest) -> KernelResult<ClusterRemoveResult>;
}
