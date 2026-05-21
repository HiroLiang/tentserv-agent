use std::{
    fs,
    path::{Path as StdPath, PathBuf},
};

use axum::{
    body::Body,
    extract::{multipart::MultipartRejection, Multipart, Path, Query, State},
    http::{
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
        HeaderValue, StatusCode,
    },
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tentgent_kernel::{
    features::{
        job::{
            domain::{JobResultFile, JobWorkspaceStreamSummary},
            infra::FileJobWorkspaceStore,
            ports::{
                JobChunkCursor, JobChunkPort, JobChunkWrite, JobResultPort, JobStreamKind,
                JobWorkspacePort,
            },
        },
        model::{
            domain::ModelRefSelector,
            usecases::{ModelCatalogReadUseCase, ModelListRequest},
        },
        runtime::domain::PythonRuntimeResolutionInput,
        video_understanding::{
            domain::{
                VideoSamplingOptions, VideoUnderstandingGenerationOptions,
                VideoUnderstandingOutputFormat, VideoUnderstandingResponse,
            },
            usecases::{VideoUnderstandingPreparationRequest, VideoUnderstandingUseCase},
        },
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
            media_upload_stream_limit_exceeded, video_upload_max_bytes,
            video_upload_too_large_message,
        },
        state::RestState,
    },
};

const DEFAULT_RESULT_MAX_CHUNKS: usize = 32;
const MAX_RESULT_CHUNKS: usize = 256;
const MAX_UPLOAD_METADATA_FIELD_BYTES: usize = 8 * 1024;

pub async fn create_understanding_job_from_upload(
    State(state): State<RestState>,
    multipart: Result<Multipart, MultipartRejection>,
) -> Result<(StatusCode, Json<JobResponse>), RestError> {
    let multipart = multipart.map_err(|error| {
        RestError::bad_request("bad_request", format!("invalid multipart request: {error}"))
    })?;
    let job = state.app().jobs().create(
        JobKind::video_understanding(),
        "understand uploaded video",
        Some(JobTarget::new("video")),
        Vec::<String>::new(),
    );
    let job_id = job.job_id.clone();
    let registry = state.app().jobs().clone();
    let store = FileJobWorkspaceStore::from_runtime_dir(state.app().layout().runtime_dir.clone());
    let workspace = match store.open_workspace(&job_id) {
        Ok(workspace) => workspace,
        Err(error) => {
            registry.fail(&job_id, format!("video upload workspace failed: {error}"));
            return Err(RestError::kernel("video_upload_failed", error));
        }
    };
    registry.update_progress(
        &job_id,
        JobProgressUpdate {
            stage: Some("receiving video input".to_string()),
            output: vec![JobOutputLine::new(
                JobStream::Event,
                "receiving video upload",
            )],
            ..JobProgressUpdate::default()
        },
    );

    let request = match parse_uploaded_understanding_request(
        &state,
        &job_id,
        &workspace.workspace_dir,
        multipart,
    )
    .await
    {
        Ok(request) => request,
        Err(error) => {
            registry.fail(&job_id, error.summary);
            return Err(error.error);
        }
    };

    registry.update_target(
        &job_id,
        JobTarget::new("video")
            .with_reference(request.model_label.clone())
            .with_path(request.input_path.display().to_string()),
    );
    spawn_video_understanding_worker(state.clone(), job_id.clone(), request);

    let job = state.app().jobs().get(&job_id).unwrap_or(job);
    Ok((
        StatusCode::ACCEPTED,
        Json(JobResponse { job: job_item(job) }),
    ))
}

