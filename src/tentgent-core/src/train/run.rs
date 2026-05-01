use std::{
    collections::VecDeque,
    env, fmt,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    adapter::AdapterManager,
    runtime_assets::{PythonRuntime, PythonRuntimeSource},
};

use super::{
    config::{LoraTrainBackend, LoraTrainPlan, TrainPlanStatus, LORA_TRAIN_SCHEMA_VERSION},
    error::TrainError,
    store::{imported_at_now, read_lora_train_plan, LoraTrainStorePaths},
};

const DAEMON_TOKEN_ENV_VAR: &str = "TENTGENT_DAEMON_TOKEN";
const CLI_BIN_ENV_VAR: &str = "TENTGENT_CLI_BIN";
const RAW_LOG_ENCODING: &str = "utf-8-lossy";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoraTrainRunStatus {
    Starting,
    Running,
    Succeeded,
    Failed,
}

impl LoraTrainRunStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    pub const fn is_live(self) -> bool {
        matches!(self, Self::Starting | Self::Running)
    }
}

impl fmt::Display for LoraTrainRunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoraTrainRun {
    pub schema_version: u32,
    pub run_ref: String,
    pub short_ref: String,
    pub status: LoraTrainRunStatus,
    pub phase: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub plan_ref: String,
    pub plan_short_ref: String,
    pub model_ref: String,
    pub dataset_ref: String,
    pub backend: Option<LoraTrainBackend>,
    pub recipe_hash: String,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub exit_signal: Option<String>,
    pub adapter_ref: Option<String>,
    pub adapter_path: Option<String>,
    pub adapter_output_path: Option<String>,
    pub adapter_store_path: Option<String>,
    pub run_dir: String,
    pub run_path: String,
    pub metrics_path: String,
    pub raw_log_path: String,
}

