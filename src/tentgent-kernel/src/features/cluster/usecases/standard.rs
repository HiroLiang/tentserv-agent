//! Standard cluster definition orchestration.

use std::sync::Arc;

use crate::features::cluster::ports::{ClusterCatalogStore, ClusterStoreLayoutInitializer};
use crate::features::model::ports::ModelCatalogStore;
use crate::features::resource_guard::{
    ResourceGuardUseCase, ResourceMutationAuthorization, ResourceMutationOutcome,
    ResourceOperation, StdResourceGuard,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::{
    cluster_store_layout, read_cluster_definition_file, validate_cluster_definition,
};
use super::port::{
    ClusterApplyDefinitionRequest, ClusterApplyFileRequest, ClusterApplyResult,
    ClusterInspectRequest, ClusterInspectResult, ClusterListRequest, ClusterListResult,
    ClusterRemoveRequest, ClusterRemoveResult, ClusterSpecUseCase, ClusterValidateFileRequest,
    ClusterValidateResult,
};

/// Standard orchestration for cluster definitions.
pub struct StdClusterUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    layout_initializer: &'a dyn ClusterStoreLayoutInitializer,
    catalog: &'a dyn ClusterCatalogStore,
    model_catalog: &'a dyn ModelCatalogStore,
    guard: Arc<dyn ResourceGuardUseCase>,
}

impl<'a> StdClusterUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        layout_initializer: &'a dyn ClusterStoreLayoutInitializer,
        catalog: &'a dyn ClusterCatalogStore,
        model_catalog: &'a dyn ModelCatalogStore,
    ) -> Self {
        Self::new_with_dependencies(
            layout_resolver,
            layout_initializer,
            catalog,
            model_catalog,
            Arc::new(StdResourceGuard::default()),
        )
    }

    pub fn new_with_dependencies(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        layout_initializer: &'a dyn ClusterStoreLayoutInitializer,
        catalog: &'a dyn ClusterCatalogStore,
        model_catalog: &'a dyn ModelCatalogStore,
        guard: Arc<dyn ResourceGuardUseCase>,
    ) -> Self {
        Self {
            layout_resolver,
            layout_initializer,
            catalog,
            model_catalog,
            guard,
        }
    }

    pub fn apply_cluster_file_guarded(
        &self,
        request: ClusterApplyFileRequest,
    ) -> KernelResult<ResourceMutationOutcome<ClusterApplyResult>> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = cluster_store_layout(&layout);
        let definition =
            read_cluster_definition_file(&request.source_path, request.force_unsafe_source)?;
        let definition =
            validate_cluster_definition(definition, None, &layout, self.model_catalog)?;
        let authorization = self.guard.authorize(
            &layout,
            ResourceOperation::ReplaceCluster {
                cluster_ref: definition.cluster_ref.to_string(),
                definition: definition.clone(),
            },
        )?;
        let _permit = match authorization {
            ResourceMutationAuthorization::Permitted(permit) => permit,
            ResourceMutationAuthorization::Rejected(rejection) => {
                return Ok(ResourceMutationOutcome::Blocked(rejection));
            }
            ResourceMutationAuthorization::Busy(busy) => {
                return Ok(ResourceMutationOutcome::Busy(busy));
            }
        };
        let definition =
            validate_cluster_definition(definition, None, &layout, self.model_catalog)?;
        self.layout_initializer
            .ensure_cluster_store_layout(&store)?;
        let inspection = self.catalog.save_cluster(&store, &definition)?;
        Ok(ResourceMutationOutcome::Applied(ClusterApplyResult {
            layout,
            store,
            inspection,
        }))
    }

    pub fn apply_cluster_definition_guarded(
        &self,
        request: ClusterApplyDefinitionRequest,
    ) -> KernelResult<ResourceMutationOutcome<ClusterApplyResult>> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = cluster_store_layout(&layout);
        let definition = validate_cluster_definition(
            request.definition,
            Some(&request.cluster_ref),
            &layout,
            self.model_catalog,
        )?;
        let authorization = self.guard.authorize(
            &layout,
            ResourceOperation::ReplaceCluster {
                cluster_ref: definition.cluster_ref.to_string(),
                definition: definition.clone(),
            },
        )?;
        let _permit = match authorization {
            ResourceMutationAuthorization::Permitted(permit) => permit,
            ResourceMutationAuthorization::Rejected(rejection) => {
                return Ok(ResourceMutationOutcome::Blocked(rejection));
            }
            ResourceMutationAuthorization::Busy(busy) => {
                return Ok(ResourceMutationOutcome::Busy(busy));
            }
        };
        let definition = validate_cluster_definition(
            definition,
            Some(&request.cluster_ref),
            &layout,
            self.model_catalog,
        )?;
        self.layout_initializer
            .ensure_cluster_store_layout(&store)?;
        let inspection = self.catalog.save_cluster(&store, &definition)?;
        Ok(ResourceMutationOutcome::Applied(ClusterApplyResult {
            layout,
            store,
            inspection,
        }))
    }

    pub fn remove_cluster_guarded(
        &self,
        request: ClusterRemoveRequest,
    ) -> KernelResult<ResourceMutationOutcome<ClusterRemoveResult>> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = cluster_store_layout(&layout);
        let authorization = self.guard.authorize(
            &layout,
            ResourceOperation::DeleteCluster {
                cluster_ref: request.cluster_ref.to_string(),
            },
        )?;
        let _permit = match authorization {
            ResourceMutationAuthorization::Permitted(permit) => permit,
            ResourceMutationAuthorization::Rejected(rejection) => {
                return Ok(ResourceMutationOutcome::Blocked(rejection));
            }
            ResourceMutationAuthorization::Busy(busy) => {
                return Ok(ResourceMutationOutcome::Busy(busy));
            }
        };
        let outcome = self.catalog.remove_cluster(&store, &request.cluster_ref)?;
        Ok(ResourceMutationOutcome::Applied(ClusterRemoveResult {
            layout,
            store,
            outcome,
        }))
    }
}

