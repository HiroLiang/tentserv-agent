use std::{
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use crate::features::server::ports::{ServerProcessController, ServerProcessProbe};
use crate::foundation::error::KernelResult;

use super::error::server_runtime_error;

/// Operating-system process liveness probe.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdServerProcessProbe;

impl ServerProcessProbe for StdServerProcessProbe {
    fn is_process_running(&self, pid: u32) -> KernelResult<bool> {
        #[cfg(unix)]
        {
            let output = Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .stdout(Stdio::null())
                .output()
                .map_err(|err| {
                    server_runtime_error(format!("probe process {pid} failed: {err}"))
                })?;
            if output.status.success() {
                if process_is_zombie(pid)? {
                    return Ok(false);
                }
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

/// Sends TERM and waits briefly for a server process to exit.
#[derive(Debug, Clone, Copy)]
pub struct StdServerProcessController<P = StdServerProcessProbe> {
    process_probe: P,
}

impl Default for StdServerProcessController<StdServerProcessProbe> {
    fn default() -> Self {
        Self {
            process_probe: StdServerProcessProbe,
        }
    }
}

impl<P> StdServerProcessController<P> {
    pub fn new(process_probe: P) -> Self {
        Self { process_probe }
    }
}

impl<P> ServerProcessController for StdServerProcessController<P>
where
    P: ServerProcessProbe,
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
                    server_runtime_error(format!("terminate process {pid} failed: {err}"))
                })?;
            if !status.success() {
                return Err(server_runtime_error(format!(
                    "failed to send TERM to pid {pid}"
                )));
            }

            for _ in 0..30 {
                if !self.process_probe.is_process_running(pid)? {
                    return Ok(());
                }
                thread::sleep(Duration::from_millis(100));
            }

            Err(server_runtime_error(format!(
                "pid {pid} did not exit after TERM"
            )))
        }

        #[cfg(not(unix))]
        {
            let _ = pid;
            Err(server_runtime_error(
                "server process termination is unsupported on this platform",
            ))
        }
    }
}

#[cfg(unix)]
fn process_is_zombie(pid: u32) -> KernelResult<bool> {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "stat="])
        .output()
        .map_err(|err| server_runtime_error(format!("inspect process {pid} failed: {err}")))?;
    if !output.status.success() {
        return Ok(false);
    }
    let stat = String::from_utf8_lossy(&output.stdout);
    Ok(stat.trim_start().starts_with('Z'))
}
