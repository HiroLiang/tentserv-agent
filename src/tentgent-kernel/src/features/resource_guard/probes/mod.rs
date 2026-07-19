//! Narrow file-backed reference probes.

use crate::features::runtime_ownership::{OwnershipProcessProbe, RuntimeOwnershipStore};

mod adapter_bindings;
mod cluster_routes;
mod ownership;
mod server_specs;
mod train_refs;

pub(crate) use adapter_bindings::adapter_binding_blockers;
pub(crate) use cluster_routes::cluster_route_blockers;
pub(crate) use ownership::ownership_blockers;
pub(crate) use server_specs::server_spec_blockers;
pub(crate) use train_refs::train_reference_blockers;

pub(crate) struct ResourceGuardProbeContext<'a> {
    pub ownership_store: &'a dyn RuntimeOwnershipStore,
    pub process_probe: &'a dyn OwnershipProcessProbe,
}
