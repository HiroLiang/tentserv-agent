use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};

use console::style;
use indicatif::ProgressBar;
use miette::{miette, IntoDiagnostic, Result};
use serde_json::Value;
use tentgent_kernel::features::adapter::domain::AdapterImportOutcome;
use tentgent_kernel::features::adapter::usecases::{
    AdapterTrainRunImportRequest, AdapterTrainRunImportUseCase,
};
use tentgent_kernel::features::model::domain::ModelRefSelector;
use tentgent_kernel::features::runtime::domain::{PythonRuntimeLayout, PythonRuntimeSource};
use tentgent_kernel::features::train::domain::{LoraTrainRun, LoraTrainRunStatus};
use tentgent_kernel::features::train::usecases::{
    LoraTrainRunFinishRequest, LoraTrainRunInspectRequest, LoraTrainRunUseCase,
    LoraTrainRunWorkerStartedRequest, LoraTrainRunWriteRequest,
};
use tentgent_kernel::foundation::layout::LayoutResolveMode;

use crate::cli::commands::{TrainLoraRunCommand, TrainLoraRunWorkerCommand};

use super::{
    parse_train_selector,
    run_render::render_event,
    run_summary::{render_run_summary, RunSummary},
    runtime_layout_input, runtime_layout_input_with_home, CliTrainKernel,
};

const DAEMON_TOKEN_ENV_VAR: &str = "TENTGENT_DAEMON_TOKEN";

pub fn run_lora_plan(command: TrainLoraRunCommand, kernel: &CliTrainKernel) -> Result<()> {
    let run_usecase = kernel.run_usecase();
    let plan_selector = parse_train_selector("run", "PLAN_REF", &command.reference)?;
    let started = run_usecase
        .start_run(
            tentgent_kernel::features::train::usecases::LoraTrainRunStartRequest {
                layout: runtime_layout_input(LayoutResolveMode::Create),
                plan_selector,
            },
        )
        .into_diagnostic()?;
    let outcome = started.outcome;

    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("LoRA run started").bold(),
        outcome.run.short_ref
    );
    println!(
        "{} resolved plan {}",
        style("✓").green().bold(),
        outcome.plan.short_ref
    );
    println!(
        "{} prepared run directory {}",
        style("✓").green().bold(),
        outcome.run_dir.display()
    );

    let completed = execute_training_process(
        kernel,
        runtime_layout_input(LayoutResolveMode::ReadOnly),
        outcome.run,
        RunArtifacts {
            run_dir: outcome.run_dir.clone(),
            metrics_path: outcome.metrics_path.clone(),
            raw_log_path: outcome.raw_log_path.clone(),
        },
        RunDisplay {
            verbose: command.verbose,
            debug: command.debug,
            render: true,
        },
    )?;

    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("LoRA run completed").bold(),
        completed.run.short_ref
    );
    render_run_summary(&completed.summary);
    println!("run record: {}", outcome.run_path.display());
    println!("metrics: {}", outcome.metrics_path.display());
    println!("raw log: {}", outcome.raw_log_path.display());
    if let Some(adapter_path) = &completed.run.adapter_output_path {
        println!("adapter output: {adapter_path}");
    }
    if let Some(outcome) = &completed.adapter_import {
        let status = if outcome.deduplicated {
            style("reused").yellow().bold()
        } else {
            style("imported").green().bold()
        };
        println!(
            "{} managed adapter {} at {}",
            status,
            outcome.metadata.short_ref,
            outcome.store_path.display()
        );
    }
    println!();
    Ok(())
}

pub fn run_lora_worker(command: TrainLoraRunWorkerCommand, kernel: &CliTrainKernel) -> Result<()> {
    let run_selector = parse_train_selector("run-worker", "RUN_REF", &command.run_ref)?;
    let layout = runtime_layout_input_with_home(LayoutResolveMode::ReadOnly, command.home.clone());
    let run_usecase = kernel.run_usecase();
    let run = run_usecase
        .record_worker_started(LoraTrainRunWorkerStartedRequest {
            layout: layout.clone(),
            run_selector: run_selector.clone(),
            pid: std::process::id(),
        })
        .into_diagnostic()?;
    let inspection = run_usecase
        .inspect_run(LoraTrainRunInspectRequest {
            layout: layout.clone(),
            run_selector,
        })
        .into_diagnostic()?
        .inspection;

    execute_training_process(
        kernel,
        layout,
        run,
        RunArtifacts {
            run_dir: inspection.run_dir,
            metrics_path: inspection.metrics_path,
            raw_log_path: inspection.raw_log_path,
        },
        RunDisplay {
            verbose: false,
            debug: false,
            render: false,
        },
    )?;
    Ok(())
}

