use std::fs;
use std::path::Path;

use crate::features::model::domain::{
    ModelCapability, ModelCapabilityProof, ModelCapabilityProofKey, ModelRef, ModelStoreLayout,
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
        let mut proofs = self.list_support_proofs(layout, model_ref)?;
        for legacy in list_legacy_capability_proofs(layout, model_ref)? {
            let legacy_key = ModelCapabilityProofKey::from_proof(&legacy);
            if !proofs
                .iter()
                .any(|proof| ModelCapabilityProofKey::from_proof(proof) == legacy_key)
            {
                proofs.push(legacy);
            }
        }

        sort_proofs(&mut proofs);
        Ok(proofs)
    }

    fn save_support_proof(
        &self,
        layout: &ModelStoreLayout,
        proof: &ModelCapabilityProof,
    ) -> KernelResult<()> {
        let key = ModelCapabilityProofKey::from_proof(proof);
        let path = layout.support_proof_path(&key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                path_error("create model support proof parent directory", parent, err)
            })?;
        }
        let body = toml::to_string_pretty(proof).map_err(|err| {
            model_store_error(format!("serialize model support proof failed: {err}"))
        })?;
        fs::write(&path, body)
            .map_err(|err| path_error("write model support proof", &path, err))?;
        Ok(())
    }

    fn list_support_proofs(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
    ) -> KernelResult<Vec<ModelCapabilityProof>> {
        let proof_dir = layout.support_proofs_dir(model_ref);
        let mut proofs = Vec::new();
        if !proof_dir.exists() {
            return Ok(proofs);
        }

        for entry in fs::read_dir(&proof_dir)
            .map_err(|err| path_error("read model support proof directory", &proof_dir, err))?
        {
            let entry = entry.map_err(|err| {
                model_store_error(format!(
                    "read entry in model support proof directory `{}` failed: {err}",
                    proof_dir.display()
                ))
            })?;
            let file_type = entry.file_type().map_err(|err| {
                path_error(
                    "read model support proof entry type",
                    entry.path().as_path(),
                    err,
                )
            })?;
            if !file_type.is_dir() {
                continue;
            }

            read_proofs_from_dir(&entry.path(), &mut proofs)?;
        }

        sort_proofs(&mut proofs);
        Ok(proofs)
    }

    fn save_capability_proof(
        &self,
        layout: &ModelStoreLayout,
        proof: &ModelCapabilityProof,
    ) -> KernelResult<()> {
        self.save_support_proof(layout, proof)?;

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
        }?;

        let support_dir = layout.support_proofs_capability_dir(model_ref, capability);
        match fs::remove_dir_all(&support_dir) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(path_error(
                "remove model support proof directory",
                &support_dir,
                err,
            )),
        }
    }
}

fn list_legacy_capability_proofs(
    layout: &ModelStoreLayout,
    model_ref: &ModelRef,
) -> KernelResult<Vec<ModelCapabilityProof>> {
    let proof_dir = layout.capability_proofs_dir(model_ref);
    let mut proofs = Vec::new();
    if !proof_dir.exists() {
        return Ok(proofs);
    }

    read_proofs_from_dir(&proof_dir, &mut proofs)?;
    sort_proofs(&mut proofs);
    Ok(proofs)
}

fn read_proofs_from_dir(path: &Path, proofs: &mut Vec<ModelCapabilityProof>) -> KernelResult<()> {
    for entry in
        fs::read_dir(path).map_err(|err| path_error("read model proof directory", path, err))?
    {
        let entry = entry.map_err(|err| {
            model_store_error(format!(
                "read entry in model proof directory `{}` failed: {err}",
                path.display()
            ))
        })?;
        let file_type = entry.file_type().map_err(|err| {
            path_error("read model proof entry type", entry.path().as_path(), err)
        })?;
        if !file_type.is_file() {
            continue;
        }
        if entry.path().extension().and_then(|value| value.to_str()) != Some("toml") {
            continue;
        }

        let body = fs::read_to_string(entry.path())
            .map_err(|err| path_error("read model proof", &entry.path(), err))?;
        let proof = toml::from_str::<ModelCapabilityProof>(&body).map_err(|err| {
            model_store_error(format!(
                "parse model proof `{}` failed: {err}",
                entry.path().display()
            ))
        })?;
        proofs.push(proof);
    }

    Ok(())
}

fn sort_proofs(proofs: &mut [ModelCapabilityProof]) {
    proofs.sort_by(|left, right| {
        left.capability
            .as_str()
            .cmp(right.capability.as_str())
            .then_with(|| left.backend.cmp(&right.backend))
            .then_with(|| {
                left.mlx_runtime_family
                    .map(|family| family.as_str())
                    .cmp(&right.mlx_runtime_family.map(|family| family.as_str()))
            })
            .then_with(|| left.runtime_version.cmp(&right.runtime_version))
            .then_with(|| left.checked_at.cmp(&right.checked_at))
    });
}
