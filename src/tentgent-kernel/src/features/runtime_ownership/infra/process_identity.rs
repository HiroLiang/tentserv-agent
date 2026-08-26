use std::process::Command;

#[cfg(unix)]
use std::process::Stdio;

use crate::{
    features::runtime_ownership::OwnershipProcessProbe,
    foundation::error::{KernelError, KernelResult},
};

#[derive(Debug, Clone, Copy, Default)]
pub struct StdOwnershipProcessProbe;

impl OwnershipProcessProbe for StdOwnershipProcessProbe {
    fn is_process_running(&self, pid: u32) -> KernelResult<bool> {
        #[cfg(unix)]
        {
            let output = Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .stdout(Stdio::null())
                .output()
                .map_err(ownership_error)?;
            if output.status.success() {
                return Ok(!is_zombie(pid)?);
            }
            let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
            Ok(stderr.contains("not permitted"))
        }
        #[cfg(target_os = "windows")]
        {
            let output = Command::new("tasklist")
                .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
                .output()
                .map_err(ownership_error)?;
            return Ok(output.status.success()
                && String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()));
        }
        #[cfg(not(any(unix, target_os = "windows")))]
        {
            let _ = pid;
            Err(ownership_error("process liveness probe is unsupported"))
        }
    }
}

#[cfg(unix)]
fn is_zombie(pid: u32) -> KernelResult<bool> {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "stat="])
        .output()
        .map_err(ownership_error)?;
    Ok(output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .trim_start()
            .starts_with('Z'))
}

fn ownership_error(error: impl std::fmt::Display) -> KernelError {
    KernelError::RuntimeOwnershipUnavailable(error.to_string())
}