fn execute_training_process(
    kernel: &CliTrainKernel,
    layout: tentgent_kernel::foundation::layout::RuntimeLayoutInput,
    mut run: LoraTrainRun,
    artifacts: RunArtifacts,
    display: RunDisplay,
) -> Result<CompletedRun> {
    let runtime_resolution = kernel.resolve_runtime(layout.clone())?;
    let python_runtime = runtime_resolution.runtime;
    let python = require_python_interpreter(
        &python_runtime,
        &kernel.python_binary_path(&python_runtime)?,
        "python training runtime",
    )?;
    let raw_log = open_append(&artifacts.raw_log_path)?;
    let raw_log = Arc::new(Mutex::new(raw_log));

    let mut process = Command::new(&python);
    process
        .current_dir(&python_runtime.project_dir)
        .env("PYTHONPATH", python_runtime.python_src_dir())
        .env_remove(DAEMON_TOKEN_ENV_VAR)
        .arg("-m")
        .arg("tentgent_daemon.cli.train_lora_run")
        .arg("--plan-ref")
        .arg(&run.plan_ref)
        .arg("--plan-file")
        .arg(
            artifacts
                .run_dir
                .parent()
                .and_then(Path::parent)
                .map(|plan_dir| plan_dir.join("plan.toml"))
                .ok_or_else(|| miette!("failed to resolve plan.toml for run"))?,
        )
        .arg("--run-dir")
        .arg(&artifacts.run_dir)
        .arg("--run-ref")
        .arg(&run.run_ref)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = process.spawn().into_diagnostic()?;
    run.pid = Some(child.id());
    run.status = LoraTrainRunStatus::Running;
    run.phase = Some("train".to_string());
    run.error = None;
    kernel
        .run_usecase()
        .write_run(LoraTrainRunWriteRequest {
            layout: layout.clone(),
            run: run.clone(),
        })
        .into_diagnostic()?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| miette!("failed to capture training runtime stderr"))?;
    let stderr_raw_log = Arc::clone(&raw_log);
    let debug = display.debug;
    let render = display.render;
    let stderr_task = thread::spawn(move || -> std::io::Result<()> {
        for line in BufReader::new(stderr).lines() {
            let line = line?;
            write_raw_line(&stderr_raw_log, "stderr", &line)?;
            if render && debug {
                eprintln!("{line}");
            }
        }
        Ok(())
    });

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| miette!("failed to capture training runtime stdout"))?;
    let mut metrics = open_append(&artifacts.metrics_path)?;
    let mut progress: Option<ProgressBar> = None;
    let mut summary = RunSummary::default();

    for line in BufReader::new(stdout).lines() {
        let line = line.into_diagnostic()?;
        write_raw_line(&raw_log, "stdout", &line).into_diagnostic()?;
        if display.render && display.debug {
            println!("{line}");
        }

        let event = match serde_json::from_str::<Value>(&line) {
            Ok(event) => event,
            Err(_) => continue,
        };

        capture_done_event(&event, &mut run);
        summary.record_event(&event);
        writeln!(metrics, "{event}").into_diagnostic()?;
        if display.render {
            render_event(&event, display.verbose, display.debug, &mut progress);
        }
    }

    let status = child.wait().into_diagnostic()?;
    stderr_task
        .join()
        .map_err(|_| miette!("training runtime stderr reader panicked"))?
        .into_diagnostic()?;

    if let Some(progress) = progress.take() {
        progress.finish_and_clear();
    }

    let mut adapter_import = None;
    if status.success() {
        if let Some(adapter_output_path) = run.adapter_output_path.clone() {
            match import_train_run_adapter(kernel, layout.clone(), &mut run, &adapter_output_path) {
                Ok(outcome) => adapter_import = Some(outcome),
                Err(err) => {
                    run.error = Some(format!(
                        "training completed, but adapter import failed: {err}"
                    ));
                    let _ = kernel
                        .run_usecase()
                        .finish_run(LoraTrainRunFinishRequest {
                            layout,
                            run,
                            status: LoraTrainRunStatus::Failed,
                            exit_code: status.code(),
                        })
                        .into_diagnostic()?;
                    return Err(miette!(
                        "LoRA training completed, but adapter import failed: {err}\n\nadapter output: {adapter_output_path}"
                    ));
                }
            }
        }
    }

    let run_status = if status.success() {
        LoraTrainRunStatus::Succeeded
    } else {
        run.error = Some(format!("training runtime exited with status {status}"));
        LoraTrainRunStatus::Failed
    };
    let run = kernel
        .run_usecase()
        .finish_run(LoraTrainRunFinishRequest {
            layout,
            run,
            status: run_status,
            exit_code: status.code(),
        })
        .into_diagnostic()?;

    if !status.success() {
        return Err(miette!(
            "LoRA training runtime exited with status {status}; raw log: {}",
            artifacts.raw_log_path.display()
        ));
    }

    Ok(CompletedRun {
        run,
        summary,
        adapter_import,
    })
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

