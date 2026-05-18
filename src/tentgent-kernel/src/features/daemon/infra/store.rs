use std::{fs, path::Path};

use crate::features::daemon::domain::{DaemonProcessMetadata, DaemonStoreLayout};
use crate::features::daemon::ports::{DaemonPidFile, DaemonStateStore, DaemonStoreSnapshot};
use crate::foundation::error::KernelResult;

use super::error::{daemon_store_error, path_error};

/// Filesystem-backed daemon process metadata store.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileDaemonStateStore;

impl DaemonStateStore for FileDaemonStateStore {
    fn inspect_daemon_store(
        &self,
        layout: &DaemonStoreLayout,
    ) -> KernelResult<DaemonStoreSnapshot> {
        let process_path = layout.process_metadata_path();
        let pid_path = layout.pid_path();
        let process = if process_path.exists() {
            Some(read_process_metadata(&process_path)?)
        } else {
            None
        };
        let pid_file = if pid_path.exists() {
            Some(read_pid_file(&pid_path)?)
        } else {
            None
        };

        Ok(DaemonStoreSnapshot {
            home_dir_exists: layout.home_dir.exists(),
            runtime_dir_exists: layout.runtime_dir.exists(),
            log_dir_exists: layout.log_dir.exists(),
            process_path_exists: process_path.exists(),
            pid_path_exists: pid_path.exists(),
            process,
            pid_file,
        })
    }

    fn record_process_start(
        &self,
        layout: &DaemonStoreLayout,
        metadata: &DaemonProcessMetadata,
    ) -> KernelResult<()> {
        write_process_metadata(&layout.process_metadata_path(), metadata)?;
        write_pid_file(&layout.pid_path(), metadata.pid)
    }

    fn clear_process_if_matches(
        &self,
        layout: &DaemonStoreLayout,
        expected_pid: Option<u32>,
    ) -> KernelResult<()> {
        let process_path = layout.process_metadata_path();
        let pid_path = layout.pid_path();

        if !process_path.exists() {
            let _ = fs::remove_file(pid_path);
            return Ok(());
        }

        if let Some(expected_pid) = expected_pid {
            let current = read_process_metadata(&process_path)?;
            if current.pid != expected_pid {
                return Ok(());
            }
        }

        let _ = fs::remove_file(&process_path);
        let _ = fs::remove_file(&pid_path);
        Ok(())
    }
}

fn write_process_metadata(path: &Path, metadata: &DaemonProcessMetadata) -> KernelResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            path_error(
                "create daemon process metadata parent directory",
                parent,
                err,
            )
        })?;
    }
    let body = toml::to_string_pretty(metadata).map_err(|err| {
        daemon_store_error(format!("serialize daemon process metadata failed: {err}"))
    })?;
    fs::write(path, body).map_err(|err| path_error("write daemon process metadata", path, err))
}

fn read_process_metadata(path: &Path) -> KernelResult<DaemonProcessMetadata> {
    let body = fs::read_to_string(path)
        .map_err(|err| path_error("read daemon process metadata", path, err))?;
    toml::from_str(&body).map_err(|err| {
        daemon_store_error(format!(
            "parse daemon process metadata `{}` failed: {err}",
            path.display()
        ))
    })
}

fn write_pid_file(path: &Path, pid: u32) -> KernelResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| path_error("create daemon pid parent directory", parent, err))?;
    }
    fs::write(path, format!("{pid}\n"))
        .map_err(|err| path_error("write daemon pid file", path, err))
}

fn read_pid_file(path: &Path) -> KernelResult<DaemonPidFile> {
    let body =
        fs::read_to_string(path).map_err(|err| path_error("read daemon pid file", path, err))?;
    Ok(match body.trim().parse::<u32>() {
        Ok(pid) => DaemonPidFile::Valid(pid),
        Err(err) => DaemonPidFile::Invalid {
            message: err.to_string(),
        },
    })
}
