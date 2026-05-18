use std::process::Command;

use crate::features::doctor::domain::{DoctorCheck, DoctorCommandCheck};
use crate::features::doctor::ports::DoctorCommandProbe;
use crate::foundation::error::KernelResult;

/// Checks external command availability for doctor diagnostics.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdDoctorCommandProbe;

impl DoctorCommandProbe for StdDoctorCommandProbe {
    fn check_command(&self, request: DoctorCommandCheck) -> KernelResult<DoctorCheck> {
        Ok(
            match Command::new(&request.command).args(&request.args).output() {
                Ok(output) if output.status.success() => DoctorCheck::pass(
                    request.category,
                    request.name,
                    command_output_detail(&output.stdout, &output.stderr),
                ),
                Ok(output) => DoctorCheck::with_status(
                    request.category,
                    request.name,
                    request.missing_status,
                    format!("command exited with status {}", output.status),
                ),
                Err(err) => DoctorCheck::with_status(
                    request.category,
                    request.name,
                    request.missing_status,
                    format!("not available on PATH: {err}"),
                ),
            },
        )
    }
}

fn command_output_detail(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    let detail = if stdout.trim().is_empty() {
        stderr.trim()
    } else {
        stdout.trim()
    };

    if detail.is_empty() {
        "command succeeded".to_string()
    } else {
        detail.to_string()
    }
}
