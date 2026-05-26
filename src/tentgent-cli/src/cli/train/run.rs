use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use console::style;
use indicatif::ProgressBar;
use miette::{miette, IntoDiagnostic, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tentgent_kernel::features::adapter::domain::AdapterImportOutcome;
use tentgent_kernel::features::adapter::usecases::{
    AdapterImportOptions, AdapterTrainRunImportRequest, AdapterTrainRunImportUseCase,
};
use tentgent_kernel::features::model::domain::ModelRefSelector;
use tentgent_kernel::features::runtime::infra::ModelRuntimeCapability;
use tentgent_kernel::features::train::domain::{
    LoraTrainBackend, LoraTrainPlan, LoraTrainRun, LoraTrainRunStatus, TrainRefSelector,
};
use tentgent_kernel::features::train::usecases::{
    LoraTrainPlanInspectRequest, LoraTrainPlanUseCase, LoraTrainRunFinishRequest,
    LoraTrainRunInspectRequest, LoraTrainRunUseCase, LoraTrainRunWorkerStartedRequest,
    LoraTrainRunWriteRequest,
};
use tentgent_kernel::foundation::layout::LayoutResolveMode;

use crate::cli::commands::{TrainLoraRunCommand, TrainLoraRunWorkerCommand};

use super::{
    parse_train_selector,
    run_render::render_event,
    run_summary::{render_run_summary, RunSummary},
    runtime_layout_input, runtime_layout_input_with_home, CliTrainKernel,
};

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
    let plan_selector = TrainRefSelector::parse(&run.plan_ref).map_err(|err| miette!("{err}"))?;
    let plan = kernel
        .plan_usecase()
        .inspect_plan(LoraTrainPlanInspectRequest {
            layout: layout.clone(),
            selector: plan_selector,
        })
        .into_diagnostic()?
        .inspection
        .plan;
    let backend = plan
        .backend
        .ok_or_else(|| miette!("LoRA train plan is blocked and has no selected backend"))?;
    let raw_log = open_append(&artifacts.raw_log_path)?;
    let raw_log = Arc::new(Mutex::new(raw_log));
    let endpoint = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(kernel.model_runtime_supervisor.ensure_unbound(
            &runtime_resolution.layout,
            &runtime_resolution.runtime,
            &kernel.executable_resolver,
            ModelRuntimeCapability::LoraTuning,
        ))
    })
    .into_diagnostic()?;

    run.pid = Some(endpoint.pid);
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

    let mut metrics = open_append(&artifacts.metrics_path)?;
    let mut progress: Option<ProgressBar> = None;
    let mut summary = RunSummary::default();
    let payload = lora_tuning_payload(&plan, backend, &run, &artifacts.run_dir)?;
    let response_result: tentgent_kernel::foundation::error::KernelResult<
        LoraTuningResponsePayload,
    > = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(kernel.model_runtime_supervisor.post_json(
            &endpoint,
            "/v1/tuning/lora/runs",
            &payload,
            |message| {
                tentgent_kernel::foundation::error::KernelError::TrainRuntimeUnavailable(message)
            },
        ))
    });
    let response = match response_result {
        Ok(response) => response,
        Err(err) => {
            run.error = Some(err.to_string());
            let _ = kernel
                .run_usecase()
                .finish_run(LoraTrainRunFinishRequest {
                    layout,
                    run,
                    status: LoraTrainRunStatus::Failed,
                    exit_code: None,
                })
                .into_diagnostic()?;
            return Err(miette!("LoRA training failed: {err}"));
        }
    };
    if response.status != "done" {
        run.error = Some(format!(
            "model runtime returned non-success LoRA tuning status `{}`",
            response.status
        ));
        let _ = kernel
            .run_usecase()
            .finish_run(LoraTrainRunFinishRequest {
                layout,
                run,
                status: LoraTrainRunStatus::Failed,
                exit_code: None,
            })
            .into_diagnostic()?;
        return Err(miette!(
            "LoRA training failed: model runtime returned status `{}`",
            response.status
        ));
    }

    for event in response.events {
        let line = serde_json::to_string(&event).into_diagnostic()?;
        write_raw_line(&raw_log, "stdout", &line).into_diagnostic()?;
        if display.render && display.debug {
            println!("{line}");
        }

        capture_done_event(&event, &mut run);
        summary.record_event(&event);
        writeln!(metrics, "{event}").into_diagnostic()?;
        if display.render {
            render_event(&event, display.verbose, display.debug, &mut progress);
        }
    }
    if run.adapter_output_path.is_none() {
        run.adapter_path = Some(response.adapter_path.clone());
        run.adapter_output_path = Some(response.adapter_path.clone());
    }

    if let Some(progress) = progress.take() {
        progress.finish_and_clear();
    }

    let mut adapter_import = None;
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
                        exit_code: None,
                    })
                    .into_diagnostic()?;
                return Err(miette!(
                    "LoRA training completed, but adapter import failed: {err}\n\nadapter output: {adapter_output_path}"
                ));
            }
        }
    }

    let run = kernel
        .run_usecase()
        .finish_run(LoraTrainRunFinishRequest {
            layout,
            run,
            status: LoraTrainRunStatus::Succeeded,
            exit_code: None,
        })
        .into_diagnostic()?;

    Ok(CompletedRun {
        run,
        summary,
        adapter_import,
    })
}