#[derive(Debug, Clone)]
pub struct LoraTrainRunStartOutcome {
    pub plan: LoraTrainPlan,
    pub run: LoraTrainRun,
    pub run_dir: PathBuf,
    pub run_path: PathBuf,
    pub metrics_path: PathBuf,
    pub raw_log_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct LoraTrainRunInspection {
    pub plan: LoraTrainPlan,
    pub run: LoraTrainRun,
    pub run_dir: PathBuf,
    pub run_path: PathBuf,
    pub metrics_path: PathBuf,
    pub raw_log_path: PathBuf,
    pub process_running: bool,
    pub stale: bool,
}

#[derive(Debug, Clone)]
pub struct LoraTrainMetricsTail {
    pub metrics_path: PathBuf,
    pub tail: usize,
    pub total_events: usize,
    pub truncated: bool,
    pub events: Vec<IndexedMetricEvent>,
    pub warnings: Vec<TrainRunWarning>,
}

#[derive(Debug, Clone)]
pub struct IndexedMetricEvent {
    pub index: usize,
    pub event: Value,
}

#[derive(Debug, Clone)]
pub struct TrainRunWarning {
    pub code: String,
    pub message: String,
    pub line: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct TrainRunLogMetadata {
    pub path: PathBuf,
    pub exists: bool,
    pub total_bytes: u64,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TrainRunLogTail {
    pub metadata: TrainRunLogMetadata,
    pub tail_bytes: u64,
    pub truncated: bool,
    pub encoding: &'static str,
    pub content: String,
}

pub struct LoraTrainRunManager {
    paths: LoraTrainStorePaths,
}

impl LoraTrainRunManager {
    pub fn new() -> Result<Self, TrainError> {
        Self::new_with_home(None)
    }

    pub fn new_with_home(home_override: Option<&Path>) -> Result<Self, TrainError> {
        let paths = LoraTrainStorePaths::resolve_with_home(home_override)?;
        paths.ensure_layout()?;
        Ok(Self { paths })
    }

    pub fn open_readonly_with_home(home_override: Option<&Path>) -> Result<Self, TrainError> {
        let paths = LoraTrainStorePaths::resolve_with_home(home_override)?;
        Ok(Self { paths })
    }

    pub fn start_run(&self, plan_reference: &str) -> Result<LoraTrainRunStartOutcome, TrainError> {
        if let Some(run) = self.live_running_run()? {
            return Err(TrainError::RunAlreadyRunning(run.run_ref));
        }

        let plan = self.resolve_plan(plan_reference)?;
        if plan.status == TrainPlanStatus::Blocked {
            return Err(TrainError::PlanBlocked {
                plan_ref: plan.plan_ref,
                reasons: plan.blockers.join("; "),
            });
        }

        let created_at = imported_at_now()?;
        let run_ref = generate_run_ref(&plan.plan_ref, &created_at);
        let short_ref = run_ref.chars().take(12).collect::<String>();
        let run_dir = self.paths.run_dir(&plan.plan_ref, &run_ref);
        let run_path = self.paths.run_toml_path(&plan.plan_ref, &run_ref);
        let metrics_path = self.paths.run_metrics_path(&plan.plan_ref, &run_ref);
        let raw_log_path = self.paths.run_raw_log_path(&plan.plan_ref, &run_ref);

        fs::create_dir_all(&run_dir)?;
        fs::write(&metrics_path, "")?;
        fs::write(&raw_log_path, "")?;

        let run = LoraTrainRun {
            schema_version: LORA_TRAIN_SCHEMA_VERSION,
            run_ref,
            short_ref,
            status: LoraTrainRunStatus::Starting,
            phase: Some("worker_spawn".to_string()),
            error: None,
            created_at: created_at.clone(),
            started_at: Some(created_at),
            ended_at: None,
            plan_ref: plan.plan_ref.clone(),
            plan_short_ref: plan.short_ref.clone(),
            model_ref: plan.model_ref.clone(),
            dataset_ref: plan.dataset_ref.clone(),
            backend: plan.backend,
            recipe_hash: plan.plan_ref.clone(),
            pid: None,
            exit_code: None,
            exit_signal: None,
            adapter_ref: None,
            adapter_path: None,
            adapter_output_path: None,
            adapter_store_path: None,
            run_dir: run_dir.display().to_string(),
            run_path: run_path.display().to_string(),
            metrics_path: metrics_path.display().to_string(),
            raw_log_path: raw_log_path.display().to_string(),
        };
        self.write_run(&run)?;

        Ok(LoraTrainRunStartOutcome {
            plan,
            run,
            run_dir,
            run_path,
            metrics_path,
            raw_log_path,
        })
    }

    pub fn record_worker_started(
        &self,
        run_reference: &str,
        pid: u32,
    ) -> Result<LoraTrainRun, TrainError> {
        let mut run = self.resolve_run(run_reference)?;
        run.pid = Some(pid);
        run.status = LoraTrainRunStatus::Running;
        run.phase = Some("train".to_string());
        run.error = None;
        if run.started_at.is_none() {
            run.started_at = Some(imported_at_now()?);
        }
        self.write_run(&run)?;
        Ok(run)
    }

    pub fn mark_run_failed(
        &self,
        run_reference: &str,
        phase: &str,
        message: impl Into<String>,
        exit_code: Option<i32>,
    ) -> Result<LoraTrainRun, TrainError> {
        let mut run = self.resolve_run(run_reference)?;
        run.status = LoraTrainRunStatus::Failed;
        run.phase = Some(phase.to_string());
        run.error = Some(message.into());
        run.ended_at = Some(imported_at_now()?);
        run.exit_code = exit_code;
        self.write_run(&run)?;
        Ok(run)
    }

    pub fn write_run(&self, run: &LoraTrainRun) -> Result<(), TrainError> {
        let run_path = self.paths.run_toml_path(&run.plan_ref, &run.run_ref);
        let body = toml::to_string_pretty(run)?;
        fs::write(run_path, body)?;
        Ok(())
    }

    pub fn finish_run(
        &self,
        run: &mut LoraTrainRun,
        status: LoraTrainRunStatus,
        exit_code: Option<i32>,
    ) -> Result<(), TrainError> {
        run.status = status;
        run.phase = Some(
            match status {
                LoraTrainRunStatus::Succeeded => "done",
                LoraTrainRunStatus::Failed => "failed",
                LoraTrainRunStatus::Starting => "worker_spawn",
                LoraTrainRunStatus::Running => "train",
            }
            .to_string(),
        );
        run.ended_at = Some(imported_at_now()?);
        run.exit_code = exit_code;
        self.write_run(run)
    }

    pub fn list_runs(&self) -> Result<Vec<LoraTrainRunInspection>, TrainError> {
        let mut runs = Vec::new();
        if !self.paths.plans_dir.exists() {
            return Ok(runs);
        }

        for plan_entry in fs::read_dir(&self.paths.plans_dir)? {
            let plan_entry = plan_entry?;
            if !plan_entry.file_type()?.is_dir() {
                continue;
            }
            let plan_ref = plan_entry.file_name().to_string_lossy().into_owned();
            runs.extend(self.list_plan_runs(&plan_ref)?);
        }

        sort_run_inspections(&mut runs);
        Ok(runs)
    }

    pub fn list_plan_runs(
        &self,
        plan_reference: &str,
    ) -> Result<Vec<LoraTrainRunInspection>, TrainError> {
        let plan = self.resolve_plan(plan_reference)?;
        let runs_dir = self.paths.plan_runs_dir(&plan.plan_ref);
        let mut runs = Vec::new();
        if !runs_dir.exists() {
            return Ok(runs);
        }

        for entry in fs::read_dir(runs_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let run_ref = entry.file_name().to_string_lossy().into_owned();
            runs.push(self.inspect_run_exact(&plan, &run_ref)?);
        }
        sort_run_inspections(&mut runs);
        Ok(runs)
    }

    pub fn inspect_run(&self, run_reference: &str) -> Result<LoraTrainRunInspection, TrainError> {
        let run = self.resolve_run(run_reference)?;
        let plan = self.resolve_plan(&run.plan_ref)?;
        self.inspect_run_from_parts(plan, run)
    }

    pub fn metrics_tail(
        &self,
        run_reference: &str,
        tail: usize,
    ) -> Result<LoraTrainMetricsTail, TrainError> {
        let inspection = self.inspect_run(run_reference)?;
        let mut total_events = 0_usize;
        let mut warnings = Vec::new();
        let mut events = VecDeque::with_capacity(tail);

        let file = match File::open(&inspection.metrics_path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(LoraTrainMetricsTail {
                    metrics_path: inspection.metrics_path,
                    tail,
                    total_events: 0,
                    truncated: false,
                    events: Vec::new(),
                    warnings: vec![TrainRunWarning {
                        code: "metrics_missing".to_string(),
                        message: "metrics.jsonl was not found".to_string(),
                        line: None,
                    }],
                });
            }
            Err(error) => return Err(error.into()),
        };

        for (line_number, line) in BufReader::new(file).lines().enumerate() {
            let line_number = line_number + 1;
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(&line) {
                Ok(event) => {
                    let index = total_events;
                    total_events += 1;
                    if events.len() == tail {
                        events.pop_front();
                    }
                    events.push_back(IndexedMetricEvent { index, event });
                }
                Err(error) => warnings.push(TrainRunWarning {
                    code: "malformed_metric".to_string(),
                    message: format!(
                        "failed to parse metrics.jsonl at line {line_number}: {error}"
                    ),
                    line: Some(line_number),
                }),
            }
        }

        Ok(LoraTrainMetricsTail {
            metrics_path: inspection.metrics_path,
            tail,
            total_events,
            truncated: total_events > tail,
            events: events.into_iter().collect(),
            warnings,
        })
    }

    pub fn raw_log_tail(
        &self,
        run_reference: &str,
        tail_bytes: u64,
    ) -> Result<TrainRunLogTail, TrainError> {
        let inspection = self.inspect_run(run_reference)?;
        let metadata = raw_log_metadata(&inspection.raw_log_path)?;
        if !metadata.exists {
            return Ok(TrainRunLogTail {
                metadata,
                tail_bytes,
                truncated: false,
                encoding: RAW_LOG_ENCODING,
                content: String::new(),
            });
        }

        let content = read_tail(&metadata.path, metadata.total_bytes, tail_bytes)?;
        let truncated = metadata.total_bytes > tail_bytes;
        Ok(TrainRunLogTail {
            metadata,
            tail_bytes,
            truncated,
            encoding: RAW_LOG_ENCODING,
            content,
        })
    }

    pub fn raw_log_metadata(&self, run_reference: &str) -> Result<TrainRunLogMetadata, TrainError> {
        let inspection = self.inspect_run(run_reference)?;
        raw_log_metadata(&inspection.raw_log_path)
    }

    fn live_running_run(&self) -> Result<Option<LoraTrainRun>, TrainError> {
        for inspection in self.list_runs()? {
            if inspection.run.status.is_live() && inspection.process_running {
                return Ok(Some(inspection.run));
            }
        }
        Ok(None)
    }

    fn inspect_run_exact(
        &self,
        plan: &LoraTrainPlan,
        run_ref: &str,
    ) -> Result<LoraTrainRunInspection, TrainError> {
        let run = read_lora_train_run(&self.paths.run_toml_path(&plan.plan_ref, run_ref))?;
        self.inspect_run_from_parts(plan.clone(), run)
    }

    fn inspect_run_from_parts(
        &self,
        plan: LoraTrainPlan,
        run: LoraTrainRun,
    ) -> Result<LoraTrainRunInspection, TrainError> {
        let process_running = match run.pid {
            Some(pid) if run.status.is_live() => is_process_running(pid)?,
            _ => false,
        };
        let stale = run.status.is_live() && run.pid.is_some() && !process_running;
        let run_dir = self.paths.run_dir(&run.plan_ref, &run.run_ref);
        let run_path = self.paths.run_toml_path(&run.plan_ref, &run.run_ref);
        let metrics_path = self.paths.run_metrics_path(&run.plan_ref, &run.run_ref);
        let raw_log_path = self.paths.run_raw_log_path(&run.plan_ref, &run.run_ref);

        Ok(LoraTrainRunInspection {
            plan,
            run,
            run_dir,
            run_path,
            metrics_path,
            raw_log_path,
            process_running,
            stale,
        })
    }

    fn resolve_plan(&self, reference: &str) -> Result<LoraTrainPlan, TrainError> {
        if reference.contains('/') || reference.is_empty() {
            return Err(TrainError::PlanNotFound(reference.to_string()));
        }
        let exact_path = self.paths.plan_toml_path(reference);
        if exact_path.exists() {
            return read_lora_train_plan(&exact_path);
        }
        if !self.paths.plans_dir.exists() {
            return Err(TrainError::PlanNotFound(reference.to_string()));
        }

        let mut matches = Vec::new();
        for entry in fs::read_dir(&self.paths.plans_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let plan_ref = entry.file_name().to_string_lossy().into_owned();
            if plan_ref.starts_with(reference) {
                matches.push(read_lora_train_plan(&self.paths.plan_toml_path(&plan_ref))?);
            }
        }

        match matches.len() {
            0 => Err(TrainError::PlanNotFound(reference.to_string())),
            1 => Ok(matches.remove(0)),
            _ => Err(TrainError::AmbiguousPlanRef(reference.to_string())),
        }
    }

    fn resolve_run(&self, reference: &str) -> Result<LoraTrainRun, TrainError> {
        if reference.contains('/') || reference.is_empty() {
            return Err(TrainError::RunNotFound(reference.to_string()));
        }
        let mut matches = Vec::new();
        for inspection in self.list_runs()? {
            if inspection.run.run_ref == reference || inspection.run.run_ref.starts_with(reference)
            {
                matches.push(inspection.run);
            }
        }

        match matches.len() {
            0 => Err(TrainError::RunNotFound(reference.to_string())),
            1 => Ok(matches.remove(0)),
            _ => Err(TrainError::AmbiguousRunRef(reference.to_string())),
        }
    }
}

pub fn launch_detached_lora_run_worker(home_dir: &Path, run_ref: &str) -> Result<u32, TrainError> {
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

    let output = process.output().map_err(TrainError::Wait)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            format!("status {}", output.status)
        } else {
            stderr
        };
        return Err(TrainError::WorkerLaunch { detail });
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()?)
}

