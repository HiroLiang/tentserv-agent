#[cfg(unix)]
use std::{process::Command, thread, time::Duration};

#[cfg(unix)]
use crate::foundation::process::{is_unix_process_running, send_unix_process_signal, unix_pid};

use crate::features::server::ports::{ServerProcessController, ServerProcessProbe};
use crate::foundation::error::KernelResult;

use super::error::server_runtime_error;

#[cfg(unix)]
const TERMINATION_POLL_INTERVAL: Duration = Duration::from_millis(100);
#[cfg(unix)]
const TERMINATION_WAIT_ATTEMPTS: usize = 320;

/// Operating-system process liveness probe.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdServerProcessProbe;

impl ServerProcessProbe for StdServerProcessProbe {
    fn is_process_running(&self, pid: u32) -> KernelResult<bool> {
        #[cfg(unix)]
        {
            if !is_unix_process_running(pid)
                .map_err(|err| server_runtime_error(format!("probe process {pid} failed: {err}")))?
            {
                return Ok(false);
            }
            Ok(!process_is_zombie(pid)?)
        }

        #[cfg(windows)]
        {
            use crate::features::runtime_ownership::{
                OwnershipProcessProbe, StdOwnershipProcessProbe,
            };

            StdOwnershipProcessProbe
                .is_process_running(pid)
                .map_err(|err| server_runtime_error(format!("probe process {pid} failed: {err}")))
        }

        #[cfg(not(any(unix, windows)))]
        {
            let _ = pid;
            Ok(false)
        }
    }
}

/// Sends TERM and waits briefly for a server process to exit.
#[derive(Debug, Clone, Copy)]
pub struct StdServerProcessController<P = StdServerProcessProbe> {
    _process_probe: P,
}

impl Default for StdServerProcessController<StdServerProcessProbe> {
    fn default() -> Self {
        Self {
            _process_probe: StdServerProcessProbe,
        }
    }
}

impl<P> StdServerProcessController<P> {
    pub fn new(process_probe: P) -> Self {
        Self {
            _process_probe: process_probe,
        }
    }
}

impl<P> ServerProcessController for StdServerProcessController<P>
where
    P: ServerProcessProbe,
{
    fn terminate_process(&self, pid: u32) -> KernelResult<()> {
        #[cfg(unix)]
        {
            if unix_pid(pid).is_none() {
                return Err(server_runtime_error(format!(
                    "invalid individual server pid {pid}"
                )));
            }
            send_unix_process_signal(pid, nix::sys::signal::Signal::SIGTERM).map_err(|err| {
                server_runtime_error(format!("terminate process {pid} failed: {err}"))
            })?;

            for _ in 0..TERMINATION_WAIT_ATTEMPTS {
                if !self._process_probe.is_process_running(pid)? {
                    return Ok(());
                }
                thread::sleep(TERMINATION_POLL_INTERVAL);
            }

            Err(server_runtime_error(format!(
                "pid {pid} did not exit within the bounded shutdown window after TERM"
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
    let pid = unix_pid(pid)
        .ok_or_else(|| server_runtime_error(format!("invalid individual process pid {pid}")))?;
    let output = Command::new("ps")
        .args(["-p", &pid.as_raw().to_string(), "-o", "stat="])
        .output()
        .map_err(|err| server_runtime_error(format!("inspect process {pid} failed: {err}")))?;
    if !output.status.success() {
        return Ok(false);
    }
    let stat = String::from_utf8_lossy(&output.stdout);
    Ok(stat.trim_start().starts_with('Z'))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    struct ProbeMustNotRun;

    impl ServerProcessProbe for ProbeMustNotRun {
        fn is_process_running(&self, _: u32) -> KernelResult<bool> {
            panic!("invalid PID reached post-signal probe");
        }
    }

    #[test]
    fn invalid_server_pids_are_not_processes_and_cannot_be_terminated() {
        // Both the controller and shared signal boundary reject invalid IDs.
        for pid in [0, i32::MAX as u32 + 1, u32::MAX] {
            assert!(!StdServerProcessProbe.is_process_running(pid).unwrap());
            assert!(process_is_zombie(pid).is_err());
            assert!(StdServerProcessController::new(ProbeMustNotRun)
                .terminate_process(pid)
                .unwrap_err()
                .to_string()
                .contains("invalid individual server pid"));
        }
    }

    #[test]
    fn server_probe_recognizes_current_process() {
        assert!(StdServerProcessProbe
            .is_process_running(std::process::id())
            .unwrap());
    }
}
