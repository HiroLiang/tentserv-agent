use std::{
    fs::{self, File, OpenOptions},
    io::ErrorKind,
    path::PathBuf,
    thread,
    time::Instant,
};

use fs2::{lock_contended_error, FileExt};

use crate::{
    features::resource_coordination::{
        bounded_retry_delay, ports::ResourceCoordinationLease, ResourceBusy,
        ResourceCoordinationCode, ResourceCoordinator, ResourceKey, ResourceLockMode,
        ResourceLockRequest, ResourcePermit,
    },
    foundation::{
        error::{KernelError, KernelResult},
        layout::RuntimeLayout,
    },
};

use super::{
    holder_metadata::{read_holders, remove_holder, write_holder},
    layout::coordination_lock_path,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct FileResourceCoordinator;

impl ResourceCoordinator for FileResourceCoordinator {
    fn acquire(
        &self,
        layout: &RuntimeLayout,
        mut request: ResourceLockRequest,
    ) -> KernelResult<Result<ResourcePermit, ResourceBusy>> {
        normalize_locks(&mut request.locks);
        let started = Instant::now();
        let mut attempts = 0;
        let mut last_busy = request
            .locks
            .first()
            .map(|(key, _)| key.clone())
            .unwrap_or_else(ResourceKey::maintenance);

        while attempts < request.max_attempts && started.elapsed() <= request.timeout {
            attempts += 1;
            match try_acquire_set(layout, &request)? {
                Ok(lease) => {
                    let keys = request.locks.iter().map(|(key, _)| key.clone()).collect();
                    return Ok(Ok(ResourcePermit::new(
                        request.operation_id,
                        keys,
                        Box::new(lease),
                    )));
                }
                Err(key) => last_busy = key,
            }
            if attempts < request.max_attempts && started.elapsed() < request.timeout {
                thread::sleep(bounded_retry_delay(
                    &request.operation_id,
                    std::process::id(),
                    attempts,
                    25,
                    75,
                ));
            }
        }

        let holders = read_holders(layout, &last_busy)?;
        let waited_millis = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        Ok(Err(ResourceBusy {
            code: ResourceCoordinationCode::ResourceBusy,
            operation_id: request.operation_id,
            key: last_busy.clone(),
            attempts,
            waited_millis,
            holders,
            retry_after_millis: 250,
            description: format!(
                "resource transition `{}` is busy; retry after the current holder finishes",
                last_busy.label()
            ),
        }))
    }
}

fn normalize_locks(locks: &mut Vec<(ResourceKey, ResourceLockMode)>) {
    locks.sort_by(|left, right| left.0.cmp(&right.0));
    let mut normalized: Vec<(ResourceKey, ResourceLockMode)> = Vec::new();
    for (key, mode) in locks.drain(..) {
        if let Some((last_key, last_mode)) = normalized.last_mut() {
            if *last_key == key {
                if mode == ResourceLockMode::Exclusive {
                    *last_mode = ResourceLockMode::Exclusive;
                }
                continue;
            }
        }
        normalized.push((key, mode));
    }
    *locks = normalized;
}

fn try_acquire_set(
    layout: &RuntimeLayout,
    request: &ResourceLockRequest,
) -> KernelResult<Result<FileCoordinationLease, ResourceKey>> {
    let mut held = Vec::new();
    for (key, mode) in &request.locks {
        let path = coordination_lock_path(layout, key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(coordination_error)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(coordination_error)?;
        let result = match mode {
            ResourceLockMode::Shared => FileExt::try_lock_shared(&file),
            ResourceLockMode::Exclusive => FileExt::try_lock_exclusive(&file),
        };
        match result {
            Ok(()) => {
                let mut provisional = HeldLock {
                    file: Some(file),
                    metadata_path: None,
                };
                let metadata_path = write_holder(
                    layout,
                    key,
                    &request.operation_id,
                    &request.operation,
                    *mode,
                )?;
                provisional.metadata_path = Some(metadata_path);
                held.push(provisional);
            }
            Err(error) if is_lock_contended(&error) => {
                drop(held);
                return Ok(Err(key.clone()));
            }
            Err(error) => return Err(coordination_error(error)),
        }
    }
    Ok(Ok(FileCoordinationLease { _held: held }))
}

pub(super) fn is_lock_contended(error: &std::io::Error) -> bool {
    let expected = lock_contended_error();
    error.kind() == ErrorKind::WouldBlock
        || matches!(
            (error.raw_os_error(), expected.raw_os_error()),
            (Some(actual), Some(expected)) if actual == expected
        )
}

struct HeldLock {
    file: Option<File>,
    metadata_path: Option<PathBuf>,
}

struct FileCoordinationLease {
    _held: Vec<HeldLock>,
}

impl ResourceCoordinationLease for FileCoordinationLease {}

impl Drop for HeldLock {
    fn drop(&mut self) {
        if let Some(metadata_path) = self.metadata_path.take() {
            remove_holder(&metadata_path);
        }
        if let Some(file) = self.file.take() {
            let _ = FileExt::unlock(&file);
        }
    }
}

fn coordination_error(error: impl std::fmt::Display) -> KernelError {
    KernelError::ResourceCoordinationUnavailable(error.to_string())
}
