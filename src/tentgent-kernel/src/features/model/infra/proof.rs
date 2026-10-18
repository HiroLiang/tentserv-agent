//! Coordinated filesystem proof storage. Legacy and v2 share one transaction boundary.

mod legacy;
mod transaction;
mod v2;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod v2_tests;

use crate::features::model::{
    compatibility::{
        CompatibilityEvidence, CompatibilityFilter, CompatibilityProofKey, CompatibilityProofV2,
    },
    domain::{ModelCapability, ModelCapabilityProof, ModelMetadata, ModelRef},
    ports::{ModelCapabilityProofStore, ModelCompatibilityProofStore},
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
                let count = legacy::list(store, model_ref, Some(capability), true)?.len()
                    + v2::list(
                        store,
                        model_ref,
                        &CompatibilityFilter {
                            capability: Some(capability),
                            ..Default::default()
                        },
                    )?
                    .len();
                legacy::clear(store, model_ref, capability)?;
                v2::clear(store, model_ref, capability)?;
                Ok(count)
            },
        )
    }
}

impl ModelCompatibilityProofStore for FileModelCapabilityProofStore {
    fn get_exact(
        &self,
        context: &ModelProofContext<'_>,
        key: &CompatibilityProofKey,
    ) -> KernelResult<Option<CompatibilityProofV2>> {
        transact(
            context,
            key.model_ref(),
            Some(key.capability()),
            ResourceLockMode::Shared,
            |store, _| v2::get(store, key),
        )
    }

    fn save_exact(
        &self,
        context: &ModelProofContext<'_>,
        proof: &CompatibilityProofV2,
    ) -> KernelResult<CompatibilityProofKey> {
        transact(
            context,
            proof.tuple().model_ref(),
            Some(proof.tuple().capability()),
            ResourceLockMode::Exclusive,
            |store, metadata| {
                if proof.tuple().components().primary_format != metadata.primary_format {
                    return Err(model_store_error("model format changed before compatibility proof persistence; resolve again"));
                }
                v2::save(store, proof)
            },
        )
    }

    fn list_exact(
        &self,
        context: &ModelProofContext<'_>,
        model_ref: &ModelRef,
        filter: &CompatibilityFilter,
    ) -> KernelResult<Vec<CompatibilityProofV2>> {
        let capability = filter
            .exact_key
            .as_ref()
            .map(|key| key.capability())
            .or(filter.capability);
        transact(
            context,
            model_ref,
            capability,
            ResourceLockMode::Shared,
            |store, _| v2::list(store, model_ref, filter),
        )
    }

    fn remove_exact(
        &self,
        context: &ModelProofContext<'_>,
        key: &CompatibilityProofKey,
    ) -> KernelResult<bool> {
        transact(
            context,
            key.model_ref(),
            Some(key.capability()),
            ResourceLockMode::Exclusive,
            |store, _| v2::remove(store, key),
        )
    }

    fn list_evidence(
        &self,
        context: &ModelProofContext<'_>,
        model_ref: &ModelRef,
        capability: Option<ModelCapability>,
    ) -> KernelResult<Vec<CompatibilityEvidence>> {
        transact(
            context,
            model_ref,
            capability,
            ResourceLockMode::Shared,
            |store, _| {
                let mut records = v2::list(
                    store,
                    model_ref,
                    &CompatibilityFilter {
                        capability,
                        ..Default::default()
                    },
                )?
                .into_iter()
                .map(CompatibilityEvidence::from_v2)
                .collect::<Vec<_>>();
                records.extend(legacy::evidence(store, model_ref, capability)?);
                Ok(records)
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
