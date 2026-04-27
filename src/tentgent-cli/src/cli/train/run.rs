use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};

use console::style;
use indicatif::ProgressBar;
use miette::{miette, IntoDiagnostic, Result};
use serde_json::Value;
use tentgent_core::{
    adapter::AdapterManager,
    train::{LoraTrainRun, LoraTrainRunManager, LoraTrainRunStatus},
};

use crate::cli::commands::TrainLoraRunCommand;
use crate::cli::python_runtime::{require_python_interpreter, resolve_python_runtime};

use super::{
    run_render::render_event,
    run_summary::{render_run_summary, RunSummary},
};

pub fn run_lora_plan(command: TrainLoraRunCommand) -> Result<()> {
    let manager = LoraTrainRunManager::new().into_diagnostic()?;
    let outcome = manager.start_run(&command.reference).into_diagnostic()?;
    let mut run = outcome.run.clone();

    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("LoRA run started").bold(),
        run.short_ref
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

    let python_runtime = resolve_python_runtime()?;
    let python = require_python_interpreter(&python_runtime, "python training runtime")?;
    let raw_log = open_append(&outcome.raw_log_path)?;
    let raw_log = Arc::new(Mutex::new(raw_log));

    let mut process = Command::new(&python);
    process
        .current_dir(python_runtime.project_dir())
        .env("PYTHONPATH", python_runtime.python_src_dir())
        .arg("-m")
        .arg("tentgent_daemon.cli.train_lora_run")
        .arg("--plan-ref")
        .arg(&run.plan_ref)
        .arg("--plan-file")
        .arg(
            outcome
                .run_dir
                .parent()
                .and_then(Path::parent)
                .map(|plan_dir| plan_dir.join("plan.toml"))
                .ok_or_else(|| miette!("failed to resolve plan.toml for run"))?,
        )
        .arg("--run-dir")
        .arg(&outcome.run_dir)
        .arg("--run-ref")
        .arg(&run.run_ref)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = process.spawn().into_diagnostic()?;
    run.pid = Some(child.id());
    manager.write_run(&run).into_diagnostic()?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| miette!("failed to capture training runtime stderr"))?;
    let stderr_raw_log = Arc::clone(&raw_log);
    let debug = command.debug;
    let stderr_task = thread::spawn(move || -> std::io::Result<()> {
        for line in BufReader::new(stderr).lines() {
            let line = line?;
            write_raw_line(&stderr_raw_log, "stderr", &line)?;
            if debug {
                eprintln!("{line}");
            }
        }
        Ok(())
    });

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| miette!("failed to capture training runtime stdout"))?;
    let mut metrics = open_append(&outcome.metrics_path)?;
    let mut progress: Option<ProgressBar> = None;
    let mut summary = RunSummary::default();

    for line in BufReader::new(stdout).lines() {
        let line = line.into_diagnostic()?;
        write_raw_line(&raw_log, "stdout", &line).into_diagnostic()?;
        if command.debug {
            println!("{line}");
        }

        let event = match serde_json::from_str::<Value>(&line) {
            Ok(event) => event,
            Err(_) => continue,
        };

        capture_done_event(&event, &mut run);
        summary.record_event(&event);
        writeln!(metrics, "{event}").into_diagnostic()?;
        render_event(&event, command.verbose, command.debug, &mut progress);
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
            let adapter_manager = AdapterManager::new().into_diagnostic()?;
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
                    adapter_import = Some(outcome);
                }
                Err(err) => {
                    manager
                        .finish_run(&mut run, LoraTrainRunStatus::Failed, status.code())
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
        LoraTrainRunStatus::Failed
    };
    manager
        .finish_run(&mut run, run_status, status.code())
        .into_diagnostic()?;

    if !status.success() {
        return Err(miette!(
            "LoRA training runtime exited with status {status}; raw log: {}",
            outcome.raw_log_path.display()
        ));
    }

    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("LoRA run completed").bold(),
        run.short_ref
    );
    render_run_summary(&summary);
    println!("run record: {}", outcome.run_path.display());
    println!("metrics: {}", outcome.metrics_path.display());
    println!("raw log: {}", outcome.raw_log_path.display());
    if let Some(adapter_path) = &run.adapter_output_path {
        println!("adapter output: {adapter_path}");
    }
    if let Some(outcome) = &adapter_import {
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