fn lora_tuning_payload(
    plan: &LoraTrainPlan,
    backend: LoraTrainBackend,
    run: &LoraTrainRun,
    run_dir: &Path,
) -> Result<LoraTuningRequestPayload> {
    Ok(LoraTuningRequestPayload {
        backend: backend.as_str().to_string(),
        model: LoraModelPayload {
            model_ref: plan.model_ref.clone(),
            source_path: plan.model.source_path.clone(),
            primary_format: plan.model.primary_format.clone(),
            capabilities: vec!["chat".to_string()],
            short_ref: Some(plan.model_short_ref.clone()),
        },
        dataset: LoraDatasetPayload {
            source_path: plan.dataset.source_path.clone(),
            max_seq_length: plan.dataset.max_seq_length,
            mask_prompt: plan.dataset.mask_prompt,
        },
        output_dir: run_dir.display().to_string(),
        lora: LoraConfigPayload {
            rank: plan.lora.rank,
            alpha: plan.lora.alpha,
            dropout: plan.lora.dropout,
            scale: plan.lora.scale.unwrap_or(20.0),
            target_modules: plan.lora.target_modules.clone(),
        },
        optimization: LoraOptimizationPayload {
            max_steps: plan.optimization.max_steps,
            batch_size: plan.optimization.batch_size,
            learning_rate: plan.optimization.learning_rate,
            weight_decay: plan.optimization.weight_decay,
            gradient_accumulation_steps: plan.optimization.gradient_accumulation_steps,
            optimizer: plan.optimization.optimizer.clone(),
            seed: plan.optimization.seed,
        },
        checkpoint: LoraCheckpointPayload {
            log_every_steps: plan.checkpoint.log_every_steps,
            eval_every_steps: plan.checkpoint.eval_every_steps,
            save_every_steps: plan.checkpoint.save_every_steps,
        },
        backend_config: LoraBackendConfigPayload {
            peft: backend_config_object(&plan.backend_config.peft, "peft")?,
            mlx: backend_config_object(&plan.backend_config.mlx, "mlx")?,
        },
        plan_ref: Some(plan.plan_ref.clone()),
        run_ref: Some(run.run_ref.clone()),
    })
}

fn backend_config_object<T: Serialize>(config: &Option<T>, label: &str) -> Result<Value> {
    let Some(config) = config else {
        return Ok(Value::Object(Default::default()));
    };
    let value = serde_json::to_value(config)
        .map_err(|err| miette!("failed to serialize {label} LoRA backend config: {err}"))?;
    match value {
        Value::Object(_) => Ok(value),
        _ => Err(miette!(
            "{label} LoRA backend config did not serialize as an object"
        )),
    }
}

#[derive(Debug, Serialize)]
struct LoraTuningRequestPayload {
    backend: String,
    model: LoraModelPayload,
    dataset: LoraDatasetPayload,
    output_dir: String,
    lora: LoraConfigPayload,
    optimization: LoraOptimizationPayload,
    checkpoint: LoraCheckpointPayload,
    backend_config: LoraBackendConfigPayload,
    plan_ref: Option<String>,
    run_ref: Option<String>,
}

#[derive(Debug, Serialize)]
struct LoraModelPayload {
    model_ref: String,
    source_path: String,
    primary_format: String,
    capabilities: Vec<String>,
    short_ref: Option<String>,
}

#[derive(Debug, Serialize)]
struct LoraDatasetPayload {
    source_path: String,
    max_seq_length: u32,
    mask_prompt: bool,
}

#[derive(Debug, Serialize)]
struct LoraConfigPayload {
    rank: u32,
    alpha: Option<u32>,
    dropout: f32,
    scale: f32,
    target_modules: Vec<String>,
}

#[derive(Debug, Serialize)]
struct LoraOptimizationPayload {
    max_steps: u32,
    batch_size: u32,
    learning_rate: f64,
    weight_decay: f64,
    gradient_accumulation_steps: u32,
    optimizer: String,
    seed: u64,
}

#[derive(Debug, Serialize)]
struct LoraCheckpointPayload {
    log_every_steps: u32,
    eval_every_steps: u32,
    save_every_steps: u32,
}

#[derive(Debug, Serialize)]
struct LoraBackendConfigPayload {
    peft: Value,
    mlx: Value,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct LoraTuningResponsePayload {
    task_ref: String,
    status: String,
    model_ref: String,
    backend: String,
    output_dir: String,
    adapter_path: String,
    adapter_file: Option<String>,
    finish_reason: String,
    events: Vec<Value>,
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
            options: AdapterImportOptions::default(),
        })
        .into_diagnostic()?;

    run.adapter_ref = Some(result.outcome.metadata.adapter_ref.to_string());
    run.adapter_store_path = Some(result.outcome.store_path.display().to_string());
    Ok(result.outcome)
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
