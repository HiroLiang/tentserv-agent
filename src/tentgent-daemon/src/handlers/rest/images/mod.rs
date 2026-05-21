use std::fs;
use std::path::Path as StdPath;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
        HeaderValue, StatusCode,
    },
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use tentgent_kernel::{
    features::{
        adapter::domain::{AdapterRefSelector, LoraScale},
        image_generation::{
            domain::{
                ImageGenerationDimensions, ImageGenerationOptions, ImageGenerationOutputFormat,
            },
            usecases::{ImageGenerationPreparationRequest, ImageGenerationUseCase},
        },
        job::{
            domain::{JobResultFile, JobResultFileList, JobWorkspaceStreamSummary},
            infra::FileJobWorkspaceStore,
            ports::{JobChunkPort, JobResultPort, JobStreamKind, JobWorkspacePort},
        },
        model::{
            domain::ModelRefSelector,
            usecases::{ModelCatalogReadUseCase, ModelListRequest},
        },
        runtime::domain::PythonRuntimeResolutionInput,
    },
    foundation::layout::{LayoutResolveMode, RuntimeLayoutInput},
};

use crate::{
    handlers::rest::jobs::{job_item, JobResponse},
    runtime::{
        JobArtifact, JobCompletion, JobId, JobKind, JobOutputLine, JobProgressPatch,
        JobProgressUpdate, JobRegistry, JobStatus, JobStream, JobTarget,
    },
    transport::rest::{error::RestError, state::RestState},
};

pub async fn create_generation_job(
    State(state): State<RestState>,
    Json(request): Json<ImageGenerationJobRequest>,
) -> Result<(StatusCode, Json<JobResponse>), RestError> {
    let request = ParsedImageGenerationJobRequest::from_request(&state, request)?;
    let label = "generate image".to_string();
    let job = state.app().jobs().create(
        JobKind::image_generation(),
        label,
        Some(JobTarget::new("image").with_reference(request.model_label.clone())),
        Vec::<String>::new(),
    );
    let job_id = job.job_id.clone();
    spawn_image_generation_worker(state, job_id, request);

    Ok((
        StatusCode::ACCEPTED,
        Json(JobResponse { job: job_item(job) }),
    ))
}

pub async fn generation_job_files(
    State(state): State<RestState>,
    Path(job_id): Path<String>,
) -> Result<Json<JobResultFileList>, RestError> {
    let job_id = JobId::new(job_id);
    let job = image_generation_job(&state, &job_id)?;
    let store = FileJobWorkspaceStore::from_runtime_dir(state.app().layout().runtime_dir.clone());
    let result_files = store
        .list_result_files(&job_id)
        .map_err(|error| RestError::kernel("image_generation_result_failed", error))?;
    if result_files.files.is_empty() {
        return Err(result_pending_or_terminal_error(&job, "result files"));
    }

    Ok(Json(result_files))
}

