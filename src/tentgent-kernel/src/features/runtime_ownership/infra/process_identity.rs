use std::process::Command;

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
            if !crate::foundation::process::is_unix_process_running(pid).map_err(ownership_error)? {
                return Ok(false);
            }
            unix_liveness_from_state(is_zombie(pid), || {
                crate::foundation::process::is_unix_process_running(pid).map_err(ownership_error)
            })
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
    let pid = crate::foundation::process::unix_pid(pid)
        .ok_or_else(|| ownership_error("process state requires a positive Unix PID"))?;
    let output = Command::new("ps")
        .args(["-p", &pid.as_raw().to_string(), "-o", "stat="])
        .output()
        .map_err(ownership_error)?;
    parse_unix_process_state(
        output.status.success(),
        &String::from_utf8_lossy(&output.stdout),
        &String::from_utf8_lossy(&output.stderr),
    )
}

#[cfg(any(unix, test))]
pub(super) fn unix_liveness_from_state(
    zombie: KernelResult<bool>,
    recheck: impl FnOnce() -> KernelResult<bool>,
) -> KernelResult<bool> {
    match zombie {
        Ok(zombie) => Ok(!zombie),
        // The process may have exited between signal 0 and ps. Only a second
        // typed probe proving absence permits stale-state cleanup.
        Err(error) => {
            if recheck()? {
                Err(error)
            } else {
                Ok(false)
            }
        }
    }
}

#[cfg(any(unix, test))]
pub(super) fn parse_unix_process_state(
    success: bool,
    stdout: &str,
    stderr: &str,
) -> KernelResult<bool> {
    if !success {
        let detail: String = stderr.trim().chars().take(256).collect();
        return Err(ownership_error(format!(
            "Unix process state probe failed: {}",
            if detail.is_empty() {
                "command exited unsuccessfully"
            } else {
                &detail
            }
        )));
    }
    let mut states = stdout.split_whitespace();
    let state = states
        .next()
        .filter(|state| state.starts_with(|character: char| character.is_ascii_alphabetic()))
        .ok_or_else(|| ownership_error("Unix process state probe returned no valid state"))?;
    if states.next().is_some() {
        return Err(ownership_error(
            "Unix process state probe returned multiple states for one PID",
        ));
    }
    Ok(state.starts_with('Z'))
}

fn ownership_error(error: impl std::fmt::Display) -> KernelError {
    KernelError::RuntimeOwnershipUnavailable(error.to_string())
}
