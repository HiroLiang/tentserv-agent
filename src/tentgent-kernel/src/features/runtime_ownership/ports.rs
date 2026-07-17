use crate::foundation::error::KernelResult;

use super::{
    OwnershipOperationRecord, RouteGenerationClaim, RuntimeGenerationHealth,
    RuntimeGenerationRecord, RuntimeOwnershipIssue, RuntimeOwnershipLayout,
};

pub trait RuntimeOwnershipStore: Send + Sync {
    fn ensure_layout(&self, layout: &RuntimeOwnershipLayout) -> KernelResult<()>;
    fn list_claims(
        &self,
        layout: &RuntimeOwnershipLayout,
    ) -> KernelResult<(Vec<RouteGenerationClaim>, Vec<RuntimeOwnershipIssue>)>;
    fn read_claim(
        &self,
        layout: &RuntimeOwnershipLayout,
        owner_id: &str,
    ) -> KernelResult<Option<RouteGenerationClaim>>;
    fn write_claim(
        &self,
        layout: &RuntimeOwnershipLayout,
        claim: &RouteGenerationClaim,
    ) -> KernelResult<()>;
    fn remove_claim(&self, layout: &RuntimeOwnershipLayout, owner_id: &str) -> KernelResult<()>;
    fn list_generations(
        &self,
        layout: &RuntimeOwnershipLayout,
    ) -> KernelResult<(Vec<RuntimeGenerationRecord>, Vec<RuntimeOwnershipIssue>)>;
    fn read_generation(
        &self,
        layout: &RuntimeOwnershipLayout,
        runtime_key: &str,
    ) -> KernelResult<Option<RuntimeGenerationRecord>>;
    fn write_generation(
        &self,
        layout: &RuntimeOwnershipLayout,
        record: &RuntimeGenerationRecord,
    ) -> KernelResult<()>;
    fn remove_generation(
        &self,
        layout: &RuntimeOwnershipLayout,
        runtime_key: &str,
    ) -> KernelResult<()>;
    fn write_operation(
        &self,
        layout: &RuntimeOwnershipLayout,
        record: &OwnershipOperationRecord,
    ) -> KernelResult<()>;
    fn remove_operation(
        &self,
        layout: &RuntimeOwnershipLayout,
        operation_id: &str,
    ) -> KernelResult<()>;
    fn list_operations(
        &self,
        layout: &RuntimeOwnershipLayout,
    ) -> KernelResult<(Vec<OwnershipOperationRecord>, Vec<RuntimeOwnershipIssue>)>;
    fn quarantine_record(
        &self,
        layout: &RuntimeOwnershipLayout,
        source: &std::path::Path,
    ) -> KernelResult<String>;
    fn purge_quarantine_before(
        &self,
        layout: &RuntimeOwnershipLayout,
        invocation_marker: &str,
    ) -> KernelResult<Vec<String>>;
}

pub trait OwnershipProcessProbe: Send + Sync {
    fn is_process_running(&self, pid: u32) -> KernelResult<bool>;
}

pub trait RuntimeGenerationHealthProbe: Send + Sync {
    fn probe_generation_health(
        &self,
        record: &RuntimeGenerationRecord,
    ) -> KernelResult<RuntimeGenerationHealth>;
}
