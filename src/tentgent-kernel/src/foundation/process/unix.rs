use nix::{
    errno::Errno,
    sys::signal::{kill, Signal},
    unistd::Pid,
};

/// A persisted unsigned number must never become Unix process-group syntax.
pub(crate) fn unix_pid(pid: u32) -> Option<Pid> {
    i32::try_from(pid)
        .ok()
        .filter(|pid| *pid > 0)
        .map(Pid::from_raw)
}

pub(crate) fn is_unix_process_running(pid: u32) -> Result<bool, Errno> {
    probe_with(pid, |pid| kill(pid, None))
}

fn probe_with(pid: u32, probe: impl FnOnce(Pid) -> Result<(), Errno>) -> Result<bool, Errno> {
    let Some(pid) = unix_pid(pid) else {
        return Ok(false);
    };
    match probe(pid) {
        Ok(()) | Err(Errno::EPERM) => Ok(true),
        Err(Errno::ESRCH) => Ok(false),
        Err(error) => Err(error),
    }
}

pub(crate) fn send_unix_process_signal(pid: u32, signal: Signal) -> Result<(), Errno> {
    signal_with(pid, signal, |pid, signal| kill(pid, signal))
}

fn signal_with(
    pid: u32,
    signal: Signal,
    send: impl FnOnce(Pid, Signal) -> Result<(), Errno>,
) -> Result<(), Errno> {
    let pid = unix_pid(pid).ok_or(Errno::EINVAL)?;
    send(pid, signal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn individual_pid_bounds_are_checked_without_os_calls() {
        for pid in [0, i32::MAX as u32 + 1, u32::MAX] {
            assert!(unix_pid(pid).is_none());
            assert_eq!(
                probe_with(pid, |_| panic!("invalid PID reached probe")),
                Ok(false)
            );
            assert_eq!(
                signal_with(pid, Signal::SIGTERM, |_, _| panic!(
                    "invalid PID reached signal"
                )),
                Err(Errno::EINVAL)
            );
        }
        assert_eq!(unix_pid(1).unwrap().as_raw(), 1);
        assert_eq!(unix_pid(i32::MAX as u32).unwrap().as_raw(), i32::MAX);
    }

    #[test]
    fn valid_probe_keeps_permission_denial_live_and_unknown_errors_closed() {
        assert_eq!(probe_with(123, |_| Ok(())), Ok(true));
        assert_eq!(probe_with(123, |_| Err(Errno::EPERM)), Ok(true));
        assert_eq!(probe_with(123, |_| Err(Errno::ESRCH)), Ok(false));
        assert_eq!(probe_with(123, |_| Err(Errno::EINVAL)), Err(Errno::EINVAL));
        assert_eq!(probe_with(123, |_| Err(Errno::EIO)), Err(Errno::EIO));
    }

    #[test]
    fn valid_signal_preserves_the_positive_pid_and_kernel_error() {
        assert_eq!(
            signal_with(123, Signal::SIGTERM, |pid, signal| {
                assert_eq!(pid.as_raw(), 123);
                assert_eq!(signal, Signal::SIGTERM);
                Err(Errno::EPERM)
            }),
            Err(Errno::EPERM)
        );
    }
}
