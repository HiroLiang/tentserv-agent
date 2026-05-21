use std::fs;
use std::path::{Path as StdPath, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    body::Body,
    extract::{multipart::MultipartRejection, Multipart, Path, State},
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
                ImageGenerationDimensions, ImageGenerationInput, ImageGenerationOptions,
                ImageGenerationOutputFormat, ImageTransformStrength,
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
use tokio::io::AsyncWriteExt;

use crate::{
    handlers::rest::jobs::{job_item, JobResponse},
    runtime::{
        JobArtifact, JobCompletion, JobId, JobKind, JobOutputLine, JobProgressPatch,
        JobProgressUpdate, JobRegistry, JobStatus, JobStream, JobTarget,
    },
    transport::rest::{
        error::RestError,
        limits::{
            media_upload_max_bytes, media_upload_stream_limit_exceeded,
            media_upload_too_large_message,
        },
        state::RestState,
    },
};

const MAX_METADATA_FIELD_BYTES: usize = 8 * 1024;

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

pub async fn create_transform_job(
    State(state): State<RestState>,
    multipart: Result<Multipart, MultipartRejection>,
) -> Result<(StatusCode, Json<JobResponse>), RestError> {
    let multipart = multipart.map_err(|error| {
        RestError::bad_request("bad_request", format!("invalid multipart request: {error}"))
    })?;
    let upload = parse_image_transform_upload(&state, multipart).await?;
    let label = "transform image".to_string();
    let job = state.app().jobs().create(
        JobKind::image_generation(),
        label,
        Some(JobTarget::new("image").with_reference(upload.request.model_label.clone())),
        Vec::<String>::new(),
    );
    let job_id = job.job_id.clone();
    let request = match persist_transform_input(&state, &job_id, upload).await {
        Ok(request) => request,
        Err(error) => {
            state
                .app()
                .jobs()
                .fail(&job_id, "image transform upload persistence failed");
            return Err(error);
        }
    };
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

pub async fn transform_job_files(
    State(state): State<RestState>,
    Path(job_id): Path<String>,
) -> Result<Json<JobResultFileList>, RestError> {
    generation_job_files(State(state), Path(job_id)).await
}

pub async fn transform_job_file(
    State(state): State<RestState>,
    Path((job_id, file_id)): Path<(String, String)>,
) -> Result<Response, RestError> {
    generation_job_file(State(state), Path((job_id, file_id))).await
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
    input: ImageGenerationInput,
    input_summary: JobWorkspaceStreamSummary,
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
            input: ImageGenerationInput::TextToImage,
            input_summary: JobWorkspaceStreamSummary {
                state: "done".to_string(),
                done: true,
                failed: false,
                chunk_count: 0,
                total_bytes: prompt.len() as u64,
                sha256: None,
                media_type: Some("text/plain".to_string()),
                original_filename: None,
            },
            prompt,
            negative_prompt: optional_trimmed_string(request.negative_prompt),
            output_format,
            output_filename,
            options,
        })
    }
}

#[derive(Debug)]
struct ParsedImageTransformUpload {
    request: ParsedImageGenerationJobRequest,
    temp_dir: PathBuf,
}

#[derive(Debug, Default)]
struct ImageTransformFields {
    image: Option<UploadedTransformImage>,
    model_ref: Option<String>,
    adapter_ref: Option<String>,
    lora_scale: Option<f32>,
    prompt: Option<String>,
    negative_prompt: Option<String>,
    strength: Option<f32>,
    output_format: Option<String>,
    output_filename: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    steps: Option<u32>,
    guidance_scale: Option<f32>,
    seed: Option<u64>,
}

#[derive(Debug)]
struct UploadedTransformImage {
    path: PathBuf,
    media_type: String,
    original_filename: Option<String>,
    total_bytes: u64,
}