pub async fn understanding_job_result(
    State(state): State<RestState>,
    Path(job_id): Path<String>,
    Query(query): Query<VideoUnderstandingResultQuery>,
) -> Result<Response, RestError> {
    let job_id = JobId::new(job_id);
    let Some(job) = state.app().jobs().get(&job_id) else {
        return Err(RestError::not_found(
            "not_found",
            format!("job `{job_id}` was not found"),
        ));
    };
    if job.kind.as_str() != JobKind::VIDEO_UNDERSTANDING {
        return Err(RestError::conflict(
            "wrong_job_kind",
            format!("job `{job_id}` is not a video understanding job"),
        ));
    }

    let store = FileJobWorkspaceStore::from_runtime_dir(state.app().layout().runtime_dir.clone());
    let result_files = store
        .list_result_files(&job_id)
        .map_err(|error| RestError::kernel("video_result_failed", error))?;
    let result_file = result_files.files.first();
    let read = store
        .read_chunks(
            &job_id,
            JobStreamKind::Result,
            JobChunkCursor {
                next_index: query.cursor.unwrap_or(0),
            },
            query
                .max_chunks
                .unwrap_or(DEFAULT_RESULT_MAX_CHUNKS)
                .clamp(1, MAX_RESULT_CHUNKS),
        )
        .map_err(|error| RestError::kernel("video_result_failed", error))?;

    if read.chunks_read == 0 && !read.done {
        if job.status.is_terminal() {
            match job.status {
                JobStatus::Failed => {
                    return Err(RestError::conflict(
                        "job_failed",
                        format!(
                            "video understanding job `{job_id}` failed before producing a result; inspect `/v1/jobs/{job_id}` for details"
                        ),
                    ));
                }
                JobStatus::Interrupted => {
                    return Err(RestError::conflict(
                        "job_interrupted",
                        format!(
                            "video understanding job `{job_id}` was interrupted before producing a result"
                        ),
                    ));
                }
                JobStatus::Canceled => {
                    return Err(RestError::conflict(
                        "job_canceled",
                        format!(
                            "video understanding job `{job_id}` was canceled before producing a result"
                        ),
                    ));
                }
                JobStatus::Succeeded => {}
                JobStatus::Queued | JobStatus::Running => {}
            }
            return Err(RestError::not_found(
                "result_not_found",
                format!("video understanding result for job `{job_id}` was not found"),
            ));
        }
        return Err(RestError::conflict(
            "result_pending",
            format!("video understanding result for job `{job_id}` is not ready yet"),
        ));
    }

    let media_type = result_file
        .and_then(|file| file.media_type.as_deref())
        .unwrap_or("application/octet-stream");
    let filename = result_file
        .map(|file| file.filename.as_str())
        .unwrap_or("video-understanding.txt");
    bytes_response(
        read.bytes,
        media_type,
        filename,
        read.next_cursor.next_index,
        read.done,
        read.chunks_read,
    )
}

#[derive(Debug, Deserialize)]
pub struct VideoUnderstandingResultQuery {
    pub cursor: Option<u64>,
    pub max_chunks: Option<usize>,
}

#[derive(Debug)]
struct ParsedVideoUnderstandingJobRequest {
    model_label: String,
    model_selector: ModelRefSelector,
    input_path: PathBuf,
    input_original_filename: Option<String>,
    input_media_type: String,
    input_chunk_count: u64,
    input_state: String,
    prompt: String,
    system_prompt: Option<String>,
    output_format: VideoUnderstandingOutputFormat,
    output_filename: String,
    options: VideoUnderstandingGenerationOptions,
    sampling: VideoSamplingOptions,
}

#[derive(Debug, Default)]
struct UploadVideoUnderstandingFields {
    model_ref: Option<String>,
    prompt: Option<String>,
    system_prompt: Option<String>,
    output_format: Option<String>,
    output_filename: Option<String>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    sample_fps: Option<f32>,
    max_frames: Option<u32>,
    max_frame_edge: Option<u32>,
    clip_start_seconds: Option<f32>,
    clip_duration_seconds: Option<f32>,
    file: Option<UploadedVideoFile>,
}

#[derive(Debug)]
struct UploadedVideoFile {
    path: PathBuf,
    original_filename: Option<String>,
    media_type: String,
    chunk_count: u64,
}

struct VideoUploadError {
    error: RestError,
    summary: String,
}

impl VideoUploadError {
    fn bad_request(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            error: RestError::bad_request("bad_request", message.clone()),
            summary: message,
        }
    }

    fn payload_too_large(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            error: RestError::payload_too_large("video_upload_too_large", message.clone()),
            summary: message,
        }
    }

    fn internal(code: &'static str, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            error: RestError::internal(code, message.clone()),
            summary: message,
        }
    }
}

