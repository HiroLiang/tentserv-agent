//! Raw v2 file operations. Only the coordinated facade may call these helpers.

use std::fs;

use super::super::error::{model_store_error, path_error};
use crate::features::model::{
    compatibility::{
        CompatibilityFilter, CompatibilityProofKey, CompatibilityProofV2, MAX_PROOF_V2_BYTES,
    },
    domain::{ModelCapability, ModelRef, ModelStoreLayout, MODEL_CAPABILITY_CANONICAL_ORDER},
};
use crate::foundation::{error::KernelResult, fs::atomic_write};

pub(super) fn get(
    store: &ModelStoreLayout,
    key: &CompatibilityProofKey,
) -> KernelResult<Option<CompatibilityProofV2>> {
    let path = store.compatibility_proof_path(key);
    match fs::symlink_metadata(&path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(path_error("inspect compatibility proof", &path, err)),
        Ok(_) => {}
    }
    let body = super::legacy::read_bounded(&path)?;
    if body.len() > MAX_PROOF_V2_BYTES {
        return Err(model_store_error(
            "compatibility proof exceeds the 16 KiB record limit",
        ));
    }
    let proof: CompatibilityProofV2 = toml::from_str(&body).map_err(|_| {
        model_store_error(format!(
            "invalid compatibility proof schema or TOML: {}",
            path.display()
        ))
    })?;
    proof
        .validate_location(key)
        .map_err(|err| model_store_error(err.to_string()))?;
    Ok(Some(proof))
}

pub(super) fn save(
    store: &ModelStoreLayout,
    proof: &CompatibilityProofV2,
) -> KernelResult<CompatibilityProofKey> {
    let key = proof
        .key()
        .map_err(|err| model_store_error(err.to_string()))?;
    // A new observation may replace valid evidence, not silently repair an
    // unsupported or corrupt record. The facade holds the exclusive permit.
    get(store, &key)?;
    let body = toml::to_string_pretty(proof)
        .map_err(|_| model_store_error("serialize compatibility proof failed"))?;
    if body.len() > MAX_PROOF_V2_BYTES {
        return Err(model_store_error(
            "compatibility proof exceeds the 16 KiB record limit",
        ));
    }
    let path = store.compatibility_proof_path(&key);
    atomic_write(&path, body.as_bytes()).map_err(|err| {
        path_error(
            "atomically replace compatibility proof (replacement may already be visible)",
            &path,
            err,
        )
    })?;
    Ok(key)
}

pub(super) fn list(
    store: &ModelStoreLayout,
    model_ref: &ModelRef,
    filter: &CompatibilityFilter,
) -> KernelResult<Vec<CompatibilityProofV2>> {
    if filter
        .model_ref
        .as_ref()
        .is_some_and(|selected| selected != model_ref)
    {
        return Ok(Vec::new());
    }
    if let Some(key) = &filter.exact_key {
        if key.model_ref() != model_ref {
            return Ok(Vec::new());
        }
        return match get(store, key)? {
            Some(proof)
                if filter
                    .matches(&proof)
                    .map_err(|err| model_store_error(err.to_string()))? =>
            {
                Ok(vec![proof])
            }
            _ => Ok(Vec::new()),
        };
    }
    if filter.capability.is_none() {
        validate_capability_directories(store, model_ref)?;
    }
    let mut keyed = Vec::new();
    for capability in MODEL_CAPABILITY_CANONICAL_ORDER {
        if filter
            .capability
            .is_some_and(|selected| selected != capability)
        {
            continue;
        }
        let directory = store.compatibility_proofs_capability_dir(model_ref, capability);
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(path_error("read compatibility proofs", &directory, err)),
        };
        for entry in entries {
            let entry = entry
                .map_err(|err| path_error("read compatibility proof entry", &directory, err))?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("toml") {
                continue;
            }
            let digest = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| model_store_error("invalid compatibility proof filename"))?;
            let key = CompatibilityProofKey::parse(model_ref.clone(), capability, digest)
                .map_err(|err| model_store_error(err.to_string()))?;
            let proof = get(store, &key)?.ok_or_else(|| {
                model_store_error("compatibility proof disappeared during a locked snapshot")
            })?;
            if filter
                .matches(&proof)
                .map_err(|err| model_store_error(err.to_string()))?
            {
                keyed.push((capability.as_str(), digest.to_owned(), proof));
            }
        }
    }
    keyed.sort_by(|left, right| (&left.0, &left.1).cmp(&(&right.0, &right.1)));
    Ok(keyed.into_iter().map(|(_, _, proof)| proof).collect())
}

pub(super) fn remove(store: &ModelStoreLayout, key: &CompatibilityProofKey) -> KernelResult<bool> {
    if get(store, key)?.is_none() {
        return Ok(false);
    }
    let path = store.compatibility_proof_path(key);
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(path_error("remove exact compatibility proof", &path, err)),
    }
}

fn validate_capability_directories(
    store: &ModelStoreLayout,
    model_ref: &ModelRef,
) -> KernelResult<()> {
    let directory = store.compatibility_proofs_dir(model_ref);
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(path_error(
                "read compatibility proof namespaces",
                &directory,
                err,
            ))
        }
    };
    for entry in entries {
        let entry = entry.map_err(|err| {
            path_error("read compatibility proof namespace entry", &directory, err)
        })?;
        let kind = entry.file_type().map_err(|err| {
            path_error("inspect compatibility proof namespace", &entry.path(), err)
        })?;
        if !kind.is_dir() && !kind.is_symlink() {
            continue;
        }
        let name = entry.file_name();
        if !MODEL_CAPABILITY_CANONICAL_ORDER
            .iter()
            .any(|capability| name.to_str() == Some(capability.as_str()))
        {
            // Do not read records in a namespace for which this binary has no
            // capability lock. Exact/scoped operations leave it untouched.
            return Err(model_store_error(
                "unsupported compatibility proof capability directory; use a compatible binary or query a known capability",
            ));
        }
    }
    Ok(())
}

pub(super) fn clear(
    store: &ModelStoreLayout,
    model_ref: &ModelRef,
    capability: ModelCapability,
) -> KernelResult<()> {
    let path = store.compatibility_proofs_capability_dir(model_ref, capability);
    match fs::remove_dir_all(&path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(path_error(
            "clear compatibility proofs; clear may be partial, retry safely",
            &path,
            err,
        )),
    }
}