async fn parse_image_transform_upload(
    state: &RestState,
    multipart: Multipart,
) -> Result<ParsedImageTransformUpload, RestError> {
    let temp_dir = state
        .app()
        .layout()
        .runtime_dir
        .join("tmp")
        .join("image-transform")
        .join(unique_suffix());
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_err(|error| {
            RestError::internal(
                "image_transform_upload_failed",
                format!("failed to create image transform upload temp dir: {error}"),
            )
        })?;
    let result = parse_image_transform_upload_in_dir(state, &temp_dir, multipart).await;
    if result.is_err() {
        cleanup_temp_dir(&temp_dir).await;
    }
    result.map(|request| ParsedImageTransformUpload { request, temp_dir })
}

async fn parse_image_transform_upload_in_dir(
    state: &RestState,
    temp_dir: &StdPath,
    mut multipart: Multipart,
) -> Result<ParsedImageGenerationJobRequest, RestError> {
    let mut fields = ImageTransformFields::default();
    let max_upload_bytes = media_upload_max_bytes();

    while let Some(field) = multipart.next_field().await.map_err(|error| {
        let message = error.to_string();
        if media_upload_stream_limit_exceeded(&message) {
            RestError::payload_too_large(
                "upload_too_large",
                media_upload_too_large_message("request body", max_upload_bytes),
            )
        } else {
            RestError::bad_request(
                "bad_request",
                format!("invalid multipart request: {message}"),
            )
        }
    })? {
        let name = field
            .name()
            .ok_or_else(|| {
                RestError::bad_request("bad_request", "multipart field is missing a name")
            })?
            .to_string();
        match name.as_str() {
            "image" => {
                if fields.image.is_some() {
                    cleanup_temp_dir(temp_dir).await;
                    return Err(RestError::bad_request(
                        "bad_request",
                        "`image` must appear exactly once",
                    ));
                }
                fields.image =
                    Some(write_uploaded_transform_image(temp_dir, field, max_upload_bytes).await?);
            }
            "model_ref" => set_text_field(&mut fields.model_ref, "model_ref", field).await?,
            "adapter_ref" => set_text_field(&mut fields.adapter_ref, "adapter_ref", field).await?,
            "prompt" => set_text_field(&mut fields.prompt, "prompt", field).await?,
            "negative_prompt" => {
                set_text_field(&mut fields.negative_prompt, "negative_prompt", field).await?
            }
            "output_format" => {
                set_text_field(&mut fields.output_format, "output_format", field).await?
            }
            "output_filename" => {
                set_text_field(&mut fields.output_filename, "output_filename", field).await?
            }
            "lora_scale" => {
                fields.lora_scale =
                    Some(parse_single_f32_field(fields.lora_scale, "lora_scale", field).await?);
            }
            "strength" => {
                fields.strength =
                    Some(parse_single_f32_field(fields.strength, "strength", field).await?);
            }
            "width" => {
                fields.width = Some(parse_single_u32_field(fields.width, "width", field).await?);
            }
            "height" => {
                fields.height = Some(parse_single_u32_field(fields.height, "height", field).await?);
            }
            "steps" => {
                fields.steps = Some(parse_single_u32_field(fields.steps, "steps", field).await?);
            }
            "guidance_scale" => {
                fields.guidance_scale = Some(
                    parse_single_f32_field(fields.guidance_scale, "guidance_scale", field).await?,
                );
            }
            "seed" => {
                fields.seed = Some(parse_single_u64_field(fields.seed, "seed", field).await?);
            }
            _ => {
                cleanup_temp_dir(temp_dir).await;
                return Err(RestError::bad_request(
                    "bad_request",
                    format!("unsupported image transform multipart field `{name}`"),
                ));
            }
        }
    }

    let image = fields
        .image
        .ok_or_else(|| RestError::bad_request("bad_request", "`image` is required"))?;
    let model_label = optional_trimmed_string(fields.model_ref)
        .ok_or_else(|| RestError::bad_request("bad_request", "`model_ref` is required"))?;
    let model_selector = model_selector(state, &model_label)?;
    let adapter_selector = optional_trimmed_string(fields.adapter_ref)
        .map(|value| {
            AdapterRefSelector::parse(value.as_str()).map_err(|error| {
                RestError::bad_request("bad_request", format!("invalid `adapter_ref`: {error}"))
            })
        })
        .transpose()?;
    let lora_scale = fields
        .lora_scale
        .map(|value| {
            LoraScale::new(value)
                .map_err(|error| RestError::bad_request("bad_request", error.to_string()))
        })
        .transpose()?;
    let prompt = optional_trimmed_string(fields.prompt)
        .ok_or_else(|| RestError::bad_request("bad_request", "`prompt` is required"))?;
    let output_format = fields
        .output_format
        .as_deref()
        .unwrap_or(ImageGenerationOutputFormat::Png.as_str())
        .parse::<ImageGenerationOutputFormat>()
        .map_err(|error| RestError::bad_request("bad_request", error.to_string()))?;
    let output_filename = result_filename(fields.output_filename, output_format)?;
    let width = fields
        .width
        .unwrap_or(ImageGenerationDimensions::DEFAULT_WIDTH);
    let height = fields
        .height
        .unwrap_or(ImageGenerationDimensions::DEFAULT_HEIGHT);
    let dimensions = ImageGenerationDimensions::new(width, height)
        .map_err(|error| RestError::bad_request("bad_request", error.to_string()))?;
    let options = ImageGenerationOptions::new(
        dimensions,
        fields
            .steps
            .unwrap_or(ImageGenerationOptions::DEFAULT_STEPS),
        fields
            .guidance_scale
            .unwrap_or(ImageGenerationOptions::DEFAULT_GUIDANCE_SCALE),
        fields.seed,
    )
    .map_err(|error| RestError::bad_request("bad_request", error.to_string()))?;
    let strength =
        ImageTransformStrength::new(fields.strength.unwrap_or(ImageTransformStrength::DEFAULT))
            .map_err(|error| RestError::bad_request("bad_request", error.to_string()))?;

    Ok(ParsedImageGenerationJobRequest {
        model_label,
        model_selector,
        adapter_selector,
        lora_scale,
        input: ImageGenerationInput::ImageToImage {
            image_path: image.path,
            media_type: Some(image.media_type.clone()),
            strength,
        },
        input_summary: JobWorkspaceStreamSummary {
            state: "done".to_string(),
            done: true,
            failed: false,
            chunk_count: 1,
            total_bytes: image.total_bytes,
            sha256: None,
            media_type: Some(image.media_type),
            original_filename: image.original_filename,
        },
        prompt,
        negative_prompt: optional_trimmed_string(fields.negative_prompt),
        output_format,
        output_filename,
        options,
    })
}