async fn parse_uploaded_understanding_request(
    state: &RestState,
    job_id: &JobId,
    workspace_dir: &StdPath,
    mut multipart: Multipart,
) -> Result<ParsedVideoUnderstandingJobRequest, VideoUploadError> {
    let mut fields = UploadVideoUnderstandingFields::default();
    let max_upload_bytes = video_upload_max_bytes();

    while let Some(field) = multipart.next_field().await.map_err(|error| {
        let message = error.to_string();
        if media_upload_stream_limit_exceeded(&message) {
            VideoUploadError::payload_too_large(video_upload_too_large_message(
                "request body",
                max_upload_bytes,
            ))
        } else {
            VideoUploadError::bad_request(format!("invalid multipart request: {message}"))
        }
    })? {
        let name = field
            .name()
            .ok_or_else(|| VideoUploadError::bad_request("multipart field is missing a name"))?
            .to_string();
        match name.as_str() {
            "file" => {
                if fields.file.is_some() {
                    return Err(VideoUploadError::bad_request(
                        "`file` must appear exactly once",
                    ));
                }
                fields.file =
                    Some(write_uploaded_video_file(workspace_dir, field, max_upload_bytes).await?);
            }
            "model_ref" => set_text_field(&mut fields.model_ref, "model_ref", field).await?,
            "prompt" => set_text_field(&mut fields.prompt, "prompt", field).await?,
            "system_prompt" => {
                set_text_field(&mut fields.system_prompt, "system_prompt", field).await?;
            }
            "output_format" => {
                set_text_field(&mut fields.output_format, "output_format", field).await?;
            }
            "output_filename" => {
                set_text_field(&mut fields.output_filename, "output_filename", field).await?;
            }
            "max_tokens" => {
                let value = read_text_field("max_tokens", field).await?;
                set_u32_field(&mut fields.max_tokens, "max_tokens", &value)?;
            }
            "temperature" => {
                let value = read_text_field("temperature", field).await?;
                set_f32_field(&mut fields.temperature, "temperature", &value)?;
            }
            "sample_fps" => {
                let value = read_text_field("sample_fps", field).await?;
                set_f32_field(&mut fields.sample_fps, "sample_fps", &value)?;
            }
            "max_frames" => {
                let value = read_text_field("max_frames", field).await?;
                set_u32_field(&mut fields.max_frames, "max_frames", &value)?;
            }
            "max_frame_edge" => {
                let value = read_text_field("max_frame_edge", field).await?;
                set_u32_field(&mut fields.max_frame_edge, "max_frame_edge", &value)?;
            }
            "clip_start_seconds" => {
                let value = read_text_field("clip_start_seconds", field).await?;
                set_f32_field(&mut fields.clip_start_seconds, "clip_start_seconds", &value)?;
            }
            "clip_duration_seconds" => {
                let value = read_text_field("clip_duration_seconds", field).await?;
                set_f32_field(
                    &mut fields.clip_duration_seconds,
                    "clip_duration_seconds",
                    &value,
                )?;
            }
            _ => {
                return Err(VideoUploadError::bad_request(format!(
                    "unsupported video understanding multipart field `{name}`"
                )));
            }
        }
    }

    let model_label = optional_trimmed_string(fields.model_ref)
        .ok_or_else(|| VideoUploadError::bad_request("`model_ref` is required"))?;
    let model_selector = model_selector(state, &model_label).map_err(|error| VideoUploadError {
        error,
        summary: format!("video model `{model_label}` could not be resolved"),
    })?;
    let file = fields
        .file
        .ok_or_else(|| VideoUploadError::bad_request("`file` is required"))?;
    let prompt = optional_trimmed_string(fields.prompt)
        .ok_or_else(|| VideoUploadError::bad_request("`prompt` is required"))?;
    let output_format = fields
        .output_format
        .as_deref()
        .unwrap_or(VideoUnderstandingOutputFormat::Text.as_str())
        .parse::<VideoUnderstandingOutputFormat>()
        .map_err(|error| VideoUploadError::bad_request(error.to_string()))?;
    let output_filename =
        result_filename(fields.output_filename, output_format).map_err(|error| {
            VideoUploadError {
                error,
                summary: "invalid video understanding output filename".to_string(),
            }
        })?;
    let sampling = VideoSamplingOptions {
        sample_fps: fields.sample_fps,
        max_frames: fields.max_frames,
        max_frame_edge: fields.max_frame_edge,
        clip_start_seconds: fields.clip_start_seconds,
        clip_duration_seconds: fields.clip_duration_seconds,
    };
    sampling
        .validate()
        .map_err(|error| VideoUploadError::bad_request(error.to_string()))?;
    let request = ParsedVideoUnderstandingJobRequest {
        model_label,
        model_selector,
        input_path: file.path,
        input_original_filename: file.original_filename,
        input_media_type: file.media_type,
        input_chunk_count: file.chunk_count,
        input_state: "done".to_string(),
        prompt,
        system_prompt: optional_trimmed_string(fields.system_prompt),
        output_format,
        output_filename,
        options: VideoUnderstandingGenerationOptions {
            max_tokens: fields.max_tokens,
            temperature: fields.temperature,
        },
        sampling,
    };

    let store = FileJobWorkspaceStore::from_runtime_dir(state.app().layout().runtime_dir.clone());
    let workspace = store
        .finalize_stream(
            job_id,
            JobStreamKind::Input,
            input_stream_summary(&request).map_err(|error| {
                VideoUploadError::internal(
                    "video_upload_failed",
                    format!(
                        "failed to inspect uploaded video input `{}`: {error}",
                        request.input_path.display()
                    ),
                )
            })?,
        )
        .map_err(|error| VideoUploadError::internal("video_upload_failed", error.to_string()))?;
    state.app().jobs().update_workspace(job_id, workspace);

    Ok(request)
}