fn open_append(path: &Path) -> Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .into_diagnostic()
}

fn write_raw_line(raw_log: &Arc<Mutex<File>>, stream: &str, line: &str) -> std::io::Result<()> {
    let mut raw_log = raw_log
        .lock()
        .map_err(|_| std::io::Error::other("raw log lock poisoned"))?;
    writeln!(raw_log, "[{stream}] {line}")
}

fn import_train_run_adapter(
    kernel: &CliTrainKernel,
    layout: tentgent_kernel::foundation::layout::RuntimeLayoutInput,
    run: &mut LoraTrainRun,
    adapter_output_path: &str,
) -> Result<AdapterImportOutcome> {
    let base_model_selector =
        ModelRefSelector::parse(&run.model_ref).map_err(|err| miette!("{err}"))?;
    let result = kernel
        .adapter_train_import_usecase()
        .import_train_run_adapter(AdapterTrainRunImportRequest {
            layout,
            output_path: PathBuf::from(adapter_output_path),
            base_model_selector,
            training_dataset_ref: run.dataset_ref.clone(),
            training_run_ref: run.run_ref.clone(),
            training_config_ref: run.plan_ref.clone(),
        })
        .into_diagnostic()?;

    run.adapter_ref = Some(result.outcome.metadata.adapter_ref.to_string());
    run.adapter_store_path = Some(result.outcome.store_path.display().to_string());
    Ok(result.outcome)
}

fn require_python_interpreter(
    runtime: &PythonRuntimeLayout,
    python: &Path,
    label: &str,
) -> Result<PathBuf> {
    if python.exists() {
        return Ok(python.to_path_buf());
    }

    Err(miette!(
        "{label} is missing at `{}`; {}",
        python.display(),
        missing_runtime_hint(runtime)
    ))
}

fn missing_runtime_hint(runtime: &PythonRuntimeLayout) -> &'static str {
    match runtime.source {
        PythonRuntimeSource::InstalledPrefix => {
            "run `tentgent runtime bootstrap`, then run `tentgent doctor` to verify the managed runtime"
        }
        PythonRuntimeSource::DevelopmentSource | PythonRuntimeSource::EnvironmentOverride => {
            "run `tentgent doctor --fix` during development or `tentgent runtime status` to inspect runtime asset paths"
        }
    }
}

struct RunArtifacts {
    run_dir: PathBuf,
    metrics_path: PathBuf,
    raw_log_path: PathBuf,
}

struct RunDisplay {
    verbose: bool,
    debug: bool,
    render: bool,
}

struct CompletedRun {
    run: LoraTrainRun,
    summary: RunSummary,
    adapter_import: Option<AdapterImportOutcome>,
}
