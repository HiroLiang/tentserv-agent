use crate::{
    features::{
        resource_coordination::{ResourceBusy, ResourceLockMode, ResourceLockRequest},
        runtime_ownership::{
            now_text, sanitize_runtime_diagnostic, RuntimeExecutionIdentity,
            RuntimeGenerationEndpoint, RuntimeGenerationLaunchTarget, RuntimeGenerationOperation,
            RuntimeGenerationRecord, RuntimeGenerationState, RuntimeLaunchPolicyRecord,
            RuntimeOwnershipLayout,
        },
    },
    foundation::{error::KernelResult, layout::RuntimeLayout},
};

use super::StdRuntimeOwnershipUseCase;

#[derive(Debug)]
pub enum RuntimeGenerationAdmission {
    Start(RuntimeGenerationRecord),
    Reuse(RuntimeGenerationRecord),
    Starting(RuntimeGenerationRecord),
    Closing(RuntimeGenerationRecord),
    Busy(ResourceBusy),
}

#[derive(Debug)]
pub enum RuntimeGenerationTransition {
    Applied(RuntimeGenerationRecord),
    Missing,
    GenerationChanged(RuntimeGenerationRecord),
    Busy(ResourceBusy),
}

struct RuntimeGenerationUpdate {
    state: RuntimeGenerationState,
    launch_target: Option<RuntimeGenerationLaunchTarget>,
    endpoint: Option<RuntimeGenerationEndpoint>,
    diagnostic: Option<String>,
}

impl RuntimeGenerationUpdate {
    fn new(state: RuntimeGenerationState) -> Self {
        Self {
            state,
            launch_target: None,
            endpoint: None,
            diagnostic: None,
        }
    }
}