pub fn execute_lora_run_worker(
    home_override: Option<&Path>,
    run_ref: &str,
) -> Result<LoraTrainRun, TrainError> {
    let manager = LoraTrainRunManager::new_with_home(home_override)?;
    let mut run = manager.record_worker_started(run_ref, std::process::id())?;
    let inspection = manager.inspect_run(&run.run_ref)?;

    match execute_python_training(home_override, &manager, &inspection, &mut run) {
        Ok(()) => Ok(run),
        Err(error) => {
            let phase = run.phase.clone().unwrap_or_else(|| "train".to_string());
            let _ = manager.mark_run_failed(&run.run_ref, &phase, error.to_string(), None);
            Err(error)
        }
    }
}

fn execute_python_training(
    home_override: Option<&Path>,
    manager: &LoraTrainRunManager,
    inspection: &LoraTrainRunInspection,
    run: &mut LoraTrainRun,
) -> Result<(), TrainError> {
    let python_runtime = PythonRuntime::resolve()?;
    let python = require_python_interpreter(&python_runtime, "python training runtime")?;
    let raw_log = open_append(&inspection.raw_log_path)?;
    let raw_log = Arc::new(Mutex::new(raw_log));

    let mut process = Command::new(&python);
    process
        .current_dir(python_runtime.project_dir())
        .env("PYTHONPATH", python_runtime.python_src_dir())
        .env_remove(DAEMON_TOKEN_ENV_VAR)
        .arg("-m")
        .arg("tentgent_daemon.cli.train_lora_run")
        .arg("--plan-ref")
        .arg(&run.plan_ref)
        .arg("--plan-file")
        .arg(
            inspection
                .run_dir
                .parent()
                .and_then(Path::parent)
                .map(|plan_dir| plan_dir.join("plan.toml"))
                .unwrap_or_else(|| PathBuf::from(&run.run_path)),
        )
        .arg("--run-dir")
        .arg(&inspection.run_dir)
        .arg("--run-ref")
        .arg(&run.run_ref)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = process.spawn().map_err(TrainError::Spawn)?;
    run.phase = Some("train".to_string());
    manager.write_run(run)?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| TrainError::WorkerLaunch {
            detail: "failed to capture training runtime stderr".to_string(),
        })?;
    let stderr_raw_log = Arc::clone(&raw_log);
    let stderr_task = thread::spawn(move || -> std::io::Result<()> {
        for line in BufReader::new(stderr).lines() {
            let line = line?;
            write_raw_line(&stderr_raw_log, "stderr", &line)?;
        }
        Ok(())
    });

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| TrainError::WorkerLaunch {
            detail: "failed to capture training runtime stdout".to_string(),
        })?;
    let mut metrics = open_append(&inspection.metrics_path)?;

    for line in BufReader::new(stdout).lines() {
        let line = line?;
        write_raw_line(&raw_log, "stdout", &line)?;

        let event = match serde_json::from_str::<Value>(&line) {
            Ok(event) => event,
            Err(_) => continue,
        };

        capture_done_event(&event, run);
        writeln!(metrics, "{event}")?;
    }

    let status = child.wait().map_err(TrainError::Wait)?;
    stderr_task.join().map_err(|_| TrainError::WorkerLaunch {
        detail: "training runtime stderr reader panicked".to_string(),
    })??;

    if status.success() {
        if let Some(adapter_output_path) = run.adapter_output_path.clone() {
            run.phase = Some("adapter_import".to_string());
            manager.write_run(run)?;
            let adapter_manager = AdapterManager::new_with_home(home_override)?;
            match adapter_manager.add_train_run_output(
                &adapter_output_path,
                &run.model_ref,
                &run.dataset_ref,
                &run.run_ref,
                &run.plan_ref,
            ) {
                Ok(outcome) => {
                    run.adapter_ref = Some(outcome.metadata.adapter_ref.clone());
                    run.adapter_store_path = Some(outcome.store_path.display().to_string());
                }
                Err(error) => {
                    run.error = Some(format!(
                        "training completed, but adapter import failed: {error}"
                    ));
                    manager.finish_run(run, LoraTrainRunStatus::Failed, status.code())?;
                    return Err(error.into());
                }
            }
        }
        manager.finish_run(run, LoraTrainRunStatus::Succeeded, status.code())?;
        Ok(())
    } else {
        run.error = Some(format!("training runtime exited with status {status}"));
        manager.finish_run(run, LoraTrainRunStatus::Failed, status.code())?;
        Err(TrainError::WorkerExit { status })
    }
}

