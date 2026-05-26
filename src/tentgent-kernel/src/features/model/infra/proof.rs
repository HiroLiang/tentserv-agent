use std::fs;

use crate::features::model::domain::{
    ModelCapability, ModelCapabilityProof, ModelRef, ModelStoreLayout,
};
use crate::features::model::ports::ModelCapabilityProofStore;
use crate::foundation::error::KernelResult;

use super::error::{model_store_error, path_error};

/// Filesystem-backed latest model capability proof store.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileModelCapabilityProofStore;

impl ModelCapabilityProofStore for FileModelCapabilityProofStore {
    fn list_capability_proofs(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
    ) -> KernelResult<Vec<ModelCapabilityProof>> {
        let proof_dir = layout.capability_proofs_dir(model_ref);
        let mut proofs = Vec::new();
        if !proof_dir.exists() {
            return Ok(proofs);
        }

        for entry in fs::read_dir(&proof_dir)
            .map_err(|err| path_error("read model capability proof directory", &proof_dir, err))?
        {
            let entry = entry.map_err(|err| {
                model_store_error(format!(
                    "read entry in model capability proof directory `{}` failed: {err}",
                    proof_dir.display()
                ))
            })?;
            let file_type = entry.file_type().map_err(|err| {
                path_error(
                    "read model capability proof entry type",
                    entry.path().as_path(),
                    err,
                )
            })?;
            if !file_type.is_file() {
                continue;
            }
            if entry.path().extension().and_then(|value| value.to_str()) != Some("toml") {
                continue;
            }

            let body = fs::read_to_string(entry.path())
                .map_err(|err| path_error("read model capability proof", &entry.path(), err))?;
            let proof = toml::from_str::<ModelCapabilityProof>(&body).map_err(|err| {
                model_store_error(format!(
                    "parse model capability proof `{}` failed: {err}",
                    entry.path().display()
                ))
            })?;
            proofs.push(proof);
        }

        proofs.sort_by(|left, right| left.capability.as_str().cmp(right.capability.as_str()));
        Ok(proofs)
    }

    fn save_capability_proof(
        &self,
        layout: &ModelStoreLayout,
        proof: &ModelCapabilityProof,
    ) -> KernelResult<()> {
        let path = layout.capability_proof_path(&proof.model_ref, proof.capability);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                path_error(
                    "create model capability proof parent directory",
                    parent,
                    err,
                )
            })?;
        }
        let body = toml::to_string_pretty(proof).map_err(|err| {
            model_store_error(format!("serialize model capability proof failed: {err}"))
        })?;
        fs::write(&path, body)
            .map_err(|err| path_error("write model capability proof", &path, err))?;
        Ok(())
    }

    fn remove_capability_proof(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
        capability: ModelCapability,
    ) -> KernelResult<()> {
        let path = layout.capability_proof_path(model_ref, capability);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(path_error("remove model capability proof", &path, err)),
        }
    }
}
