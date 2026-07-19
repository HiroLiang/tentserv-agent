use std::{fs, path::Path};

use serde::{de::DeserializeOwned, Serialize};

use crate::{
    features::runtime_ownership::{
        OwnershipOperationRecord, RouteGenerationClaim, RuntimeGenerationRecord,
        RuntimeOwnershipIssue, RuntimeOwnershipLayout, RuntimeOwnershipStore,
    },
    foundation::{
        error::{KernelError, KernelResult},
        fs::{atomic_write, sync_directory},
    },
};

#[derive(Debug, Clone, Copy, Default)]
pub struct FileRuntimeOwnershipStore;

impl RuntimeOwnershipStore for FileRuntimeOwnershipStore {
    fn ensure_layout(&self, layout: &RuntimeOwnershipLayout) -> KernelResult<()> {
        for dir in [
            &layout.claims,
            &layout.generations,
            &layout.operations,
            &layout.quarantine,
        ] {
            fs::create_dir_all(dir).map_err(ownership_error)?;
        }
        Ok(())
    }

    fn list_claims(
        &self,
        layout: &RuntimeOwnershipLayout,
    ) -> KernelResult<(Vec<RouteGenerationClaim>, Vec<RuntimeOwnershipIssue>)> {
        read_records(&layout.claims, "route-claim")
    }

    fn read_claim(
        &self,
        layout: &RuntimeOwnershipLayout,
        owner_id: &str,
    ) -> KernelResult<Option<RouteGenerationClaim>> {
        read_record(&layout.claims.join(format!("{owner_id}.toml")))
    }

    fn write_claim(
        &self,
        layout: &RuntimeOwnershipLayout,
        claim: &RouteGenerationClaim,
    ) -> KernelResult<()> {
        write_record(
            &layout.claims.join(format!("{}.toml", claim.owner_id)),
            claim,
        )
    }

    fn remove_claim(&self, layout: &RuntimeOwnershipLayout, owner_id: &str) -> KernelResult<()> {
        remove_record(&layout.claims.join(format!("{owner_id}.toml")))
    }

    fn list_generations(
        &self,
        layout: &RuntimeOwnershipLayout,
    ) -> KernelResult<(Vec<RuntimeGenerationRecord>, Vec<RuntimeOwnershipIssue>)> {
        read_records(&layout.generations, "runtime-generation")
    }

    fn read_generation(
        &self,
        layout: &RuntimeOwnershipLayout,
        runtime_key: &str,
    ) -> KernelResult<Option<RuntimeGenerationRecord>> {
        read_record(&layout.generations.join(format!("{runtime_key}.toml")))
    }

    fn write_generation(
        &self,
        layout: &RuntimeOwnershipLayout,
        record: &RuntimeGenerationRecord,
    ) -> KernelResult<()> {
        write_record(
            &layout
                .generations
                .join(format!("{}.toml", record.runtime_key)),
            record,
        )
    }

    fn remove_generation(
        &self,
        layout: &RuntimeOwnershipLayout,
        runtime_key: &str,
    ) -> KernelResult<()> {
        remove_record(&layout.generations.join(format!("{runtime_key}.toml")))
    }

    fn write_operation(
        &self,
        layout: &RuntimeOwnershipLayout,
        record: &OwnershipOperationRecord,
    ) -> KernelResult<()> {
        write_record(
            &layout
                .operations
                .join(format!("{}.toml", record.operation_id)),
            record,
        )
    }

    fn list_operations(
        &self,
        layout: &RuntimeOwnershipLayout,
    ) -> KernelResult<(Vec<OwnershipOperationRecord>, Vec<RuntimeOwnershipIssue>)> {
        read_records(&layout.operations, "ownership-operation")
    }

    fn remove_operation(
        &self,
        layout: &RuntimeOwnershipLayout,
        operation_id: &str,
    ) -> KernelResult<()> {
        remove_record(&layout.operations.join(format!("{operation_id}.toml")))
    }

    fn quarantine_record(
        &self,
        layout: &RuntimeOwnershipLayout,
        source: &Path,
    ) -> KernelResult<String> {
        self.ensure_layout(layout)?;
        let name = format!(
            "{}-{}",
            crate::features::resource_coordination::new_operation_id(),
            source
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("record.toml")
        );
        let destination = layout.quarantine.join(&name);
        fs::rename(source, &destination).map_err(ownership_error)?;
        sync_directory(&layout.quarantine).map_err(ownership_error)?;
        Ok(name)
    }

    fn purge_quarantine_before(
        &self,
        layout: &RuntimeOwnershipLayout,
        invocation_marker: &str,
    ) -> KernelResult<Vec<String>> {
        let cutoff = invocation_marker.parse::<u128>().map_err(ownership_error)?;
        let mut removed = Vec::new();
        for path in record_paths(&layout.quarantine)? {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string();
            let modified = fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .and_then(|modified| {
                    modified
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_err(|error| {
                            std::io::Error::new(std::io::ErrorKind::InvalidData, error)
                        })
                })
                .map_err(ownership_error)?
                .as_nanos();
            if modified < cutoff {
                fs::remove_file(&path).map_err(ownership_error)?;
                removed.push(name);
            }
        }
        removed.sort();
        Ok(removed)
    }
}

fn write_record(path: &Path, value: &impl Serialize) -> KernelResult<()> {
    let body = toml::to_string_pretty(value).map_err(ownership_error)?;
    atomic_write(path, body.as_bytes()).map_err(ownership_error)
}

fn ownership_error(error: impl std::fmt::Display) -> KernelError {
    KernelError::RuntimeOwnershipUnavailable(error.to_string())
}

fn read_record<T: DeserializeOwned>(path: &Path) -> KernelResult<Option<T>> {
    let body = match fs::read_to_string(path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(ownership_error(error)),
    };
    toml::from_str(&body).map(Some).map_err(ownership_error)
}

fn read_records<T: DeserializeOwned>(
    dir: &Path,
    kind: &str,
) -> KernelResult<(Vec<T>, Vec<RuntimeOwnershipIssue>)> {
    let mut records = Vec::new();
    let mut issues = Vec::new();
    for path in record_paths(dir)? {
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("unknown")
            .to_string();
        match fs::read_to_string(&path) {
            Ok(body) => match toml::from_str::<T>(&body) {
                Ok(record) => records.push(record),
                Err(error) => issues.push(RuntimeOwnershipIssue {
                    kind: kind.to_string(),
                    record: path.display().to_string(),
                    description: format!("malformed ownership record `{name}`: {error}"),
                    recoverable: false,
                }),
            },
            Err(error) => issues.push(RuntimeOwnershipIssue {
                kind: kind.to_string(),
                record: path.display().to_string(),
                description: format!("unreadable ownership record `{name}`: {error}"),
                recoverable: false,
            }),
        }
    }
    Ok((records, issues))
}

fn record_paths(dir: &Path) -> KernelResult<Vec<std::path::PathBuf>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(ownership_error(error)),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry.map_err(ownership_error)?.path();
        if path.is_file() {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn remove_record(path: &Path) -> KernelResult<()> {
    match fs::remove_file(path) {
        Ok(()) => {
            if let Some(parent) = path.parent() {
                sync_directory(parent).map_err(ownership_error)?;
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ownership_error(error)),
    }
}