fn resolve_worker_binary() -> Result<PathBuf, TrainError> {
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

    Err(TrainError::WorkerBinaryMissing)
}

fn read_lora_train_run(path: &Path) -> Result<LoraTrainRun, TrainError> {
    let body = fs::read_to_string(path)?;
    toml::from_str(&body).map_err(|err| TrainError::MetadataParse {
        path: path.to_path_buf(),
        message: err.to_string(),
    })
}

fn sort_run_inspections(runs: &mut [LoraTrainRunInspection]) {
    runs.sort_by(|left, right| {
        right
            .run
            .created_at
            .cmp(&left.run.created_at)
            .then_with(|| left.run.run_ref.cmp(&right.run.run_ref))
    });
}

fn raw_log_metadata(path: &Path) -> Result<TrainRunLogMetadata, TrainError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(TrainRunLogMetadata {
                path: path.to_path_buf(),
                exists: false,
                total_bytes: 0,
                modified_at: None,
            });
        }
        Err(error) => return Err(error.into()),
    };

    let modified_at = metadata
        .modified()
        .ok()
        .map(time::OffsetDateTime::from)
        .map(|time| time.format(&time::format_description::well_known::Rfc3339))
        .transpose()?;

    Ok(TrainRunLogMetadata {
        path: path.to_path_buf(),
        exists: true,
        total_bytes: metadata.len(),
        modified_at,
    })
}