async fn persist_transform_input(
    state: &RestState,
    job_id: &JobId,
    upload: ParsedImageTransformUpload,
) -> Result<ParsedImageGenerationJobRequest, RestError> {
    let store = FileJobWorkspaceStore::from_runtime_dir(state.app().layout().runtime_dir.clone());
    let workspace = store
        .open_workspace(job_id)
        .map_err(|error| RestError::kernel("image_transform_workspace_failed", error))?;
    let mut request = upload.request;
    let ImageGenerationInput::ImageToImage { image_path, .. } = &mut request.input else {
        cleanup_temp_dir(&upload.temp_dir).await;
        return Ok(request);
    };
    let filename = image_path
        .file_name()
        .and_then(|value| value.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| "image-input".to_string());
    let input_dir = workspace.workspace_dir.join("input");
    tokio::fs::create_dir_all(&input_dir)
        .await
        .map_err(|error| {
            RestError::internal(
                "image_transform_workspace_failed",
                format!("failed to create image transform input dir: {error}"),
            )
        })?;
    let final_path = input_dir.join(filename);
    tokio::fs::rename(&image_path, &final_path)
        .await
        .map_err(|error| {
            RestError::internal(
                "image_transform_workspace_failed",
                format!(
                    "failed to move uploaded image `{}` into job workspace `{}`: {error}",
                    image_path.display(),
                    final_path.display()
                ),
            )
        })?;
    *image_path = final_path;
    cleanup_temp_dir(&upload.temp_dir).await;
    Ok(request)
}

