use crate::features::daemon::domain::{
    DaemonInspection, DaemonStoreLayout, DaemonWarning, DAEMON_WARNING_PID_PATH_STALE,
    DAEMON_WARNING_PROCESS_METADATA_STALE, DAEMON_WARNING_PROCESS_PATH_MISSING,
    DAEMON_WARNING_RUNTIME_DIR_MISSING, DAEMON_WARNING_RUNTIME_HOME_MISSING,
};
use crate::features::daemon::ports::{DaemonPidFile, DaemonProcessProbe, DaemonStateStore};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;

use super::port::DaemonInspectionMode;

pub(super) fn daemon_store_layout(layout: &RuntimeLayout) -> DaemonStoreLayout {
    DaemonStoreLayout::from_home_runtime_log_dirs(
        layout.home_dir.clone(),
        layout.runtime_dir.clone(),
        layout.logs_dir.clone(),
    )
}

pub(super) fn inspect_daemon(
    store: &DaemonStoreLayout,
    state_store: &dyn DaemonStateStore,
    process_probe: &dyn DaemonProcessProbe,
    mode: DaemonInspectionMode,
) -> KernelResult<DaemonInspection> {
    let snapshot = state_store.inspect_daemon_store(store)?;
    let process_path = store.process_metadata_path();
    let pid_path = store.pid_path();
    let stdout_log_path = store.stdout_log_path();
    let stderr_log_path = store.stderr_log_path();

    if !snapshot.home_dir_exists {
        return Ok(DaemonInspection {
            home_dir: store.home_dir.clone(),
            runtime_dir: store.runtime_dir.clone(),
            log_dir: store.log_dir.clone(),
            process_path,
            pid_path,
            stdout_log_path,
            stderr_log_path,
            running: false,
            process: None,
            warnings: vec![DaemonWarning::new(
                DAEMON_WARNING_RUNTIME_HOME_MISSING,
                format!("runtime home is missing: {}", store.home_dir.display()),
                Some(store.home_dir.clone()),
            )],
        });
    }

    if !snapshot.runtime_dir_exists {
        return Ok(DaemonInspection {
            home_dir: store.home_dir.clone(),
            runtime_dir: store.runtime_dir.clone(),
            log_dir: store.log_dir.clone(),
            process_path,
            pid_path,
            stdout_log_path,
            stderr_log_path,
            running: false,
            process: None,
            warnings: vec![DaemonWarning::new(
                DAEMON_WARNING_RUNTIME_DIR_MISSING,
                format!(
                    "daemon runtime directory is missing: {}",
                    store.runtime_dir.display()
                ),
                Some(store.runtime_dir.clone()),
            )],
        });
    }

    let Some(process) = snapshot.process else {
        let warning = if snapshot.pid_path_exists {
            DaemonWarning::new(
                DAEMON_WARNING_PID_PATH_STALE,
                format!(
                    "daemon pid file exists without process metadata: {}",
                    pid_path.display()
                ),
                Some(pid_path.clone()),
            )
        } else {
            DaemonWarning::new(
                DAEMON_WARNING_PROCESS_PATH_MISSING,
                format!(
                    "daemon process metadata is missing: {}",
                    process_path.display()
                ),
                Some(process_path.clone()),
            )
        };
        return Ok(DaemonInspection {
            home_dir: store.home_dir.clone(),
            runtime_dir: store.runtime_dir.clone(),
            log_dir: store.log_dir.clone(),
            process_path,
            pid_path,
            stdout_log_path,
            stderr_log_path,
            running: false,
            process: None,
            warnings: vec![warning],
        });
    };

    let running = process_probe.is_process_running(process.pid)?;
    if running {
        let warnings = pid_file_warnings(&pid_path, &snapshot.pid_file, process.pid);
        return Ok(DaemonInspection {
            home_dir: store.home_dir.clone(),
            runtime_dir: store.runtime_dir.clone(),
            log_dir: store.log_dir.clone(),
            process_path,
            pid_path,
            stdout_log_path,
            stderr_log_path,
            running: true,
            process: Some(process),
            warnings,
        });
    }

    let warning = DaemonWarning::new(
        DAEMON_WARNING_PROCESS_METADATA_STALE,
        format!(
            "daemon metadata references pid {}, but that process is not running",
            process.pid
        ),
        Some(process_path.clone()),
    );
    if mode == DaemonInspectionMode::CleanupStale {
        state_store.clear_process_if_matches(store, Some(process.pid))?;
        return Ok(DaemonInspection {
            home_dir: store.home_dir.clone(),
            runtime_dir: store.runtime_dir.clone(),
            log_dir: store.log_dir.clone(),
            process_path,
            pid_path,
            stdout_log_path,
            stderr_log_path,
            running: false,
            process: None,
            warnings: vec![warning],
        });
    }

    Ok(DaemonInspection {
        home_dir: store.home_dir.clone(),
        runtime_dir: store.runtime_dir.clone(),
        log_dir: store.log_dir.clone(),
        process_path,
        pid_path,
        stdout_log_path,
        stderr_log_path,
        running: false,
        process: Some(process),
        warnings: vec![warning],
    })
}

fn pid_file_warnings(
    pid_path: &std::path::Path,
    pid_file: &Option<DaemonPidFile>,
    process_pid: u32,
) -> Vec<DaemonWarning> {
    match pid_file {
        Some(DaemonPidFile::Valid(pid)) if *pid != process_pid => vec![DaemonWarning::new(
            DAEMON_WARNING_PID_PATH_STALE,
            format!("daemon pid file records pid {pid}, but metadata records pid {process_pid}"),
            Some(pid_path.to_path_buf()),
        )],
        Some(DaemonPidFile::Invalid { .. }) => vec![DaemonWarning::new(
            DAEMON_WARNING_PID_PATH_STALE,
            format!("daemon pid file is not a valid pid: {}", pid_path.display()),
            Some(pid_path.to_path_buf()),
        )],
        _ => Vec::new(),
    }
}
