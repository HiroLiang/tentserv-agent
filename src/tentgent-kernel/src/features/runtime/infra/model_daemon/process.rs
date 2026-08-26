use std::{process::Child, sync::Arc, thread};

#[cfg(unix)]
use std::time::Duration;

use crate::foundation::error::{KernelError, KernelResult};

#[cfg(unix)]
const TERMINATE_GRACE: Duration = Duration::from_secs(2);
#[cfg(unix)]
const TERMINATE_POLL: Duration = Duration::from_millis(25);

pub(super) struct PendingRuntimeProcess {
    child: Option<Child>,
    terminator: Arc<dyn RuntimeProcessTerminator>,
}

pub(super) trait RuntimeProcessTerminator: Send + Sync {
    fn terminate_and_wait(&self, child: &mut Child) -> KernelResult<()>;
}

struct StdRuntimeProcessTerminator;

impl PendingRuntimeProcess {
    pub(super) fn new(child: Child) -> Self {
        Self {
            child: Some(child),
            terminator: Arc::new(StdRuntimeProcessTerminator),
        }
    }

    #[cfg(test)]
    pub(super) fn new_with_terminator(
        child: Child,
        terminator: Arc<dyn RuntimeProcessTerminator>,
    ) -> Self {
        Self {
            child: Some(child),
            terminator,
        }
    }

    pub(super) fn pid(&self) -> u32 {
        self.child.as_ref().map(Child::id).unwrap_or_default()
    }

    pub(super) fn disarm(mut self) {
        if let Some(mut child) = self.child.take() {
            thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }

    pub(super) fn terminate_and_wait(&mut self) -> KernelResult<()> {
        let Some(child) = self.child.as_mut() else {
            return Ok(());
        };
        self.terminator.terminate_and_wait(child)?;
        self.child.take();
        Ok(())
    }
}

impl RuntimeProcessTerminator for StdRuntimeProcessTerminator {
    fn terminate_and_wait(&self, child: &mut Child) -> KernelResult<()> {
        terminate_process(child)
    }
}

impl Drop for PendingRuntimeProcess {
    fn drop(&mut self) {
        let _ = self.terminate_and_wait();
    }
}

#[cfg(unix)]
fn terminate_process(child: &mut Child) -> KernelResult<()> {
    use nix::{
        errno::Errno,
        sys::signal::{killpg, Signal},
        unistd::Pid,
    };

    let pid = child.id();
    if child.try_wait().map_err(runtime_error)?.is_some() {
        return Ok(());
    }
    let process_group = Pid::from_raw(pid as i32);
    if let Err(error) = killpg(process_group, Signal::SIGTERM) {
        if error != Errno::ESRCH {
            return Err(runtime_error(format!(
                "terminate runtime process group {pid} failed: {error}"
            )));
        }
    }
    let started = std::time::Instant::now();
    while started.elapsed() < TERMINATE_GRACE {
        if child.try_wait().map_err(runtime_error)?.is_some() {
            return Ok(());
        }
        thread::sleep(TERMINATE_POLL);
    }
    if let Err(error) = killpg(process_group, Signal::SIGKILL) {
        if error != Errno::ESRCH {
            return Err(runtime_error(format!(
                "kill runtime process group {pid} failed: {error}"
            )));
        }
    }
    child.wait().map_err(runtime_error)?;
    Ok(())
}

#[cfg(windows)]
fn terminate_process(child: &mut Child) -> KernelResult<()> {
    if child.try_wait().map_err(runtime_error)?.is_some() {
        return Ok(());
    }
    child.kill().map_err(runtime_error)?;
    child.wait().map_err(runtime_error)?;
    Ok(())
}

fn runtime_error(error: impl std::fmt::Display) -> KernelError {
    KernelError::RuntimeStateUnavailable(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};

    #[cfg(unix)]
    use std::{thread, time::Duration};

    use super::PendingRuntimeProcess;

    #[cfg(unix)]
    #[test]
    fn pending_runtime_process_terminates_and_waits_for_process_group() {
        use std::os::unix::process::CommandExt;

        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("sleep 30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        let child = command.spawn().expect("spawn process group");
        let mut pending = PendingRuntimeProcess::new(child);

        pending
            .terminate_and_wait()
            .expect("terminate pending process group");
        assert_eq!(pending.pid(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn disarmed_runtime_process_is_reaped_after_natural_exit() {
        let child = Command::new("sh")
            .args(["-c", "exit 0"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn short process");
        let pid = child.id();

        PendingRuntimeProcess::new(child).disarm();

        for _ in 0..100 {
            let status = Command::new("ps")
                .args(["-p", &pid.to_string(), "-o", "stat="])
                .output()
                .expect("probe child");
            if !status.status.success() || status.stdout.is_empty() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("disarmed runtime process {pid} was not reaped");
    }

    #[cfg(windows)]
    #[test]
    fn pending_runtime_process_terminates_and_waits_for_child() {
        let child = Command::new("cmd")
            .args(["/C", "ping", "127.0.0.1", "-n", "30"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn child");
        let mut pending = PendingRuntimeProcess::new(child);

        pending
            .terminate_and_wait()
            .expect("terminate pending child");
        assert_eq!(pending.pid(), 0);
    }
}
