use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::features::doctor::domain::{
    DoctorCheck, DoctorExecutionMode, DoctorPathCheck, DoctorPathExpectation,
};
use crate::features::doctor::ports::DoctorPathProbe;
use crate::foundation::error::KernelResult;

/// Checks filesystem path health for doctor diagnostics.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdDoctorPathProbe;

impl DoctorPathProbe for StdDoctorPathProbe {
    fn check_path(&self, request: DoctorPathCheck) -> KernelResult<DoctorCheck> {
        Ok(match request.expectation {
            DoctorPathExpectation::RequiredDirectory => check_required_directory(request),
            DoctorPathExpectation::OptionalDirectory => check_optional_directory(request),
            DoctorPathExpectation::RequiredFile => check_required_file(request),
            DoctorPathExpectation::ExecutableFile => check_executable_file(request),
        })
    }
}

fn check_required_directory(request: DoctorPathCheck) -> DoctorCheck {
    if request.path.exists() {
        if !request.path.is_dir() {
            return DoctorCheck::fail(
                request.category,
                request.name,
                format!("not a directory: {}", request.path.display()),
            );
        }
        if request.mode == DoctorExecutionMode::Observational {
            return DoctorCheck::pass(
                request.category,
                request.name,
                format!("present: {}", request.path.display()),
            );
        }
        return match write_probe(&request.path) {
            Ok(()) => DoctorCheck::pass(
                request.category,
                request.name,
                format!("writable: {}", request.path.display()),
            ),
            Err(err) => DoctorCheck::fail(
                request.category,
                request.name,
                format!("not writable: {}; {err}", request.path.display()),
            ),
        };
    }

    if request.mode == DoctorExecutionMode::Observational {
        return DoctorCheck::warn(
            request.category,
            request.name,
            format!(
                "missing: {}; observational doctor did not create it",
                request.path.display()
            ),
        );
    }

    match nearest_existing_parent(&request.path)
        .and_then(|parent| write_probe(&parent).map(|_| parent))
    {
        Ok(parent) => DoctorCheck::pass(
            request.category,
            request.name,
            format!(
                "creatable on demand; parent is writable: {}",
                parent.display()
            ),
        ),
        Err(err) => DoctorCheck::fail(
            request.category,
            request.name,
            format!(
                "missing and cannot verify creation: {}; {err}",
                request.path.display()
            ),
        ),
    }
}

fn check_optional_directory(request: DoctorPathCheck) -> DoctorCheck {
    if request.path.exists() {
        if !request.path.is_dir() {
            return DoctorCheck::warn(
                request.category,
                request.name,
                format!(
                    "optional path exists but is not a directory: {}",
                    request.path.display()
                ),
            );
        }
        if request.mode == DoctorExecutionMode::Observational {
            return DoctorCheck::pass(
                request.category,
                request.name,
                format!("present: {}", request.path.display()),
            );
        }
        return match write_probe(&request.path) {
            Ok(()) => DoctorCheck::pass(
                request.category,
                request.name,
                format!("present and writable: {}", request.path.display()),
            ),
            Err(err) => DoctorCheck::warn(
                request.category,
                request.name,
                format!(
                    "optional path is not writable: {}; {err}",
                    request.path.display()
                ),
            ),
        };
    }

    if request.mode == DoctorExecutionMode::Observational {
        return DoctorCheck::skipped(
            request.category,
            request.name,
            format!("optional path is missing: {}", request.path.display()),
        );
    }

    match nearest_existing_parent(&request.path)
        .and_then(|parent| write_probe(&parent).map(|_| parent))
    {
        Ok(parent) => DoctorCheck::pass(
            request.category,
            request.name,
            format!(
                "optional; installer can create it on demand; parent is writable: {}",
                parent.display()
            ),
        ),
        Err(err) => DoctorCheck::warn(
            request.category,
            request.name,
            format!(
                "optional but installer may not be able to create it: {}; {err}",
                request.path.display()
            ),
        ),
    }
}

fn check_required_file(request: DoctorPathCheck) -> DoctorCheck {
    if request.path.is_file() {
        return DoctorCheck::pass(
            request.category,
            request.name,
            format!("present: {}", request.path.display()),
        );
    }

    let detail = if request.path.exists() {
        format!("not a file: {}", request.path.display())
    } else {
        format!("missing: {}", request.path.display())
    };
    DoctorCheck::fail(request.category, request.name, detail)
}

fn check_executable_file(request: DoctorPathCheck) -> DoctorCheck {
    if is_executable_file(&request.path) {
        return DoctorCheck::pass(
            request.category,
            request.name,
            format!("executable: {}", request.path.display()),
        );
    }

    let detail = if request.path.is_file() {
        format!("not executable: {}", request.path.display())
    } else if request.path.exists() {
        format!("not a file: {}", request.path.display())
    } else {
        format!("missing: {}", request.path.display())
    };
    DoctorCheck::fail(request.category, request.name, detail)
}

fn nearest_existing_parent(path: &Path) -> std::io::Result<PathBuf> {
    let mut cursor = path;
    loop {
        if cursor.exists() {
            return Ok(cursor.to_path_buf());
        }
        cursor = cursor
            .parent()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no parent"))?;
    }
}

fn write_probe(dir: &Path) -> std::io::Result<()> {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let path = dir.join(format!(
        ".tentgent-doctor-probe-{}-{id}",
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(b"probe")?;
    drop(file);
    fs::remove_file(path)?;
    Ok(())
}

fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return path
            .metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }

    #[cfg(not(unix))]
    {
        true
    }
}