async fn write_uploaded_transform_image(
    temp_dir: &StdPath,
    mut field: axum::extract::multipart::Field<'_>,
    max_upload_bytes: usize,
) -> Result<UploadedTransformImage, RestError> {
    let original_filename = field.file_name().map(str::to_string);
    let media_type = image_media_type(field.content_type(), original_filename.as_deref())?;
    let filename = safe_upload_filename(original_filename.as_deref(), "image-input");
    let final_path = temp_dir.join(&filename);
    let partial_path = temp_dir.join(format!("{filename}.part"));
    let mut file = tokio::fs::File::create(&partial_path)
        .await
        .map_err(|error| {
            RestError::internal(
                "image_transform_upload_failed",
                format!("create `{}` failed: {error}", partial_path.display()),
            )
        })?;
    let mut total_bytes = 0u64;

    while let Some(chunk) = field.chunk().await.map_err(|error| {
        let message = error.to_string();
        if media_upload_stream_limit_exceeded(&message) {
            RestError::payload_too_large(
                "upload_too_large",
                media_upload_too_large_message("image", max_upload_bytes),
            )
        } else {
            RestError::bad_request(
                "bad_request",
                format!("invalid `image` upload stream: {message}"),
            )
        }
    })? {
        if chunk.is_empty() {
            continue;
        }
        total_bytes = total_bytes.saturating_add(chunk.len() as u64);
        if total_bytes > max_upload_bytes as u64 {
            let _ = tokio::fs::remove_file(&partial_path).await;
            return Err(RestError::payload_too_large(
                "upload_too_large",
                media_upload_too_large_message("image", max_upload_bytes),
            ));
        }
        file.write_all(&chunk).await.map_err(|error| {
            RestError::internal(
                "image_transform_upload_failed",
                format!("write `{}` failed: {error}", partial_path.display()),
            )
        })?;
    }
    file.flush().await.map_err(|error| {
        RestError::internal(
            "image_transform_upload_failed",
            format!("flush `{}` failed: {error}", partial_path.display()),
        )
    })?;
    drop(file);

    if total_bytes == 0 {
        let _ = tokio::fs::remove_file(&partial_path).await;
        return Err(RestError::bad_request(
            "bad_request",
            "`image` must not be empty",
        ));
    }

    tokio::fs::rename(&partial_path, &final_path)
        .await
        .map_err(|error| {
            RestError::internal(
                "image_transform_upload_failed",
                format!(
                    "replace `{}` with `{}` failed: {error}",
                    partial_path.display(),
                    final_path.display()
                ),
            )
        })?;

    Ok(UploadedTransformImage {
        path: final_path,
        media_type,
        original_filename,
        total_bytes,
    })
}

async fn set_text_field(
    slot: &mut Option<String>,
    name: &'static str,
    field: axum::extract::multipart::Field<'_>,
) -> Result<(), RestError> {
    if slot.is_some() {
        return Err(RestError::bad_request(
            "bad_request",
            format!("`{name}` must not be provided more than once"),
        ));
    }
    *slot = Some(read_text_field(name, field).await?);
    Ok(())
}