pub async fn generation_job_file(
    State(state): State<RestState>,
    Path((job_id, file_id)): Path<(String, String)>,
) -> Result<Response, RestError> {
    let job_id = JobId::new(job_id);
    let job = image_generation_job(&state, &job_id)?;
    let store = FileJobWorkspaceStore::from_runtime_dir(state.app().layout().runtime_dir.clone());
    let result_files = store
        .list_result_files(&job_id)
        .map_err(|error| RestError::kernel("image_generation_result_failed", error))?;
    let Some(result_file) = result_files
        .files
        .iter()
        .find(|file| file.file_id == file_id)
    else {
        if result_files.files.is_empty() {
            return Err(result_pending_or_terminal_error(&job, "result file"));
        }
        return Err(RestError::not_found(
            "result_not_found",
            format!("image generation result file `{file_id}` was not found for job `{job_id}`"),
        ));
    };
    let bytes = store
        .read_result_file(&job_id, &file_id)
        .map_err(|error| RestError::kernel("image_generation_result_failed", error))?;

    bytes_response(
        bytes,
        result_file
            .media_type
            .as_deref()
            .unwrap_or("application/octet-stream"),
        &result_file.filename,
        &job_id,
        &file_id,
    )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageGenerationJobRequest {
    pub model_ref: String,
    pub adapter_ref: Option<String>,
    pub lora_scale: Option<f32>,
    pub prompt: String,
    pub negative_prompt: Option<String>,
    pub output_format: Option<String>,
    pub output_filename: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub steps: Option<u32>,
    pub guidance_scale: Option<f32>,
    pub seed: Option<u64>,
}

#[derive(Debug)]
struct ParsedImageGenerationJobRequest {
    model_label: String,
    model_selector: ModelRefSelector,
    adapter_selector: Option<AdapterRefSelector>,
    lora_scale: Option<LoraScale>,
    prompt: String,
    negative_prompt: Option<String>,
    output_format: ImageGenerationOutputFormat,
    output_filename: String,
    options: ImageGenerationOptions,
}

impl ParsedImageGenerationJobRequest {
    fn from_request(
        state: &RestState,
        request: ImageGenerationJobRequest,
    ) -> Result<Self, RestError> {
        let model_label = request.model_ref.trim().to_string();
        let model_selector = model_selector(state, &model_label)?;
        let adapter_selector = optional_trimmed_string(request.adapter_ref)
            .map(|value| {
                AdapterRefSelector::parse(value.as_str()).map_err(|error| {
                    RestError::bad_request("bad_request", format!("invalid `adapter_ref`: {error}"))
                })
            })
            .transpose()?;
        let lora_scale = request
            .lora_scale
            .map(|value| {
                LoraScale::new(value)
                    .map_err(|error| RestError::bad_request("bad_request", error.to_string()))
            })
            .transpose()?;
        let prompt = optional_trimmed_string(Some(request.prompt))
            .ok_or_else(|| RestError::bad_request("bad_request", "`prompt` is required"))?;
        let output_format = request
            .output_format
            .as_deref()
            .unwrap_or(ImageGenerationOutputFormat::Png.as_str())
            .parse::<ImageGenerationOutputFormat>()
            .map_err(|error| RestError::bad_request("bad_request", error.to_string()))?;
        let output_filename = result_filename(request.output_filename, output_format)?;
        let width = request
            .width
            .unwrap_or(ImageGenerationDimensions::DEFAULT_WIDTH);
        let height = request
            .height
            .unwrap_or(ImageGenerationDimensions::DEFAULT_HEIGHT);
        let dimensions = ImageGenerationDimensions::new(width, height)
            .map_err(|error| RestError::bad_request("bad_request", error.to_string()))?;
        let options = ImageGenerationOptions::new(
            dimensions,
            request
                .steps
                .unwrap_or(ImageGenerationOptions::DEFAULT_STEPS),
            request
                .guidance_scale
                .unwrap_or(ImageGenerationOptions::DEFAULT_GUIDANCE_SCALE),
            request.seed,
        )
        .map_err(|error| RestError::bad_request("bad_request", error.to_string()))?;

        Ok(Self {
            model_label,
            model_selector,
            adapter_selector,
            lora_scale,
            prompt,
            negative_prompt: optional_trimmed_string(request.negative_prompt),
            output_format,
            output_filename,
            options,
        })
    }
}

fn spawn_image_generation_worker(
    state: RestState,
    job_id: JobId,
    request: ParsedImageGenerationJobRequest,
) {
    let registry = state.app().jobs().clone();
    let task_state = state.clone();
    let layout = state.app().layout_input(LayoutResolveMode::Create);
    let handle = tokio::runtime::Handle::current();

    state.app().job_runner().spawn_blocking(
        registry,
        job_id,
        "preparing image generation",
        move |registry, job_id| {
            run_image_generation_job(task_state, layout, request, handle, registry, job_id)
        },
    );
}

fn run_image_generation_job(
    state: RestState,
    layout: RuntimeLayoutInput,
    request: ParsedImageGenerationJobRequest,
    handle: tokio::runtime::Handle,
    registry: JobRegistry,
    job_id: JobId,
) -> Result<JobCompletion, String> {
    let store = FileJobWorkspaceStore::from_runtime_dir(state.app().layout().runtime_dir.clone());
    let workspace = store
        .open_workspace(&job_id)
        .map_err(|error| error.to_string())?;
    let workspace_summary = store
        .finalize_stream(
            &job_id,
            JobStreamKind::Input,
            JobWorkspaceStreamSummary {
                state: "done".to_string(),
                done: true,
                failed: false,
                chunk_count: 0,
                total_bytes: request.prompt.len() as u64,
                sha256: None,
                media_type: Some("text/plain".to_string()),
                original_filename: None,
            },
        )
        .map_err(|error| error.to_string())?;
    registry.update_workspace(&job_id, workspace_summary);
    registry.update_progress(
        &job_id,
        JobProgressUpdate {
            stage: Some("running image generation".to_string()),
            progress: JobProgressPatch {
                files_total: Some(1),
                files_done: Some(0),
                ..JobProgressPatch::default()
            },
            output: vec![JobOutputLine::new(JobStream::Event, "running diffusion")],
            ..JobProgressUpdate::default()
        },
    );

    let output_path = workspace
        .workspace_dir
        .join("files")
        .join(&request.output_filename);
    let output_format = request.output_format;
    let execution = handle
        .block_on(async {
            state
                .app()
                .services()
                .kernel()
                .image_generation_usecase()
                .generate_image(ImageGenerationPreparationRequest {
                    layout,
                    runtime: PythonRuntimeResolutionInput::default(),
                    model_selector: request.model_selector,
                    adapter_selector: request.adapter_selector,
                    lora_scale: request.lora_scale,
                    prompt: request.prompt,
                    negative_prompt: request.negative_prompt,
                    output_path: output_path.clone(),
                    output_format,
                    options: request.options,
                })
                .await
        })
        .map_err(|error| error.to_string())?;

    let output_path = execution.response.output_path.clone();
    let metadata = fs::metadata(&output_path).map_err(|error| {
        format!(
            "failed to inspect image generation output `{}`: {error}",
            output_path.display()
        )
    })?;
    let result_summary = JobWorkspaceStreamSummary {
        state: "done".to_string(),
        done: true,
        failed: false,
        chunk_count: 1,
        total_bytes: metadata.len(),
        sha256: None,
        media_type: Some(execution.response.media_type.clone()),
        original_filename: Some(request.output_filename.clone()),
    };
    let workspace_summary = store
        .finalize_stream(&job_id, JobStreamKind::Result, result_summary)
        .map_err(|error| error.to_string())?;
    store
        .declare_result_file(
            &job_id,
            JobResultFile {
                file_id: request.output_filename.clone(),
                filename: request.output_filename.clone(),
                media_type: Some(execution.response.media_type.clone()),
                format: Some(execution.response.output_format.as_str().to_string()),
                total_bytes: execution.response.total_bytes,
            },
        )
        .map_err(|error| error.to_string())?;
    registry.update_workspace(&job_id, workspace_summary);
    registry.update_progress(
        &job_id,
        JobProgressUpdate {
            stage: Some("image generation finished".to_string()),
            progress: JobProgressPatch {
                bytes_done: Some(execution.response.total_bytes),
                bytes_total: Some(execution.response.total_bytes),
                files_done: Some(1),
                files_total: Some(1),
                percent: Some(100.0),
                ..JobProgressPatch::default()
            },
            output: vec![JobOutputLine::new(
                JobStream::Event,
                format!("wrote {}", output_path.display()),
            )],
            ..JobProgressUpdate::default()
        },
    );

    Ok(JobCompletion::new(format!(
        "image generation wrote {}",
        request.output_filename
    ))
    .with_artifact(
        JobArtifact::new("image_generation").with_path(output_path.display().to_string()),
    ))
}

fn image_generation_job(
    state: &RestState,
    job_id: &JobId,
) -> Result<crate::runtime::JobItem, RestError> {
    let Some(job) = state.app().jobs().get(job_id) else {
        return Err(RestError::not_found(
            "not_found",
            format!("job `{job_id}` was not found"),
        ));
    };
    if job.kind.as_str() != JobKind::IMAGE_GENERATION {
        return Err(RestError::conflict(
            "wrong_job_kind",
            format!("job `{job_id}` is not an image generation job"),
        ));
    }
    Ok(job)
}

fn result_pending_or_terminal_error(job: &crate::runtime::JobItem, noun: &str) -> RestError {
    let job_id = &job.job_id;
    if job.status.is_terminal() {
        match job.status {
            JobStatus::Failed => {
                return RestError::conflict(
                    "job_failed",
                    format!(
                        "image generation job `{job_id}` failed before producing {noun}; inspect `/v1/jobs/{job_id}` for details"
                    ),
                );
            }
            JobStatus::Interrupted => {
                return RestError::conflict(
                    "job_interrupted",
                    format!(
                        "image generation job `{job_id}` was interrupted before producing {noun}"
                    ),
                );
            }
            JobStatus::Canceled => {
                return RestError::conflict(
                    "job_canceled",
                    format!("image generation job `{job_id}` was canceled before producing {noun}"),
                );
            }
            JobStatus::Succeeded => {}
            JobStatus::Queued | JobStatus::Running => {}
        }
        return RestError::not_found(
            "result_not_found",
            format!("image generation {noun} for job `{job_id}` was not found"),
        );
    }
    RestError::conflict(
        "result_pending",
        format!("image generation {noun} for job `{job_id}` is not ready yet"),
    )
}

fn model_selector(state: &RestState, value: &str) -> Result<ModelRefSelector, RestError> {
    match ModelRefSelector::parse(value) {
        Ok(selector) => Ok(selector),
        Err(_) => model_alias_selector(state, value).map_err(|alias_error| alias_error.error),
    }
}

fn model_alias_selector(
    state: &RestState,
    value: &str,
) -> Result<ModelRefSelector, ModelAliasError> {
    let alias = value.trim();
    if alias.is_empty() {
        return Err(ModelAliasError {
            error: RestError::bad_request("bad_request", "model reference is empty"),
        });
    }
    let result = state
        .app()
        .services()
        .kernel()
        .models()
        .list_models(ModelListRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
        })
        .map_err(|error| ModelAliasError {
            error: RestError::store_lookup("image_generation_model_failed", error.to_string()),
        })?;

    let matches = result
        .models
        .into_iter()
        .filter(|model| model_alias_matches(alias, model.metadata.source_repo.as_deref()))
        .map(|model| model.metadata.model_ref)
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [] => Err(ModelAliasError {
            error: RestError::not_found(
                "not_found",
                format!("model alias `{alias}` was not found"),
            ),
        }),
        [model_ref] => ModelRefSelector::parse(model_ref.as_str()).map_err(|err| ModelAliasError {
            error: RestError::internal("image_generation_model_failed", err.to_string()),
        }),
        _ => Err(ModelAliasError {
            error: RestError::conflict(
                "ambiguous_ref",
                format!("model alias `{alias}` matches multiple stored models"),
            ),
        }),
    }
}