impl ClusterSpecUseCase for StdClusterUseCase<'_> {
    fn list_clusters(&self, request: ClusterListRequest) -> KernelResult<ClusterListResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = cluster_store_layout(&layout);
        let clusters = self.catalog.list_clusters(&store)?;
        Ok(ClusterListResult {
            layout,
            store,
            clusters,
        })
    }

    fn inspect_cluster(
        &self,
        request: ClusterInspectRequest,
    ) -> KernelResult<ClusterInspectResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = cluster_store_layout(&layout);
        let inspection = self.catalog.inspect_cluster(&store, &request.cluster_ref)?;
        Ok(ClusterInspectResult {
            layout,
            store,
            inspection,
        })
    }

    fn apply_cluster_file(
        &self,
        request: ClusterApplyFileRequest,
    ) -> KernelResult<ClusterApplyResult> {
        self.apply_cluster_file_guarded(request)?
            .into_compat_result()
    }

    fn validate_cluster_file(
        &self,
        request: ClusterValidateFileRequest,
    ) -> KernelResult<ClusterValidateResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = cluster_store_layout(&layout);
        let definition =
            read_cluster_definition_file(&request.source_path, request.force_unsafe_source)?;
        let definition =
            validate_cluster_definition(definition, None, &layout, self.model_catalog)?;
        Ok(ClusterValidateResult {
            layout,
            store,
            definition,
        })
    }

    fn apply_cluster_definition(
        &self,
        request: ClusterApplyDefinitionRequest,
    ) -> KernelResult<ClusterApplyResult> {
        self.apply_cluster_definition_guarded(request)?
            .into_compat_result()
    }

    fn remove_cluster(&self, request: ClusterRemoveRequest) -> KernelResult<ClusterRemoveResult> {
        self.remove_cluster_guarded(request)?.into_compat_result()
    }
}