async fn set_text_field(
    slot: &mut Option<String>,
    name: &'static str,
    field: axum::extract::multipart::Field<'_>,
) -> Result<(), VideoUploadError> {
    if slot.is_some() {
        return Err(VideoUploadError::bad_request(format!(
            "`{name}` must not be provided more than once"
        )));
    }
    *slot = Some(read_text_field(name, field).await?);
    Ok(())
}

fn set_u32_field(
    slot: &mut Option<u32>,
    name: &'static str,
    value: &str,
) -> Result<(), VideoUploadError> {
    if slot.is_some() {
        return Err(VideoUploadError::bad_request(format!(
            "`{name}` must not be provided more than once"
        )));
    }
    let value = value.trim().parse::<u32>().map_err(|error| {
        VideoUploadError::bad_request(format!("`{name}` must be an unsigned integer: {error}"))
    })?;
    *slot = Some(value);
    Ok(())
}

fn set_f32_field(
    slot: &mut Option<f32>,
    name: &'static str,
    value: &str,
) -> Result<(), VideoUploadError> {
    if slot.is_some() {
        return Err(VideoUploadError::bad_request(format!(
            "`{name}` must not be provided more than once"
        )));
    }
    let value = value.trim().parse::<f32>().map_err(|error| {
        VideoUploadError::bad_request(format!("`{name}` must be a number: {error}"))
    })?;
    *slot = Some(value);
    Ok(())
}

async fn read_text_field(
    name: &'static str,
    mut field: axum::extract::multipart::Field<'_>,
) -> Result<String, VideoUploadError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = field.chunk().await.map_err(|error| {
        VideoUploadError::bad_request(format!("invalid `{name}` field: {error}"))
    })? {
        let next_len = bytes.len().saturating_add(chunk.len());
        if next_len > MAX_UPLOAD_METADATA_FIELD_BYTES {
            return Err(VideoUploadError::bad_request(format!(
                "`{name}` must be at most {MAX_UPLOAD_METADATA_FIELD_BYTES} bytes"
            )));
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|error| {
        VideoUploadError::bad_request(format!("`{name}` must be valid UTF-8: {error}"))
    })
}

