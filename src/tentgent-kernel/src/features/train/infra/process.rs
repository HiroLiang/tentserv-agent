#[cfg(unix)]
use crate::foundation::process::is_unix_process_running;

use crate::features::train::ports::TrainProcessProbe;
use crate::foundation::error::KernelResult;

#[cfg(unix)]
use super::error::train_store_error;

/// Operating-system process liveness probe.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdTrainProcessProbe;

impl TrainProcessProbe for StdTrainProcessProbe {
    fn is_process_running(&self, pid: u32) -> KernelResult<bool> {
        #[cfg(unix)]
        {
            is_unix_process_running(pid)
                .map_err(|err| train_store_error(format!("probe process {pid} failed: {err}")))
        }

        #[cfg(not(unix))]
        {
            let _ = pid;
            Ok(false)
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn train_probe_rejects_invalid_pids_and_recognizes_current_process() {
        for pid in [0, i32::MAX as u32 + 1, u32::MAX] {
            assert!(!StdTrainProcessProbe.is_process_running(pid).unwrap());
        }
        assert!(StdTrainProcessProbe
            .is_process_running(std::process::id())
            .unwrap());
    }
}