fn model_alias_matches(alias: &str, source_repo: Option<&str>) -> bool {
    let Some(source_repo) = source_repo else {
        return false;
    };
    source_repo.eq_ignore_ascii_case(alias)
        || source_repo
            .rsplit('/')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case(alias))
}

struct ModelAliasError {
    error: RestError,
}

fn result_filename(
    value: Option<String>,
    output_format: ImageGenerationOutputFormat,
) -> Result<String, RestError> {
    let filename = value
        .and_then(|value| optional_trimmed_string(Some(value)))
        .unwrap_or_else(|| output_format.default_filename());
    let path = StdPath::new(&filename);
    if path.file_name().and_then(|value| value.to_str()) != Some(filename.as_str()) {
        return Err(RestError::bad_request(
            "bad_request",
            "`output_filename` must be a file name, not a path",
        ));
    }
    if filename
        .chars()
        .any(|ch| ch.is_control() || ch == '"' || ch == '\\')
    {
        return Err(RestError::bad_request(
            "bad_request",
            "`output_filename` contains unsupported characters",
        ));
    }
    Ok(filename)
}

fn optional_trimmed_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn bytes_response(
    bytes: Vec<u8>,
    media_type: &str,
    filename: &str,
    job_id: &JobId,
    file_id: &str,
) -> Result<Response, RestError> {
    let mut response = Body::from(bytes).into_response();
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_str(media_type).map_err(|error| {
            RestError::internal(
                "image_generation_result_failed",
                format!("invalid media type: {error}"),
            )
        })?,
    );
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{}\"", filename)).map_err(
            |error| {
                RestError::internal(
                    "image_generation_result_failed",
                    format!("invalid result filename header: {error}"),
                )
            },
        )?,
    );
    headers.insert(
        "x-tentgent-job-id",
        HeaderValue::from_str(job_id.as_str()).expect("job id header"),
    );
    headers.insert(
        "x-tentgent-file-id",
        HeaderValue::from_str(file_id).expect("file id header"),
    );
    Ok(response)
}