impl StdRuntimeOwnershipUseCase {
    pub fn adopt_runtime_generation(
        &self,
        layout: &RuntimeLayout,
        identity: RuntimeExecutionIdentity,
        policy: RuntimeLaunchPolicyRecord,
        endpoint: RuntimeGenerationEndpoint,
    ) -> KernelResult<RuntimeGenerationAdmission> {
        let ownership = RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir);
        self.store.ensure_layout(&ownership)?;
        let locks = generation_transition_locks(&identity);
        let permit = match self.coordinator.acquire(
            layout,
            ResourceLockRequest::new("adopt-runtime-generation", locks),
        )? {
            Ok(permit) => permit,
            Err(busy) => return Ok(RuntimeGenerationAdmission::Busy(busy)),
        };
        let _operation = self.begin_operation(
            layout,
            "adopt-runtime-generation",
            permit.operation_id().to_string(),
            permit.keys().to_vec(),
        )?;
        let runtime_key = identity.physical_key().identity;
        let admission = match self.store.read_generation(&ownership, &runtime_key)? {
            Some(record) => match record.state {
                RuntimeGenerationState::Ready => RuntimeGenerationAdmission::Reuse(record),
                RuntimeGenerationState::Starting => RuntimeGenerationAdmission::Starting(record),
                RuntimeGenerationState::Closing => RuntimeGenerationAdmission::Closing(record),
            },
            None => {
                let record = RuntimeGenerationRecord::adopted(identity, policy, endpoint);
                self.store.write_generation(&ownership, &record)?;
                RuntimeGenerationAdmission::Reuse(record)
            }
        };
        drop(permit);
        Ok(admission)
    }

    pub fn admit_runtime_generation(
        &self,
        layout: &RuntimeLayout,
        identity: RuntimeExecutionIdentity,
        policy: RuntimeLaunchPolicyRecord,
    ) -> KernelResult<RuntimeGenerationAdmission> {
        let ownership = RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir);
        self.store.ensure_layout(&ownership)?;
        let locks = generation_transition_locks(&identity);
        let permit = match self.coordinator.acquire(
            layout,
            ResourceLockRequest::new("start-runtime-generation", locks),
        )? {
            Ok(permit) => permit,
            Err(busy) => return Ok(RuntimeGenerationAdmission::Busy(busy)),
        };
        let _operation = self.begin_operation(
            layout,
            "start-runtime-generation",
            permit.operation_id().to_string(),
            permit.keys().to_vec(),
        )?;
        let runtime_key = identity.physical_key().identity;
        let admission = match self.store.read_generation(&ownership, &runtime_key)? {
            Some(record) => match record.state {
                RuntimeGenerationState::Ready => RuntimeGenerationAdmission::Reuse(record),
                RuntimeGenerationState::Starting => RuntimeGenerationAdmission::Starting(record),
                RuntimeGenerationState::Closing => RuntimeGenerationAdmission::Closing(record),
            },
            None => {
                let record = RuntimeGenerationRecord::starting(identity, policy);
                self.store.write_generation(&ownership, &record)?;
                RuntimeGenerationAdmission::Start(record)
            }
        };
        drop(permit);
        Ok(admission)
    }

    pub fn mark_runtime_ready(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
        endpoint: RuntimeGenerationEndpoint,
    ) -> KernelResult<RuntimeGenerationTransition> {
        self.transition_generation(
            layout,
            identity,
            generation_id,
            RuntimeGenerationUpdate {
                endpoint: Some(endpoint),
                ..RuntimeGenerationUpdate::new(RuntimeGenerationState::Ready)
            },
        )
    }

    pub fn prepare_runtime_launch(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
        launch_target: RuntimeGenerationLaunchTarget,
    ) -> KernelResult<RuntimeGenerationTransition> {
        self.transition_generation(
            layout,
            identity,
            generation_id,
            RuntimeGenerationUpdate {
                launch_target: Some(launch_target),
                ..RuntimeGenerationUpdate::new(RuntimeGenerationState::Starting)
            },
        )
    }

    pub fn mark_runtime_spawned(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
        endpoint: RuntimeGenerationEndpoint,
    ) -> KernelResult<RuntimeGenerationTransition> {
        self.transition_generation(
            layout,
            identity,
            generation_id,
            RuntimeGenerationUpdate {
                endpoint: Some(endpoint),
                ..RuntimeGenerationUpdate::new(RuntimeGenerationState::Starting)
            },
        )
    }

    pub fn mark_runtime_closing(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
    ) -> KernelResult<RuntimeGenerationTransition> {
        self.transition_generation(
            layout,
            identity,
            generation_id,
            RuntimeGenerationUpdate::new(RuntimeGenerationState::Closing),
        )
    }

    pub fn mark_runtime_closing_with_diagnostic(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
        diagnostic: impl AsRef<str>,
    ) -> KernelResult<RuntimeGenerationTransition> {
        self.transition_generation(
            layout,
            identity,
            generation_id,
            RuntimeGenerationUpdate {
                diagnostic: Some(sanitize_runtime_diagnostic(diagnostic)),
                ..RuntimeGenerationUpdate::new(RuntimeGenerationState::Closing)
            },
        )
    }

    fn transition_generation(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
        update: RuntimeGenerationUpdate,
    ) -> KernelResult<RuntimeGenerationTransition> {
        let ownership = RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir);
        let runtime_key = identity.physical_key();
        let permit = match self.coordinator.acquire(
            layout,
            ResourceLockRequest::new(
                "transition-runtime-generation",
                generation_transition_locks(identity),
            ),
        )? {
            Ok(permit) => permit,
            Err(busy) => return Ok(RuntimeGenerationTransition::Busy(busy)),
        };
        let _operation = self.begin_operation(
            layout,
            "transition-runtime-generation",
            permit.operation_id().to_string(),
            permit.keys().to_vec(),
        )?;
        let Some(mut record) = self
            .store
            .read_generation(&ownership, &runtime_key.identity)?
        else {
            return Ok(RuntimeGenerationTransition::Missing);
        };
        if record.generation_id != generation_id || record.identity != *identity {
            return Ok(RuntimeGenerationTransition::GenerationChanged(record));
        }
        record.state = update.state;
        record.operation = match update.state {
            RuntimeGenerationState::Starting => RuntimeGenerationOperation::Start,
            RuntimeGenerationState::Ready => RuntimeGenerationOperation::Ready,
            RuntimeGenerationState::Closing => RuntimeGenerationOperation::Close,
        };
        if let Some(launch_target) = update.launch_target {
            record.launch_target = Some(launch_target);
        }
        if let Some(endpoint) = update.endpoint {
            record.launch_target = Some(RuntimeGenerationLaunchTarget {
                host: endpoint.host.clone(),
                port: endpoint.port,
            });
            record.endpoint = Some(endpoint);
        }
        record.diagnostic = update.diagnostic;
        record.updated_at = now_text();
        self.store.write_generation(&ownership, &record)?;
        drop(permit);
        Ok(RuntimeGenerationTransition::Applied(record))
    }

    pub fn remove_runtime_generation(
        &self,
        layout: &RuntimeLayout,
        identity: &RuntimeExecutionIdentity,
        generation_id: &str,
    ) -> KernelResult<RuntimeGenerationTransition> {
        let ownership = RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir);
        let runtime_key = identity.physical_key();
        let permit = match self.coordinator.acquire(
            layout,
            ResourceLockRequest::new(
                "remove-runtime-generation",
                generation_transition_locks(identity),
            ),
        )? {
            Ok(permit) => permit,
            Err(busy) => return Ok(RuntimeGenerationTransition::Busy(busy)),
        };
        let _operation = self.begin_operation(
            layout,
            "remove-runtime-generation",
            permit.operation_id().to_string(),
            permit.keys().to_vec(),
        )?;
        let Some(record) = self
            .store
            .read_generation(&ownership, &runtime_key.identity)?
        else {
            return Ok(RuntimeGenerationTransition::Missing);
        };
        if record.generation_id != generation_id || record.identity != *identity {
            return Ok(RuntimeGenerationTransition::GenerationChanged(record));
        }
        self.store
            .remove_generation(&ownership, &runtime_key.identity)?;
        drop(permit);
        Ok(RuntimeGenerationTransition::Applied(record))
    }
}

fn generation_transition_locks(
    identity: &RuntimeExecutionIdentity,
) -> Vec<(
    crate::features::resource_coordination::ResourceKey,
    ResourceLockMode,
)> {
    identity
        .transition_keys()
        .into_iter()
        .map(|key| {
            let mode = if key.kind
                == crate::features::resource_coordination::ResourceKind::PhysicalRuntime
            {
                ResourceLockMode::Exclusive
            } else {
                ResourceLockMode::Shared
            };
            (key, mode)
        })
        .collect()
}
