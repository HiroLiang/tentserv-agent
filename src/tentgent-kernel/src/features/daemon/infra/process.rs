#[cfg(unix)]
use std::{thread, time::Duration};

#[cfg(unix)]
use crate::foundation::process::{is_unix_process_running, send_unix_process_signal, unix_pid};

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
            is_unix_process_running(pid).map_err(|err| {
                daemon_runtime_error(format!("probe daemon process {pid} failed: {err}"))
            })
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
    _process_probe: P,
}

impl Default for StdDaemonProcessController<StdDaemonProcessProbe> {
    fn default() -> Self {
        Self {
            _process_probe: StdDaemonProcessProbe,
        }
    }
}

impl<P> StdDaemonProcessController<P> {
    pub fn new(process_probe: P) -> Self {
        Self {
            _process_probe: process_probe,
        }
    }
}

impl<P> DaemonProcessController for StdDaemonProcessController<P>
where
    P: DaemonProcessProbe,
{
    fn terminate_process(&self, pid: u32) -> KernelResult<()> {
        #[cfg(unix)]
        {
            if unix_pid(pid).is_none() {
                return Err(daemon_runtime_error(format!(
                    "invalid individual daemon pid {pid}"
                )));
            }
            send_unix_process_signal(pid, nix::sys::signal::Signal::SIGTERM).map_err(|err| {
                daemon_runtime_error(format!("terminate daemon process {pid} failed: {err}"))
            })?;

            for _ in 0..30 {
                if !self._process_probe.is_process_running(pid)? {
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    struct ProbeMustNotRun;

    impl DaemonProcessProbe for ProbeMustNotRun {
        fn is_process_running(&self, _: u32) -> KernelResult<bool> {
            panic!("invalid PID reached post-signal probe");
        }
    }

    #[test]
    fn invalid_daemon_pids_are_not_processes_and_cannot_be_terminated() {
        // The controller guard and shared checked signal boundary both reject
        // these values before any OS call. Never invoke an external kill tool.
        for pid in [0, i32::MAX as u32 + 1, u32::MAX] {
            assert!(!StdDaemonProcessProbe.is_process_running(pid).unwrap());
            assert!(StdDaemonProcessController::new(ProbeMustNotRun)
                .terminate_process(pid)
                .unwrap_err()
                .to_string()
                .contains("invalid individual daemon pid"));
        }
    }

    #[test]
    fn daemon_probe_recognizes_current_process() {
        assert!(StdDaemonProcessProbe
            .is_process_running(std::process::id())
            .unwrap());
    }
}