async fn read_text_field(
    name: &'static str,
    mut field: axum::extract::multipart::Field<'_>,
) -> Result<String, RestError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = field.chunk().await.map_err(|error| {
        RestError::bad_request("bad_request", format!("invalid `{name}` field: {error}"))
    })? {
        let next_len = bytes.len().saturating_add(chunk.len());
        if next_len > MAX_METADATA_FIELD_BYTES {
            return Err(RestError::bad_request(
                "bad_request",
                format!("`{name}` must be at most {MAX_METADATA_FIELD_BYTES} bytes"),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|error| {
        RestError::bad_request(
            "bad_request",
            format!("`{name}` must be valid UTF-8: {error}"),
        )
    })
}

async fn parse_single_f32_field(
    existing: Option<f32>,
    name: &'static str,
    field: axum::extract::multipart::Field<'_>,
) -> Result<f32, RestError> {
    if existing.is_some() {
        return Err(RestError::bad_request(
            "bad_request",
            format!("`{name}` must not be provided more than once"),
        ));
    }
    parse_f32_field(name, &read_text_field(name, field).await?)
}

async fn parse_single_u32_field(
    existing: Option<u32>,
    name: &'static str,
    field: axum::extract::multipart::Field<'_>,
) -> Result<u32, RestError> {
    if existing.is_some() {
        return Err(RestError::bad_request(
            "bad_request",
            format!("`{name}` must not be provided more than once"),
        ));
    }
    parse_u32_field(name, &read_text_field(name, field).await?)
}

async fn parse_single_u64_field(
    existing: Option<u64>,
    name: &'static str,
    field: axum::extract::multipart::Field<'_>,
) -> Result<u64, RestError> {
    if existing.is_some() {
        return Err(RestError::bad_request(
            "bad_request",
            format!("`{name}` must not be provided more than once"),
        ));
    }
    read_text_field(name, field)
        .await?
        .trim()
        .parse::<u64>()
        .map_err(|error| {
            RestError::bad_request(
                "bad_request",
                format!("`{name}` must be an unsigned integer: {error}"),
            )
        })
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
        .finalize_stream(&job_id, JobStreamKind::Input, request.input_summary.clone())
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
                    input: request.input,
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

fn parse_u32_field(name: &'static str, value: &str) -> Result<u32, RestError> {
    value.trim().parse::<u32>().map_err(|error| {
        RestError::bad_request(
            "bad_request",
            format!("`{name}` must be an unsigned integer: {error}"),
        )
    })
}

fn parse_f32_field(name: &'static str, value: &str) -> Result<f32, RestError> {
    let parsed = value.trim().parse::<f32>().map_err(|error| {
        RestError::bad_request("bad_request", format!("`{name}` must be a float: {error}"))
    })?;
    if !parsed.is_finite() {
        return Err(RestError::bad_request(
            "bad_request",
            format!("`{name}` must be a finite float"),
        ));
    }
    Ok(parsed)
}

fn image_media_type(
    content_type: Option<&str>,
    original_filename: Option<&str>,
) -> Result<String, RestError> {
    if let Some(content_type) = content_type {
        let normalized = content_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if is_supported_image_media_type(&normalized) {
            return Ok(normalized);
        }
    }

    original_filename
        .map(StdPath::new)
        .and_then(|path| image_media_type_from_extension(path).map(str::to_string))
        .ok_or_else(|| {
            RestError::bad_request(
                "bad_request",
                "`image` must be image/png, image/jpeg, or image/webp",
            )
        })
}

fn image_media_type_from_extension(path: &StdPath) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

fn is_supported_image_media_type(value: &str) -> bool {
    matches!(value, "image/png" | "image/jpeg" | "image/webp")
}

fn safe_upload_filename(original_filename: Option<&str>, fallback: &str) -> String {
    let candidate = original_filename
        .and_then(|name| StdPath::new(name).file_name())
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback);
    let sanitized = candidate
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let sanitized = sanitized.trim_matches('.').trim_matches('_');
    if sanitized.is_empty() {
        fallback.to_string()
    } else {
        sanitized.to_string()
    }
}

fn optional_trimmed_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

async fn cleanup_temp_dir(path: &StdPath) {
    let _ = tokio::fs::remove_dir_all(path).await;
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{}-{nanos}", std::process::id())
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
