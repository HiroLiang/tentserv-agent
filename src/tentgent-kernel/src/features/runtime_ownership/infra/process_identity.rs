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
            // A filtered no-match result is localized prose, not CSV. Read the
            // full table so an absent PID is determined without parsing it.
            let output = Command::new("tasklist")
                .args(["/FO", "CSV", "/NH"])
                .output()
                .map_err(ownership_error)?;
            parse_tasklist_output(
                pid,
                output.status.success(),
                &String::from_utf8_lossy(&output.stdout),
                &String::from_utf8_lossy(&output.stderr),
            )
        }
        #[cfg(not(any(unix, target_os = "windows")))]
        {
            let _ = pid;
            Err(ownership_error("process liveness probe is unsupported"))
        }
    }
}

#[cfg(any(windows, test))]
pub(super) fn parse_tasklist_output(
    pid: u32,
    success: bool,
    stdout: &str,
    stderr: &str,
) -> KernelResult<bool> {
    if !success {
        let detail: String = stderr.trim().chars().take(256).collect();
        return Err(ownership_error(format!(
            "tasklist process probe for pid {pid} failed: {}",
            if detail.is_empty() {
                "command exited unsuccessfully"
            } else {
                &detail
            }
        )));
    }

    let mut found = false;
    for line in stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line.starts_with("INFO:") {
            continue;
        }
        let mut fields = line.chars().peekable();
        let parsed_pid = quoted_csv_field(&mut fields)
            .and_then(|_| quoted_csv_field(&mut fields))
            .and_then(|field| field.parse::<u32>().ok())
            .ok_or_else(|| ownership_error("tasklist returned an invalid process CSV row"))?;
        found |= parsed_pid == pid;
    }
    Ok(found)
}

#[cfg(any(windows, test))]
fn quoted_csv_field(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
    if chars.next()? != '"' {
        return None;
    }
    let mut field = String::new();
    while let Some(character) = chars.next() {
        if character != '"' {
            field.push(character);
        } else if chars.peek() == Some(&'"') {
            chars.next();
            field.push('"');
        } else {
            return match chars.next() {
                Some(',') | None => Some(field),
                _ => None,
            };
        }
    }
    None
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
