use crate::{
    features::{
        resource_coordination::ResourceBusy,
        runtime_ownership::{
            RuntimeExecutionIdentity, RuntimeGenerationEndpoint, RuntimeGenerationLaunchTarget,
            RuntimeLaunchPolicyRecord, RuntimeOwnershipInspection, RuntimeOwnershipScope,
            RuntimeOwnershipView, RuntimeReconcileRequest, RuntimeReconcileResult,
        },
    },
    foundation::{error::KernelResult, layout::RuntimeLayout},
};

use super::{
    RouteClaimAcquireRequest, RouteClaimTransition, RuntimeGenerationAdmission,
    RuntimeGenerationTransition, StdRuntimeOwnershipUseCase,
};

pub trait RouteClaimOwnershipUseCase: Send + Sync {
    fn acquire_route_claim(
        &self,
        layout: &RuntimeLayout,
        request: RouteClaimAcquireRequest,
    ) -> KernelResult<RouteClaimTransition>;

    fn retire_route_claim(
        &self,
        layout: &RuntimeLayout,
        owner_id: &str,
    ) -> KernelResult<Option<ResourceBusy>>;

    fn release_route_claim(
        &self,
        layout: &RuntimeLayout,
        owner_id: &str,
    ) -> KernelResult<Option<ResourceBusy>>;
}

pub trait RuntimeGenerationOwnershipUseCase: Send + Sync {
    fn adopt_runtime_generation(
        &self,
        layout: &RuntimeLayout,
        identity: RuntimeExecutionIdentity,
        policy: RuntimeLaunchPolicyRecord,
        endpoint: RuntimeGenerationEndpoint,
    ) -> KernelResult<RuntimeGenerationAdmission>;

    fn admit_runtime_generation(
        &self,
        layout: &RuntimeLayout,
        identity: RuntimeExecutionIdentity,
        policy: RuntimeLaunchPolicyRecord,
    ) -> KernelResult<RuntimeGenerationAdmission>;

    fn mark_runtime_ready(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
        endpoint: RuntimeGenerationEndpoint,
    ) -> KernelResult<RuntimeGenerationTransition>;

    fn prepare_runtime_launch(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
        launch_target: RuntimeGenerationLaunchTarget,
    ) -> KernelResult<RuntimeGenerationTransition>;

    fn mark_runtime_spawned(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
        endpoint: RuntimeGenerationEndpoint,
    ) -> KernelResult<RuntimeGenerationTransition>;

    fn mark_runtime_closing(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
    ) -> KernelResult<RuntimeGenerationTransition>;

    fn mark_runtime_closing_with_diagnostic(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
        diagnostic: &str,
    ) -> KernelResult<RuntimeGenerationTransition>;

    fn remove_runtime_generation(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
    ) -> KernelResult<RuntimeGenerationTransition>;
}

pub trait RuntimeOwnershipInspectionUseCase: Send + Sync {
    fn inspect_runtime_ownership(
        &self,
        layout: &RuntimeLayout,
    ) -> KernelResult<RuntimeOwnershipInspection>;

    fn summarize_runtime_ownership(
        &self,
        layout: &RuntimeLayout,
    ) -> KernelResult<RuntimeOwnershipInspection>;

    fn inspect_runtime_ownership_scope(
        &self,
        layout: &RuntimeLayout,
        scope: RuntimeOwnershipScope,
    ) -> KernelResult<RuntimeOwnershipView>;
}

pub trait RuntimeOwnershipReconcileUseCase: Send + Sync {
    fn reconcile_runtime_ownership(
        &self,
        layout: &RuntimeLayout,
        request: RuntimeReconcileRequest,
    ) -> KernelResult<RuntimeReconcileResult>;
}

impl RouteClaimOwnershipUseCase for StdRuntimeOwnershipUseCase {
    fn acquire_route_claim(
        &self,
        layout: &RuntimeLayout,
        request: RouteClaimAcquireRequest,
    ) -> KernelResult<RouteClaimTransition> {
        StdRuntimeOwnershipUseCase::acquire_route_claim(self, layout, request)
    }

    fn retire_route_claim(
        &self,
        layout: &RuntimeLayout,
        owner_id: &str,
    ) -> KernelResult<Option<ResourceBusy>> {
        StdRuntimeOwnershipUseCase::retire_route_claim(self, layout, owner_id)
    }

