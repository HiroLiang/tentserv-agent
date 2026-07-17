use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    features::resource_coordination::{ResourceHolder, ResourceKey, ResourceLockMode},
    foundation::{
        error::{KernelError, KernelResult},
        fs::atomic_write,
        layout::RuntimeLayout,
    },
};

use super::layout::holder_dir;

#[derive(Debug, Serialize, Deserialize)]
struct HolderRecord {
    schema_version: u32,
    key: ResourceKey,
    holder: ResourceHolder,
}

pub(super) fn write_holder(
    layout: &RuntimeLayout,
    key: &ResourceKey,
    operation_id: &str,
    operation: &str,
    mode: ResourceLockMode,
) -> KernelResult<PathBuf> {
    let dir = holder_dir(layout, key);
    fs::create_dir_all(&dir).map_err(coordination_error)?;
    let path = dir.join(format!("{operation_id}.toml"));
    let record = HolderRecord {
        schema_version: 1,
        key: key.clone(),
        holder: ResourceHolder {
            operation_id: operation_id.to_string(),
            operation: operation.to_string(),
            pid: std::process::id(),
            mode,
            observed_at: now_text(),
        },
    };
    let body = toml::to_string_pretty(&record).map_err(coordination_error)?;
    atomic_write(&path, body.as_bytes()).map_err(coordination_error)?;
    Ok(path)
}

pub(super) fn read_holders(
    layout: &RuntimeLayout,
    key: &ResourceKey,
) -> KernelResult<Vec<ResourceHolder>> {
    let dir = holder_dir(layout, key);
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(coordination_error(err)),
    };
    let mut holders = Vec::new();
    for entry in entries {
        let entry = entry.map_err(coordination_error)?;
        if entry.path().extension().and_then(|value| value.to_str()) != Some("toml") {
            continue;
        }
        let body = fs::read_to_string(entry.path()).map_err(coordination_error)?;
        if let Ok(record) = toml::from_str::<HolderRecord>(&body) {
            holders.push(record.holder);
        }
    }
    holders.sort_by(|left, right| left.operation_id.cmp(&right.operation_id));
    Ok(holders)
}

pub(super) fn remove_holder(path: &PathBuf) {
    let _ = fs::remove_file(path);
}

fn now_text() -> String {
    use time::{format_description::well_known::Rfc3339, OffsetDateTime};
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}

fn coordination_error(error: impl std::fmt::Display) -> KernelError {
    KernelError::ResourceCoordinationUnavailable(error.to_string())
}
