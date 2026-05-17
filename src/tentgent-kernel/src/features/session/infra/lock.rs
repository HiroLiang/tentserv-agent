use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime},
};

use crate::features::session::domain::{SessionRef, SessionStoreConfig};
use crate::features::session::ports::{SessionLock, SessionLockGuard, SessionLockManager};
use crate::foundation::error::KernelResult;

use super::error::{path_error, session_store_error};

const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_STALE_LOCK_AFTER: Duration = Duration::from_secs(120);
const LOCK_RETRY_SLEEP: Duration = Duration::from_millis(25);

/// Filesystem lock manager compatible with the legacy session store.
#[derive(Debug, Clone, Copy)]
pub struct FileSessionLockManager {
    lock_timeout: Duration,
    stale_lock_after: Duration,
}

impl Default for FileSessionLockManager {
    fn default() -> Self {
        Self {
            lock_timeout: DEFAULT_LOCK_TIMEOUT,
            stale_lock_after: DEFAULT_STALE_LOCK_AFTER,
        }
    }
}

impl FileSessionLockManager {
    pub fn new(lock_timeout: Duration, stale_lock_after: Duration) -> Self {
        Self {
            lock_timeout,
            stale_lock_after,
        }
    }
}

impl SessionLockManager for FileSessionLockManager {
    fn acquire_create_lock(&self, store: &SessionStoreConfig) -> KernelResult<SessionLock> {
        let layout = store.file_layout().ok_or_else(|| {
            session_store_error("file session lock manager requires a file session store")
        })?;
        acquire_lock(
            &layout.create_lock_path(),
            "sessions",
            self.lock_timeout,
            self.stale_lock_after,
        )
    }

    fn acquire_session_lock(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
    ) -> KernelResult<SessionLock> {
        let layout = store.file_layout().ok_or_else(|| {
            session_store_error("file session lock manager requires a file session store")
        })?;
        acquire_lock(
            &layout.session_lock_path(session_ref),
            session_ref.short_ref(),
            self.lock_timeout,
            self.stale_lock_after,
        )
    }
}

#[derive(Debug)]
struct FileSessionLockGuard {
    path: PathBuf,
}

impl SessionLockGuard for FileSessionLockGuard {}

impl Drop for FileSessionLockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn acquire_lock(
    path: &Path,
    label: &str,
    timeout: Duration,
    stale_after: Duration,
) -> KernelResult<SessionLock> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| path_error("create session lock parent directory", parent, err))?;
    }

    let started = Instant::now();
    loop {
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(_) => {
                return Ok(Box::new(FileSessionLockGuard {
                    path: path.to_path_buf(),
                }));
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                remove_stale_lock(path, stale_after)?;
                if started.elapsed() >= timeout {
                    return Err(session_store_error(format!(
                        "timed out waiting for session lock `{label}`"
                    )));
                }
                thread::sleep(LOCK_RETRY_SLEEP);
            }
            Err(err) => return Err(path_error("create session lock", path, err)),
        }
    }
}

fn remove_stale_lock(path: &Path, stale_after: Duration) -> KernelResult<()> {
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(());
    };
    let Ok(modified) = metadata.modified() else {
        return Ok(());
    };
    let Ok(age) = SystemTime::now().duration_since(modified) else {
        return Ok(());
    };

    if age >= stale_after {
        fs::remove_file(path).map_err(|err| path_error("remove stale session lock", path, err))?;
    }
    Ok(())
}
