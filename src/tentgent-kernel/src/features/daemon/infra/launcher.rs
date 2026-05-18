use std::{
    fs::OpenOptions,
    process::{Command, Stdio},
};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use crate::features::daemon::ports::{DaemonDetachedCommand, DaemonDetachedLauncher};
use crate::foundation::error::KernelResult;

use super::error::{daemon_runtime_error, path_error};

/// Standard detached daemon child launcher.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdDaemonDetachedLauncher;

impl DaemonDetachedLauncher for StdDaemonDetachedLauncher {
    fn launch_detached(&self, command: &DaemonDetachedCommand) -> KernelResult<u32> {
        if let Some(parent) = command.stdout_log_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| path_error("create daemon stdout log directory", parent, err))?;
        }
        if let Some(parent) = command.stderr_log_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| path_error("create daemon stderr log directory", parent, err))?;
        }

        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&command.stdout_log_path)
            .map_err(|err| path_error("open daemon stdout log", &command.stdout_log_path, err))?;
        let stderr = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&command.stderr_log_path)
            .map_err(|err| path_error("open daemon stderr log", &command.stderr_log_path, err))?;

        let mut child = Command::new(&command.executable);
        child
            .args(&command.args)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        #[cfg(unix)]
        {
            child.process_group(0);
        }

        let child = child.spawn().map_err(|err| {
            daemon_runtime_error(format!(
                "launch detached daemon `{}` failed: {err}",
                command.executable.display()
            ))
        })?;
        Ok(child.id())
    }
}
