use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use tentgent_kernel::{
    features::train::{
        domain::{LoraTrainRunStatus, TrainRefSelector},
        ports::LoraTrainWorkerLauncher,
        usecases::{
            LoraTrainRunInspectRequest, LoraTrainRunMarkFailedRequest, LoraTrainRunStartRequest,
            LoraTrainRunUseCase, LoraTrainRunWorkerStartedRequest,
        },
    },
    foundation::layout::{LayoutResolveMode, RuntimeLayoutInput},
};

use crate::{
    handlers::rest::jobs::{job_item, JobResponse},
    runtime::{
        JobArtifact, JobCompletion, JobId, JobKind, JobOutputLine, JobProgressUpdate, JobRegistry,
        JobStream, JobTarget,
    },
    transport::rest::{error::RestError, state::RestState},
};

const TRAIN_RUN_POLL_INTERVAL: Duration = Duration::from_secs(2);

pub async fn start_lora_run_job(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<(StatusCode, Json<JobResponse>), RestError> {
    let plan_selector = TrainRefSelector::parse(&reference).map_err(|err| {
        RestError::bad_request(
            "bad_request",
            format!("invalid train plan reference: {err}"),
        )
    })?;
    let job = state.app().jobs().create(
        JobKind::lora_train_run(),
        format!("run LoRA plan {reference}"),
        Some(JobTarget::new("train").with_reference(reference)),
        ["train".to_string(), "adapters".to_string()],
    );
    let job_id = job.job_id.clone();
    let registry = state.app().jobs().clone();
    let task_state = state.clone();
    let layout = state.app().layout_input(LayoutResolveMode::Create);

    state.app().job_runner().spawn_blocking(
        registry,
        job_id,
        "starting LoRA train run",
        move |registry, job_id| {
            run_lora_train_job(task_state, layout, plan_selector, registry, job_id)
        },
    );

    Ok((
        StatusCode::ACCEPTED,
        Json(JobResponse { job: job_item(job) }),
    ))
}

fn run_lora_train_job(
    state: RestState,
    layout: RuntimeLayoutInput,
    plan_selector: TrainRefSelector,
    registry: JobRegistry,
    job_id: JobId,
) -> Result<JobCompletion, String> {
    let run_usecase = state.app().services().kernel().train_run_usecase();
    let started = run_usecase
        .start_run(LoraTrainRunStartRequest {
            layout: layout.clone(),
            plan_selector,
        })
        .map_err(|error| error.to_string())?;
    let run_ref = started.outcome.run.run_ref.clone();
    registry.update_progress(
        &job_id,
        JobProgressUpdate {
            stage: Some("launching LoRA train worker".to_string()),
            output: vec![JobOutputLine::new(
                JobStream::Event,
                format!("created train run {}", started.outcome.run.short_ref),
            )],
            ..JobProgressUpdate::default()
        },
    );

    let launch = state
        .app()
        .services()
        .kernel()
        .training()
        .worker_launcher()
        .launch_worker(&started.layout.home_dir, &run_ref);
    let pid = match launch {
        Ok(pid) => pid,
        Err(error) => {
            let _ = state
                .app()
                .services()
                .kernel()
                .train_run_usecase()
                .mark_run_failed(LoraTrainRunMarkFailedRequest {
                    layout,
                    run_selector: TrainRefSelector::parse(&run_ref)
                        .map_err(|err| err.to_string())?,
                    phase: "worker_spawn".to_string(),
                    message: error.to_string(),
                    exit_code: None,
                });
            return Err(error.to_string());
        }
    };

    let run = state
        .app()
        .services()
        .kernel()
        .train_run_usecase()
        .record_worker_started(LoraTrainRunWorkerStartedRequest {
            layout: layout.clone(),
            run_selector: TrainRefSelector::parse(&run_ref).map_err(|err| err.to_string())?,
            pid,
        })
        .map_err(|error| error.to_string())?;
    registry.update_progress(
        &job_id,
        JobProgressUpdate {
            stage: Some("LoRA train worker running".to_string()),
            output: vec![JobOutputLine::new(
                JobStream::Event,
                format!("worker pid {pid} for train run {}", run.short_ref),
            )],
            ..JobProgressUpdate::default()
        },
    );

    wait_for_train_run_terminal(state, layout, run.run_ref, registry, job_id)
}

fn wait_for_train_run_terminal(
    state: RestState,
    layout: RuntimeLayoutInput,
    run_ref: String,
    registry: JobRegistry,
    job_id: JobId,
) -> Result<JobCompletion, String> {
    let run_selector = TrainRefSelector::parse(&run_ref).map_err(|err| err.to_string())?;
    let mut last_stage = String::new();
    loop {
        std::thread::sleep(TRAIN_RUN_POLL_INTERVAL);
        let inspection = state
            .app()
            .services()
            .kernel()
            .train_run_usecase()
            .inspect_run(LoraTrainRunInspectRequest {
                layout: layout.clone(),
                run_selector: run_selector.clone(),
            })
            .map_err(|error| error.to_string())?;
        let run = inspection.inspection.run;
        let phase = run.phase.clone().unwrap_or_else(|| run.status.to_string());
        let stage = format!("LoRA train {}: {phase}", run.status);
        let output = (stage != last_stage)
            .then(|| JobOutputLine::new(JobStream::Event, stage.clone()))
            .into_iter()
            .collect();
        last_stage = stage.clone();
        registry.update_progress(
            &job_id,
            JobProgressUpdate {
                stage: Some(stage),
                output,
                ..JobProgressUpdate::default()
            },
        );

        match run.status {
            LoraTrainRunStatus::Starting | LoraTrainRunStatus::Running => {}
            LoraTrainRunStatus::Succeeded => {
                let artifact = run
                    .adapter_ref
                    .clone()
                    .map(|adapter_ref| {
                        let mut artifact = JobArtifact::new("adapter").with_reference(adapter_ref);
                        if let Some(path) =
                            run.adapter_store_path.clone().or(run.adapter_path.clone())
                        {
                            artifact = artifact.with_path(path);
                        }
                        artifact
                    })
                    .unwrap_or_else(|| {
                        JobArtifact::new("lora_train_run")
                            .with_reference(run.run_ref.clone())
                            .with_path(run.run_dir.clone())
                    });
                return Ok(JobCompletion::new(format!(
                    "LoRA train run {} succeeded",
                    run.short_ref
                ))
                .with_artifact(artifact));
            }
            LoraTrainRunStatus::Failed => {
                return Err(run
                    .error
                    .unwrap_or_else(|| format!("LoRA train run {} failed", run.short_ref)));
            }
        }
    }
}
