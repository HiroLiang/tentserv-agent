use std::process::{Command, Stdio};

use crate::features::train::ports::TrainProcessProbe;
use crate::foundation::error::KernelResult;

use super::error::train_store_error;

/// Operating-system process liveness probe.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdTrainProcessProbe;

impl TrainProcessProbe for StdTrainProcessProbe {
    fn is_process_running(&self, pid: u32) -> KernelResult<bool> {
        #[cfg(unix)]
        {
            let status = Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(|err| train_store_error(format!("probe process {pid} failed: {err}")))?;
            Ok(status.success())
        }

        #[cfg(not(unix))]
        {
            let _ = pid;
            Ok(false)
        }
    }
}
