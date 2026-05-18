use std::{
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use crate::features::daemon::ports::{DaemonProcessController, DaemonProcessProbe};
use crate::foundation::error::KernelResult;

use super::error::daemon_runtime_error;

/// Operating-system process liveness probe for daemon process ids.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdDaemonProcessProbe;

impl DaemonProcessProbe for StdDaemonProcessProbe {
    fn is_process_running(&self, pid: u32) -> KernelResult<bool> {
        #[cfg(unix)]
        {
            let output = Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output()
                .map_err(|err| {
                    daemon_runtime_error(format!("probe daemon process {pid} failed: {err}"))
                })?;
            if output.status.success() {
                return Ok(true);
            }

            let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
            if stderr.contains("operation not permitted") || stderr.contains("not permitted") {
                return Ok(true);
            }

            Ok(false)
        }

        #[cfg(not(unix))]
        {
            let _ = pid;
            Ok(false)
        }
    }
}

/// Sends TERM and waits briefly for a daemon process to exit.
#[derive(Debug, Clone, Copy)]
pub struct StdDaemonProcessController<P = StdDaemonProcessProbe> {
    process_probe: P,
}

impl Default for StdDaemonProcessController<StdDaemonProcessProbe> {
    fn default() -> Self {
        Self {
            process_probe: StdDaemonProcessProbe,
        }
    }
}

impl<P> StdDaemonProcessController<P> {
    pub fn new(process_probe: P) -> Self {
        Self { process_probe }
    }
}

impl<P> DaemonProcessController for StdDaemonProcessController<P>
where
    P: DaemonProcessProbe,
{
    fn terminate_process(&self, pid: u32) -> KernelResult<()> {
        #[cfg(unix)]
        {
            let status = Command::new("kill")
                .arg(pid.to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(|err| {
                    daemon_runtime_error(format!("terminate daemon process {pid} failed: {err}"))
                })?;
            if !status.success() {
                return Err(daemon_runtime_error(format!(
                    "failed to send TERM to daemon pid {pid}"
                )));
            }

            for _ in 0..30 {
                if !self.process_probe.is_process_running(pid)? {
                    return Ok(());
                }
                thread::sleep(Duration::from_millis(100));
            }

            Err(daemon_runtime_error(format!(
                "daemon pid {pid} did not exit after TERM"
            )))
        }

        #[cfg(not(unix))]
        {
            let _ = pid;
            Err(daemon_runtime_error(
                "daemon process termination is unsupported on this platform",
            ))
        }
    }
}
