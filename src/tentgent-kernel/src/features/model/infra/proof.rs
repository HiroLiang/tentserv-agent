//! Coordinated filesystem proof storage. Legacy generations share one transaction boundary.

mod legacy;
mod transaction;

use crate::features::model::{
    domain::{ModelCapability, ModelCapabilityProof, ModelMetadata, ModelRef},
    ports::ModelCapabilityProofStore,
    proof_context::ModelProofContext,
};
use crate::features::resource_coordination::ResourceLockMode;
use crate::foundation::error::KernelResult;

use super::error::model_store_error;
use transaction::transact;

#[derive(Debug, Clone, Copy, Default)]
pub struct FileModelCapabilityProofStore;

impl ModelCapabilityProofStore for FileModelCapabilityProofStore {
    fn list_capability_proofs(
        &self,
        context: &ModelProofContext<'_>,
        model_ref: &ModelRef,
    ) -> KernelResult<Vec<ModelCapabilityProof>> {
        transact(
            context,
            model_ref,
            None,
            ResourceLockMode::Shared,
            |store, _| legacy::list(store, model_ref, None, true),
        )
    }

    fn list_capability_proofs_for(
        &self,
        context: &ModelProofContext<'_>,
        model_ref: &ModelRef,
        capability: ModelCapability,
    ) -> KernelResult<Vec<ModelCapabilityProof>> {
        transact(
            context,
            model_ref,
            Some(capability),
            ResourceLockMode::Shared,
            |store, _| legacy::list(store, model_ref, Some(capability), true),
        )
    }

    fn list_support_proofs(
        &self,
        context: &ModelProofContext<'_>,
        model_ref: &ModelRef,
    ) -> KernelResult<Vec<ModelCapabilityProof>> {
        transact(
            context,
            model_ref,
            None,
            ResourceLockMode::Shared,
            |store, _| legacy::list(store, model_ref, None, false),
        )
    }

    fn save_support_proof(
        &self,
        context: &ModelProofContext<'_>,
        proof: &ModelCapabilityProof,
    ) -> KernelResult<()> {
        transact(
            context,
            &proof.model_ref,
            Some(proof.capability),
            ResourceLockMode::Exclusive,
            |store, metadata| {
                validate_observed_model(proof, metadata)?;
                legacy::save(store, proof, false)
            },
        )
    }

    fn save_capability_proof(
        &self,
        context: &ModelProofContext<'_>,
        proof: &ModelCapabilityProof,
    ) -> KernelResult<()> {
        transact(
            context,
            &proof.model_ref,
            Some(proof.capability),
            ResourceLockMode::Exclusive,
            |store, metadata| {
                validate_observed_model(proof, metadata)?;
                legacy::save(store, proof, true)
            },
        )
    }

    fn remove_capability_proof(
        &self,
        context: &ModelProofContext<'_>,
        model_ref: &ModelRef,
        capability: ModelCapability,
    ) -> KernelResult<usize> {
        transact(
            context,
            model_ref,
            Some(capability),
            ResourceLockMode::Exclusive,
            |store, _| {
                let count = legacy::list(store, model_ref, Some(capability), true)?.len();
                legacy::clear(store, model_ref, capability)?;
                Ok(count)
            },
        )
    }
}

fn validate_observed_model(
    proof: &ModelCapabilityProof,
    metadata: &ModelMetadata,
) -> KernelResult<()> {
    if proof.primary_format != metadata.primary_format
        || proof.mlx_runtime_family != metadata.mlx_runtime_family
    {
        return Err(model_store_error(
            "model execution metadata changed before proof persistence; resolve again",
        ));
    }
    Ok(())
}