async fn write_uploaded_video_file(
    workspace_dir: &StdPath,
    mut field: axum::extract::multipart::Field<'_>,
    max_upload_bytes: usize,
) -> Result<UploadedVideoFile, VideoUploadError> {
    let original_filename = field.file_name().map(str::to_string);
    let media_type = field.content_type().map(str::to_string).unwrap_or_else(|| {
        original_filename
            .as_deref()
            .map(|name| video_media_type(StdPath::new(name)).to_string())
            .unwrap_or_else(|| "application/octet-stream".to_string())
    });
    let filename = safe_upload_filename(original_filename.as_deref());
    let input_dir = workspace_dir.join("input");
    tokio::fs::create_dir_all(&input_dir)
        .await
        .map_err(|error| {
            VideoUploadError::internal(
                "video_upload_failed",
                format!("create `{}` failed: {error}", input_dir.display()),
            )
        })?;
    let final_path = input_dir.join(&filename);
    let partial_path = input_dir.join(format!("{filename}.part"));
    let mut file = tokio::fs::File::create(&partial_path)
        .await
        .map_err(|error| {
            VideoUploadError::internal(
                "video_upload_failed",
                format!("create `{}` failed: {error}", partial_path.display()),
            )
        })?;
    let mut chunk_count = 0u64;
    let mut total_bytes = 0u64;

    while let Some(chunk) = field.chunk().await.map_err(|error| {
        let message = error.to_string();
        if media_upload_stream_limit_exceeded(&message) {
            VideoUploadError::payload_too_large(video_upload_too_large_message(
                "file",
                max_upload_bytes,
            ))
        } else {
            VideoUploadError::bad_request(format!("invalid `file` upload stream: {message}"))
        }
    })? {
        if chunk.is_empty() {
            continue;
        }
        total_bytes = total_bytes.saturating_add(chunk.len() as u64);
        if total_bytes > max_upload_bytes as u64 {
            let _ = tokio::fs::remove_file(&partial_path).await;
            return Err(VideoUploadError::payload_too_large(
                video_upload_too_large_message("file", max_upload_bytes),
            ));
        }
        chunk_count = chunk_count.saturating_add(1);
        file.write_all(&chunk).await.map_err(|error| {
            VideoUploadError::internal(
                "video_upload_failed",
                format!("write `{}` failed: {error}", partial_path.display()),
            )
        })?;
    }
    file.flush().await.map_err(|error| {
        VideoUploadError::internal(
            "video_upload_failed",
            format!("flush `{}` failed: {error}", partial_path.display()),
        )
    })?;
    drop(file);

    if total_bytes == 0 {
        let _ = tokio::fs::remove_file(&partial_path).await;
        return Err(VideoUploadError::bad_request("`file` must not be empty"));
    }

    tokio::fs::rename(&partial_path, &final_path)
        .await
        .map_err(|error| {
            VideoUploadError::internal(
                "video_upload_failed",
                format!(
                    "replace `{}` with `{}` failed: {error}",
                    partial_path.display(),
                    final_path.display()
                ),
            )
        })?;

    Ok(UploadedVideoFile {
        path: final_path,
        original_filename,
        media_type,
        chunk_count,
    })
}

fn spawn_video_understanding_worker(
    state: RestState,
    job_id: JobId,
    request: ParsedVideoUnderstandingJobRequest,
) {
    let registry = state.app().jobs().clone();
    let task_state = state.clone();
    let layout = state.app().layout_input(LayoutResolveMode::Create);
    let handle = tokio::runtime::Handle::current();

    state.app().job_runner().spawn_blocking(
        registry,
        job_id,
        "preparing video understanding",
        move |registry, job_id| {
            run_video_understanding_job(task_state, layout, request, handle, registry, job_id)
        },
    );
}

fn run_video_understanding_job(
    state: RestState,
    layout: RuntimeLayoutInput,
    request: ParsedVideoUnderstandingJobRequest,
    handle: tokio::runtime::Handle,
    registry: JobRegistry,
    job_id: JobId,
) -> Result<JobCompletion, String> {
    let store = FileJobWorkspaceStore::from_runtime_dir(state.app().layout().runtime_dir.clone());
    let workspace = store
        .open_workspace(&job_id)
        .map_err(|error| error.to_string())?;
    let input_summary = input_stream_summary(&request).map_err(|error| {
        format!(
            "failed to inspect video input `{}`: {error}",
            request.input_path.display()
        )
    })?;
    let workspace_summary = store
        .finalize_stream(&job_id, JobStreamKind::Input, input_summary)
        .map_err(|error| error.to_string())?;
    registry.update_workspace(&job_id, workspace_summary);
    registry.update_progress(
        &job_id,
        JobProgressUpdate {
            stage: Some("running video understanding".to_string()),
            progress: JobProgressPatch {
                files_total: Some(1),
                files_done: Some(0),
                ..JobProgressPatch::default()
            },
            output: vec![JobOutputLine::new(
                JobStream::Event,
                format!("sampling {}", request.input_path.display()),
            )],
            ..JobProgressUpdate::default()
        },
    );

    let execution = handle
        .block_on(async {
            state
                .app()
                .services()
                .kernel()
                .video_understanding_usecase()
                .understand_video(VideoUnderstandingPreparationRequest {
                    layout,
                    runtime: PythonRuntimeResolutionInput::default(),
                    model_selector: request.model_selector,
                    video_path: request.input_path.clone(),
                    video_media_type: Some(request.input_media_type.clone()),
                    prompt: request.prompt.clone(),
                    system_prompt: request.system_prompt.clone(),
                    output_format: request.output_format,
                    options: request.options.clone(),
                    sampling: request.sampling,
                })
                .await
        })
        .map_err(|error| error.to_string())?;

    let result_bytes = rendered_result_body(
        execution.prepared.model.metadata.model_ref.to_string(),
        &execution.response,
    )
    .map_err(|error| error.to_string())?;
    let files_dir = workspace.workspace_dir.join("files");
    fs::create_dir_all(&files_dir)
        .map_err(|error| format!("create `{}` failed: {error}", files_dir.display()))?;
    let output_path = files_dir.join(&request.output_filename);
    fs::write(&output_path, &result_bytes)
        .map_err(|error| format!("write `{}` failed: {error}", output_path.display()))?;
    store
        .write_chunk(
            &job_id,
            JobChunkWrite {
                stream: JobStreamKind::Result,
                index: 0,
                bytes: result_bytes,
            },
        )
        .and_then(|_| store.commit_chunk(&job_id, JobStreamKind::Result, 0))
        .map_err(|error| error.to_string())?;
    let total_bytes = fs::metadata(&output_path)
        .map_err(|error| format!("inspect `{}` failed: {error}", output_path.display()))?
        .len();
    let result_summary = JobWorkspaceStreamSummary {
        state: "done".to_string(),
        done: true,
        failed: false,
        chunk_count: 1,
        total_bytes,
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
                total_bytes,
            },
        )
        .map_err(|error| error.to_string())?;
    registry.update_workspace(&job_id, workspace_summary);
    registry.update_progress(
        &job_id,
        JobProgressUpdate {
            stage: Some("video understanding finished".to_string()),
            progress: JobProgressPatch {
                bytes_done: Some(total_bytes),
                bytes_total: Some(total_bytes),
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
        "video understanding wrote {}",
        request.output_filename
    ))
    .with_artifact(
        JobArtifact::new("video_understanding").with_path(output_path.display().to_string()),
    ))
}

