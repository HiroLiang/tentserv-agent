use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::features::train::ports::LoraTrainWorkerLauncher;
use crate::foundation::error::KernelResult;

use super::error::train_runtime_error;

const DAEMON_TOKEN_ENV_VAR: &str = "TENTGENT_DAEMON_TOKEN";
const CLI_BIN_ENV_VAR: &str = "TENTGENT_CLI_BIN";

/// Launches hidden detached LoRA train worker processes.
#[derive(Debug, Clone, Copy, Default)]
pub struct ShellLoraTrainWorkerLauncher;

impl LoraTrainWorkerLauncher for ShellLoraTrainWorkerLauncher {
    fn launch_worker(&self, home_dir: &Path, run_ref: &str) -> KernelResult<u32> {
        let worker = resolve_worker_binary()?;
        let mut process = Command::new("sh");
        process
            .env("TENTGENT_WORKER_BIN", &worker)
            .env_remove(DAEMON_TOKEN_ENV_VAR)
            .env("TENTGENT_HOME", home_dir)
            .arg("-c")
            .arg("nohup \"$@\" >/dev/null 2>/dev/null < /dev/null & echo $!")
            .arg("sh")
            .arg(worker)
            .arg("train")
            .arg("lora")
            .arg("run-worker")
            .arg("--home")
            .arg(home_dir)
            .arg("--run-ref")
            .arg(run_ref)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = process.output().map_err(|err| {
            train_runtime_error(format!("failed to launch LoRA training worker: {err}"))
        })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let detail = if stderr.is_empty() {
                format!("status {}", output.status)
            } else {
                stderr
            };
            return Err(train_runtime_error(format!(
                "failed to launch LoRA training worker: {detail}"
            )));
        }

        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u32>()
            .map_err(|err| train_runtime_error(format!("failed to parse worker pid: {err}")))
    }
}

fn resolve_worker_binary() -> KernelResult<PathBuf> {
    if let Some(path) = read_env_path(CLI_BIN_ENV_VAR) {
        if path.exists() {
            return Ok(path);
        }
    }

    if let Ok(current) = env::current_exe() {
        if current
            .file_stem()
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| stem == "tentgent")
        {
            return Ok(current);
        }
        if let Some(parent) = current.parent() {
            let sibling = parent.join(if cfg!(windows) {
                "tentgent.exe"
            } else {
                "tentgent"
            });
            if sibling.exists() {
                return Ok(sibling);
            }
        }
    }

    if let Some(path) = env::var_os("PATH") {
        for dir in env::split_paths(&path) {
            let candidate = dir.join(if cfg!(windows) {
                "tentgent.exe"
            } else {
                "tentgent"
            });
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    Err(train_runtime_error(
        "failed to resolve a `tentgent` worker binary; set TENTGENT_CLI_BIN",
    ))
}

fn read_env_path(name: &str) -> Option<PathBuf> {
    let value = env::var(name).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}
