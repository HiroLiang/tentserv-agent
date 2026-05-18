use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::features::adapter::domain::HfAdapterPullProgress;
use crate::features::adapter::ports::{
    HfAdapterSnapshot, HfAdapterSnapshotFetcher, HfAdapterSnapshotRequest,
};
use crate::features::runtime::domain::{
    PythonRuntimeLayout, PythonRuntimeSource, RuntimeEntrypoint,
};
use crate::foundation::error::{KernelError, KernelResult};

use super::error::{adapter_store_error, path_error};

/// Runs the managed Python Hugging Face snapshot helper for adapters.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdHfAdapterSnapshotFetcher;

impl HfAdapterSnapshotFetcher for StdHfAdapterSnapshotFetcher {
    fn fetch_hf_snapshot(
        &self,
        request: HfAdapterSnapshotRequest,
        progress: &mut dyn FnMut(HfAdapterPullProgress),
    ) -> KernelResult<HfAdapterSnapshot> {
        if !request.runtime.pyproject_path().is_file() {
            return Err(adapter_store_error(format!(
                "Hugging Face snapshot helper project is missing at `{}`",
                request.runtime.pyproject_path().display()
            )));
        }

        fs::create_dir_all(&request.destination_dir).map_err(|err| {
            path_error(
                "create HF snapshot destination",
                &request.destination_dir,
                err,
            )
        })?;
        let result_path = request
            .destination_dir
            .parent()
            .unwrap_or(request.destination_dir.as_path())
            .join("hf_snapshot_result.json");

        let mut command = hf_snapshot_command(&request.runtime)?;
        command
            .arg("--repo-id")
            .arg(&request.repo_id)
            .arg("--local-dir")
            .arg(&request.destination_dir)
            .arg("--result-path")
            .arg(&result_path)
            .arg("--progress-json")
            .env_remove("VIRTUAL_ENV")
            .env("HF_HUB_DISABLE_PROGRESS_BARS", "1")
            .stdout(Stdio::piped());

        if let Some(revision) = &request.revision {
            command.arg("--revision").arg(revision);
        }

        if let Some(secret) = &request.secret {
            command.env(secret.provider.env_var(), secret.secret());
        }

        let mut child = command.spawn().map_err(|err| {
            adapter_store_error(format!(
                "spawn Hugging Face snapshot helper for `{}` failed: {err}",
                request.repo_id
            ))
        })?;

        if let Some(stdout) = child.stdout.take() {
            for line in BufReader::new(stdout).lines() {
                let line = line.map_err(|err| {
                    adapter_store_error(format!(
                        "read Hugging Face snapshot progress for `{}` failed: {err}",
                        request.repo_id
                    ))
                })?;
                if let Some(event) = parse_hf_progress_line(&line) {
                    progress(event);
                }
            }
        }

        let status = child.wait().map_err(|err| {
            adapter_store_error(format!(
                "wait for Hugging Face snapshot helper for `{}` failed: {err}",
                request.repo_id
            ))
        })?;
        if !status.success() {
            return Err(adapter_store_error(format!(
                "Hugging Face snapshot helper for `{}` exited with status {status}",
                request.repo_id
            )));
        }

        let body = fs::read_to_string(&result_path)
            .map_err(|err| path_error("read HF snapshot result", &result_path, err))?;
        let output: HfSnapshotOutput = serde_json::from_str(&body).map_err(|err| {
            adapter_store_error(format!(
                "parse HF snapshot result `{}` failed: {err}; body was `{}`",
                result_path.display(),
                body.trim()
            ))
        })?;

        Ok(HfAdapterSnapshot {
            repo_id: output.repo_id,
            resolved_revision: output.resolved_revision,
            local_dir: PathBuf::from(output.local_dir),
        })
    }
}

fn hf_snapshot_command(runtime: &PythonRuntimeLayout) -> KernelResult<Command> {
    let script = script_bin(runtime, RuntimeEntrypoint::HfSnapshot.script_name());
    if script.exists() {
        let mut command = Command::new(script);
        command.current_dir(&runtime.project_dir);
        return Ok(command);
    }

    if runtime.source == PythonRuntimeSource::InstalledPrefix {
        return Err(KernelError::AdapterStoreUnavailable(format!(
            "Hugging Face snapshot helper is missing at `{}`; run `tentgent runtime bootstrap` or `tentgent doctor` to repair the managed runtime",
            script.display()
        )));
    }

    if let Some(parent) = runtime.env_dir.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            path_error(
                "create Python runtime environment parent for uv helper",
                parent,
                err,
            )
        })?;
    }

    let mut command = Command::new("uv");
    command
        .current_dir(&runtime.project_dir)
        .arg("--no-config")
        .arg("run")
        .arg("--project")
        .arg(&runtime.project_dir)
        .arg(RuntimeEntrypoint::HfSnapshot.script_name())
        .env("UV_PROJECT_ENVIRONMENT", &runtime.env_dir);
    Ok(command)
}

fn script_bin(runtime: &PythonRuntimeLayout, name: &str) -> PathBuf {
    python_bin_dir(&runtime.env_dir).join(python_script_name(name))
}

fn python_bin_dir(env_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        env_dir.join("Scripts")
    } else {
        env_dir.join("bin")
    }
}

fn python_script_name(name: &str) -> String {
    if cfg!(windows) && !name.ends_with(".exe") {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn parse_hf_progress_line(line: &str) -> Option<HfAdapterPullProgress> {
    let parsed = serde_json::from_str::<HfProgressLine>(line).ok()?;
    if parsed.event != "progress" {
        return None;
    }

    let position = parsed
        .position
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_default()
        .round() as u64;
    let total = parsed
        .total
        .filter(|value| value.is_finite() && *value > 0.0)
        .map(|value| value.round() as u64);

    Some(HfAdapterPullProgress {
        description: parsed.desc.unwrap_or_default(),
        position,
        total,
        unit: parsed.unit.unwrap_or_else(|| "it".to_string()),
        finished: parsed.kind == "close",
    })
}

#[derive(Debug, Deserialize)]
struct HfSnapshotOutput {
    repo_id: String,
    resolved_revision: String,
    local_dir: String,
}

#[derive(Debug, Deserialize)]
struct HfProgressLine {
    event: String,
    kind: String,
    #[serde(default)]
    desc: Option<String>,
    #[serde(default)]
    position: Option<f64>,
    #[serde(default)]
    total: Option<f64>,
    #[serde(default)]
    unit: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::parse_hf_progress_line;

    #[test]
    fn parses_hf_progress_json_lines() {
        let progress = parse_hf_progress_line(
            r#"{"event":"progress","kind":"close","desc":"file","position":2.4,"total":4.2,"unit":"B"}"#,
        )
        .expect("progress");

        assert_eq!(progress.description, "file");
        assert_eq!(progress.position, 2);
        assert_eq!(progress.total, Some(4));
        assert_eq!(progress.unit, "B");
        assert!(progress.finished);
    }

    #[test]
    fn ignores_non_progress_json_lines() {
        assert!(parse_hf_progress_line(r#"{"event":"done"}"#).is_none());
        assert!(parse_hf_progress_line("not json").is_none());
    }
}