fn input_stream_summary(
    request: &ParsedVideoUnderstandingJobRequest,
) -> Result<JobWorkspaceStreamSummary, std::io::Error> {
    let metadata = fs::metadata(&request.input_path)?;
    Ok(JobWorkspaceStreamSummary {
        state: request.input_state.clone(),
        done: true,
        failed: false,
        chunk_count: request.input_chunk_count,
        total_bytes: metadata.len(),
        sha256: None,
        media_type: Some(request.input_media_type.clone()),
        original_filename: request.input_original_filename.clone(),
    })
}

fn rendered_result_body(
    model_ref: String,
    response: &VideoUnderstandingResponse,
) -> Result<Vec<u8>, serde_json::Error> {
    if response.output_format == VideoUnderstandingOutputFormat::Json {
        let value = json!({
            "model_ref": model_ref,
            "output_format": response.output_format.as_str(),
            "text": response.text,
            "finish_reason": response.finish_reason,
            "sampled_frames": response.sampled_frames,
        });
        let mut body = serde_json::to_vec_pretty(&value)?;
        body.push(b'\n');
        return Ok(body);
    }

    let mut body = response.text.as_bytes().to_vec();
    if !body.ends_with(b"\n") {
        body.push(b'\n');
    }
    Ok(body)
}

fn result_filename(
    value: Option<String>,
    output_format: VideoUnderstandingOutputFormat,
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

fn safe_upload_filename(original_filename: Option<&str>) -> String {
    let candidate = original_filename
        .and_then(|name| StdPath::new(name).file_name())
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("video-input");
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
        "video-input".to_string()
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
            error: RestError::store_lookup("video_model_failed", error.to_string()),
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
            error: RestError::internal("video_model_failed", err.to_string()),
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

fn video_media_type(path: &StdPath) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        _ => "application/octet-stream",
    }
}

fn bytes_response(
    bytes: Vec<u8>,
    media_type: &str,
    filename: &str,
    next_cursor: u64,
    done: bool,
    chunks_read: usize,
) -> Result<Response, RestError> {
    let mut response = Body::from(bytes).into_response();
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_str(media_type).map_err(|error| {
            RestError::internal(
                "video_result_failed",
                format!("invalid media type: {error}"),
            )
        })?,
    );
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{}\"", filename)).map_err(
            |error| {
                RestError::internal(
                    "video_result_failed",
                    format!("invalid result filename header: {error}"),
                )
            },
        )?,
    );
    headers.insert(
        "x-tentgent-next-cursor",
        HeaderValue::from_str(&next_cursor.to_string()).expect("cursor header"),
    );
    headers.insert(
        "x-tentgent-result-done",
        HeaderValue::from_static(if done { "true" } else { "false" }),
    );
    headers.insert(
        "x-tentgent-chunks-read",
        HeaderValue::from_str(&chunks_read.to_string()).expect("chunks header"),
    );
    Ok(response)
}