    fn release_route_claim(
        &self,
        layout: &RuntimeLayout,
        owner_id: &str,
    ) -> KernelResult<Option<ResourceBusy>> {
        StdRuntimeOwnershipUseCase::release_route_claim(self, layout, owner_id)
    }
}

impl RuntimeGenerationOwnershipUseCase for StdRuntimeOwnershipUseCase {
    fn adopt_runtime_generation(
        &self,
        layout: &RuntimeLayout,
        identity: RuntimeExecutionIdentity,
        policy: RuntimeLaunchPolicyRecord,
        endpoint: RuntimeGenerationEndpoint,
    ) -> KernelResult<RuntimeGenerationAdmission> {
        StdRuntimeOwnershipUseCase::adopt_runtime_generation(
            self, layout, identity, policy, endpoint,
        )
    }

    fn admit_runtime_generation(
        &self,
        layout: &RuntimeLayout,
        identity: RuntimeExecutionIdentity,
        policy: RuntimeLaunchPolicyRecord,
    ) -> KernelResult<RuntimeGenerationAdmission> {
        StdRuntimeOwnershipUseCase::admit_runtime_generation(self, layout, identity, policy)
    }

    fn mark_runtime_ready(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
        endpoint: RuntimeGenerationEndpoint,
    ) -> KernelResult<RuntimeGenerationTransition> {
        StdRuntimeOwnershipUseCase::mark_runtime_ready(
            self,
            layout,
            identity,
            generation_id,
            endpoint,
        )
    }

    fn prepare_runtime_launch(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
        launch_target: RuntimeGenerationLaunchTarget,
    ) -> KernelResult<RuntimeGenerationTransition> {
        StdRuntimeOwnershipUseCase::prepare_runtime_launch(
            self,
            layout,
            identity,
            generation_id,
            launch_target,
        )
    }

    fn mark_runtime_spawned(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
        endpoint: RuntimeGenerationEndpoint,
    ) -> KernelResult<RuntimeGenerationTransition> {
        StdRuntimeOwnershipUseCase::mark_runtime_spawned(
            self,
            layout,
            identity,
            generation_id,
            endpoint,
        )
    }

    fn mark_runtime_closing(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
    ) -> KernelResult<RuntimeGenerationTransition> {
        StdRuntimeOwnershipUseCase::mark_runtime_closing(self, layout, identity, generation_id)
    }

    fn mark_runtime_closing_with_diagnostic(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
        diagnostic: &str,
    ) -> KernelResult<RuntimeGenerationTransition> {
        StdRuntimeOwnershipUseCase::mark_runtime_closing_with_diagnostic(
            self,
            layout,
            identity,
            generation_id,
            diagnostic,
        )
    }

    fn remove_runtime_generation(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
    ) -> KernelResult<RuntimeGenerationTransition> {
        StdRuntimeOwnershipUseCase::remove_runtime_generation(self, layout, identity, generation_id)
    }
}

impl RuntimeOwnershipInspectionUseCase for StdRuntimeOwnershipUseCase {
    fn inspect_runtime_ownership(
        &self,
        layout: &RuntimeLayout,
    ) -> KernelResult<RuntimeOwnershipInspection> {
        StdRuntimeOwnershipUseCase::inspect_runtime_ownership(self, layout)
    }

    fn summarize_runtime_ownership(
        &self,
        layout: &RuntimeLayout,
    ) -> KernelResult<RuntimeOwnershipInspection> {
        StdRuntimeOwnershipUseCase::summarize_runtime_ownership(self, layout)
    }

    fn inspect_runtime_ownership_scope(
        &self,
        layout: &RuntimeLayout,
        scope: RuntimeOwnershipScope,
    ) -> KernelResult<RuntimeOwnershipView> {
        StdRuntimeOwnershipUseCase::inspect_runtime_ownership_scope(self, layout, scope)
    }
}

impl RuntimeOwnershipReconcileUseCase for StdRuntimeOwnershipUseCase {
    fn reconcile_runtime_ownership(
        &self,
        layout: &RuntimeLayout,
        request: RuntimeReconcileRequest,
    ) -> KernelResult<RuntimeReconcileResult> {
        StdRuntimeOwnershipUseCase::reconcile_runtime_ownership(self, layout, request)
    }
}
