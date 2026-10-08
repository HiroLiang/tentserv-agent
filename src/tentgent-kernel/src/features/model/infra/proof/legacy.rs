//! Raw legacy helpers; callers must already hold a validated proof transaction.

use std::{fs, io::Read, path::Path};

use crate::features::model::{
    domain::{
        ModelCapability, ModelCapabilityProof, ModelCapabilityProofKey, ModelRef, ModelStoreLayout,
        MODEL_CAPABILITY_CANONICAL_ORDER,
    },
    usecases::sanitize_proof_error,
};
use crate::foundation::{error::KernelResult, fs::atomic_write};

use super::super::error::{model_store_error, path_error};

pub(super) const MAX_PROOF_BYTES: u64 = 64 * 1024;

pub(super) fn list(
    store: &ModelStoreLayout,
    model_ref: &ModelRef,
    selected: Option<ModelCapability>,
    include_latest: bool,
) -> KernelResult<Vec<ModelCapabilityProof>> {
    let mut proofs = Vec::new();
    for capability in MODEL_CAPABILITY_CANONICAL_ORDER {
        if selected.is_some_and(|wanted| wanted != capability) {
            continue;
        }
        let directory = store.support_proofs_capability_dir(model_ref, capability);
        match fs::read_dir(&directory) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry
                        .map_err(|err| path_error("read model proof entry", &directory, err))?;
                    let path = entry.path();
                    if entry
                        .file_type()
                        .map_err(|err| path_error("read model proof type", &path, err))?
                        .is_file()
                        && path.extension().and_then(|value| value.to_str()) == Some("toml")
                    {
                        proofs.push(read(&path, model_ref, capability)?);
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(path_error("read model support proofs", &directory, err)),
        }
        if include_latest {
            let path = store.capability_proof_path(model_ref, capability);
            match fs::symlink_metadata(&path) {
                Ok(_) => {
                    let latest = read(&path, model_ref, capability)?;
                    let key = ModelCapabilityProofKey::from_proof(&latest);
                    if !proofs
                        .iter()
                        .any(|proof| ModelCapabilityProofKey::from_proof(proof) == key)
                    {
                        proofs.push(latest);
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(path_error("read latest model proof", &path, err)),
            }
        }
    }
    sort(&mut proofs);
    Ok(proofs)
}

pub(super) fn save(
    store: &ModelStoreLayout,
    proof: &ModelCapabilityProof,
    mirror: bool,
) -> KernelResult<()> {
    let mut safe = proof.clone();
    safe.error = safe.error.map(sanitize_proof_error);
    let body = toml::to_string_pretty(&safe)
        .map_err(|_| model_store_error("serialize model proof failed"))?;
    if body.len() as u64 > MAX_PROOF_BYTES {
        return Err(model_store_error(
            "model proof exceeds the 64 KiB record limit",
        ));
    }
    let key = ModelCapabilityProofKey::from_proof(&safe);
    let path = store.support_proof_path(&key);
    atomic_write(&path, body.as_bytes()).map_err(|err| {
        path_error(
            "atomically replace model support proof (replacement may already be visible)",
            &path,
            err,
        )
    })?;
    if mirror {
        let latest = store.capability_proof_path(&safe.model_ref, safe.capability);
        atomic_write(&latest, body.as_bytes()).map_err(|err| path_error(
            "update legacy proof mirror (primary support proof is already committed; reread and retry)",
            &latest, err,
        ))?;
    }
    Ok(())
}

pub(super) fn clear(
    store: &ModelStoreLayout,
    model_ref: &ModelRef,
    capability: ModelCapability,
) -> KernelResult<()> {
    let latest = store.capability_proof_path(model_ref, capability);
    match fs::remove_file(&latest) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(path_error(
                "remove latest proof; clear may be partial, retry safely",
                &latest,
                err,
            ))
        }
    }
    let directory = store.support_proofs_capability_dir(model_ref, capability);
    match fs::remove_dir_all(&directory) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(path_error(
            "remove support proofs; clear may be partial, retry safely",
            &directory,
            err,
        )),
    }
}

pub(super) fn read_bounded(path: &Path) -> KernelResult<String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|err| path_error("inspect model proof", path, err))?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_PROOF_BYTES {
        return Err(model_store_error(format!(
            "invalid model proof file or record exceeds 64 KiB: {}",
            path.display()
        )));
    }
    let file = fs::File::open(path).map_err(|err| path_error("open model proof", path, err))?;
    let mut body = String::new();
    file.take(MAX_PROOF_BYTES + 1)
        .read_to_string(&mut body)
        .map_err(|err| path_error("read model proof", path, err))?;
    if body.len() as u64 > MAX_PROOF_BYTES {
        return Err(model_store_error(
            "model proof exceeds the 64 KiB record limit",
        ));
    }
    Ok(body)
}

fn read(
    path: &Path,
    model_ref: &ModelRef,
    capability: ModelCapability,
) -> KernelResult<ModelCapabilityProof> {
    let body = read_bounded(path)?;
    let mut proof: ModelCapabilityProof = toml::from_str(&body)
        .map_err(|_| model_store_error(format!("invalid model proof TOML: {}", path.display())))?;
    if proof.model_ref != *model_ref || proof.capability != capability {
        return Err(model_store_error(
            "model proof body does not match its model/capability directory",
        ));
    }
    proof.error = proof.error.map(sanitize_proof_error);
    Ok(proof)
}

fn sort(proofs: &mut [ModelCapabilityProof]) {
    // Preserve the legacy resolver's precedence, including stable same-key mirror preference.
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