fn read_tail(path: &Path, total_bytes: u64, tail_bytes: u64) -> Result<String, TrainError> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => return Err(error.into()),
    };

    file.seek(SeekFrom::Start(total_bytes.saturating_sub(tail_bytes)))?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

fn require_python_interpreter(
    runtime: &PythonRuntime,
    label: &'static str,
) -> Result<PathBuf, TrainError> {
    let python = runtime.python_bin();
    if python.exists() {
        return Ok(python);
    }

    Err(TrainError::MissingPythonInterpreter {
        label,
        path: python,
        hint: missing_runtime_hint(runtime),
    })
}

fn missing_runtime_hint(runtime: &PythonRuntime) -> &'static str {
    match runtime.source() {
        PythonRuntimeSource::InstalledPrefix => {
            "run the installer Python bootstrap, then run `tentgent doctor` to verify the managed runtime"
        }
        PythonRuntimeSource::DevelopmentSource | PythonRuntimeSource::EnvironmentOverride => {
            "run `tentgent doctor --fix` during development or `tentgent status` to inspect runtime asset paths"
        }
    }
}

fn capture_done_event(event: &Value, run: &mut LoraTrainRun) {
    if event.get("type").and_then(Value::as_str) != Some("done") {
        return;
    }
    run.adapter_path = event
        .get("adapter_path")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    run.adapter_output_path = run.adapter_path.clone();
    run.adapter_ref = event
        .get("adapter_ref")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
}

fn open_append(path: &Path) -> Result<File, TrainError> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(Into::into)
}

fn write_raw_line(raw_log: &Arc<Mutex<File>>, stream: &str, line: &str) -> std::io::Result<()> {
    let mut raw_log = raw_log
        .lock()
        .map_err(|_| std::io::Error::other("raw log lock poisoned"))?;
    writeln!(raw_log, "[{stream}] {line}")
}

fn generate_run_ref(plan_ref: &str, created_at: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plan_ref.as_bytes());
    hasher.update(b"\0");
    hasher.update(created_at.as_bytes());
    hasher.update(b"\0");
    hasher.update(std::process::id().to_string().as_bytes());
    hex::encode(hasher.finalize())
}

fn is_process_running(pid: u32) -> Result<bool, TrainError> {
    let status = Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    Ok(status.success())
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
