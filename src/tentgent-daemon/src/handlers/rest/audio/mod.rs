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
use tentgent_kernel::{
    features::{
        audio::{
            domain::AudioTranscriptionOutputFormat,
            usecases::{AudioTranscriptionPreparationRequest, AudioTranscriptionUseCase},
        },
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
    },
    foundation::layout::{LayoutResolveMode, RuntimeLayoutInput},
};

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
use tokio::io::AsyncWriteExt;

mod speech;

pub use speech::{create_speech_job, speech_job_result};

const DEFAULT_RESULT_MAX_CHUNKS: usize = 32;
const MAX_RESULT_CHUNKS: usize = 256;
const MAX_UPLOAD_METADATA_FIELD_BYTES: usize = 8 * 1024;

pub async fn create_transcription_job_from_upload(
    State(state): State<RestState>,
    multipart: Result<Multipart, MultipartRejection>,
) -> Result<(StatusCode, Json<JobResponse>), RestError> {
    let multipart = multipart.map_err(|error| {
        RestError::bad_request("bad_request", format!("invalid multipart request: {error}"))
    })?;
    let job = state.app().jobs().create(
        JobKind::audio_transcription(),
        "transcribe uploaded audio",
        Some(JobTarget::new("audio")),
        Vec::<String>::new(),
    );
    let job_id = job.job_id.clone();
    let registry = state.app().jobs().clone();
    let store = FileJobWorkspaceStore::from_runtime_dir(state.app().layout().runtime_dir.clone());
    let workspace = match store.open_workspace(&job_id) {
        Ok(workspace) => workspace,
        Err(error) => {
            registry.fail(&job_id, format!("audio upload workspace failed: {error}"));
            return Err(RestError::kernel("audio_upload_failed", error));
        }
    };
    registry.update_progress(
        &job_id,
        JobProgressUpdate {
            stage: Some("receiving audio input".to_string()),
            output: vec![JobOutputLine::new(
                JobStream::Event,
                "receiving audio upload",
            )],
            ..JobProgressUpdate::default()
        },
    );

    let request = match parse_uploaded_transcription_request(
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
        JobTarget::new("audio")
            .with_reference(request.model_label.clone())
            .with_path(request.input_path.display().to_string()),
    );
    spawn_transcription_worker(state.clone(), job_id.clone(), request);

    let job = state.app().jobs().get(&job_id).unwrap_or(job);
    Ok((
        StatusCode::ACCEPTED,
        Json(JobResponse { job: job_item(job) }),
    ))
}

pub async fn create_transcription_job(
    State(state): State<RestState>,
    Json(request): Json<AudioTranscriptionJobRequest>,
) -> Result<(StatusCode, Json<JobResponse>), RestError> {
    let request = ParsedAudioTranscriptionJobRequest::from_request(&state, request)?;
    let label = format!(
        "transcribe {}",
        request
            .input_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("audio")
    );
    let job = state.app().jobs().create(
        JobKind::audio_transcription(),
        label,
        Some(
            JobTarget::new("audio")
                .with_reference(request.model_label.clone())
                .with_path(request.input_path.display().to_string()),
        ),
        Vec::<String>::new(),
    );
    let job_id = job.job_id.clone();
    spawn_transcription_worker(state, job_id, request);

    Ok((
        StatusCode::ACCEPTED,
        Json(JobResponse { job: job_item(job) }),
    ))
}

fn spawn_transcription_worker(
    state: RestState,
    job_id: JobId,
    request: ParsedAudioTranscriptionJobRequest,
) {
    let registry = state.app().jobs().clone();
    let task_state = state.clone();
    let layout = state.app().layout_input(LayoutResolveMode::Create);
    let handle = tokio::runtime::Handle::current();

    state.app().job_runner().spawn_blocking(
        registry,
        job_id,
        "preparing audio transcription",
        move |registry, job_id| {
            run_audio_transcription_job(task_state, layout, request, handle, registry, job_id)
        },
    );
}

pub async fn transcription_job_result(
    State(state): State<RestState>,
    Path(job_id): Path<String>,
    Query(query): Query<AudioTranscriptionResultQuery>,
) -> Result<Response, RestError> {
    let job_id = JobId::new(job_id);
    let Some(job) = state.app().jobs().get(&job_id) else {
        return Err(RestError::not_found(
            "not_found",
            format!("job `{job_id}` was not found"),
        ));
    };
    if job.kind.as_str() != JobKind::AUDIO_TRANSCRIPTION {
        return Err(RestError::conflict(
            "wrong_job_kind",
            format!("job `{job_id}` is not an audio transcription job"),
        ));
    }

    let store = FileJobWorkspaceStore::from_runtime_dir(state.app().layout().runtime_dir.clone());
    let result_files = store
        .list_result_files(&job_id)
        .map_err(|error| RestError::kernel("audio_result_failed", error))?;
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
        .map_err(|error| RestError::kernel("audio_result_failed", error))?;

    if read.chunks_read == 0 && !read.done {
        if job.status.is_terminal() {
            match job.status {
                JobStatus::Failed => {
                    return Err(RestError::conflict(
                        "job_failed",
                        format!(
                            "audio transcription job `{job_id}` failed before producing a result; inspect `/v1/jobs/{job_id}` for details"
                        ),
                    ));
                }
                JobStatus::Interrupted => {
                    return Err(RestError::conflict(
                        "job_interrupted",
                        format!(
                            "audio transcription job `{job_id}` was interrupted before producing a result"
                        ),
                    ));
                }
                JobStatus::Canceled => {
                    return Err(RestError::conflict(
                        "job_canceled",
                        format!(
                            "audio transcription job `{job_id}` was canceled before producing a result"
                        ),
                    ));
                }
                JobStatus::Succeeded => {}
                JobStatus::Queued | JobStatus::Running => {}
            }
            return Err(RestError::not_found(
                "result_not_found",
                format!("audio transcription result for job `{job_id}` was not found"),
            ));
        }
        return Err(RestError::conflict(
            "result_pending",
            format!("audio transcription result for job `{job_id}` is not ready yet"),
        ));
    }

    let media_type = result_file
        .and_then(|file| file.media_type.as_deref())
        .unwrap_or("application/octet-stream");
    let filename = result_file
        .map(|file| file.filename.as_str())
        .unwrap_or("transcript.txt");
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
#[serde(deny_unknown_fields)]
pub struct AudioTranscriptionJobRequest {
    pub model_ref: String,
    pub path: String,
    pub language: Option<String>,
    pub output_format: Option<String>,
    pub output_filename: Option<String>,
    pub timestamps: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct AudioTranscriptionResultQuery {
    pub cursor: Option<u64>,
    pub max_chunks: Option<usize>,
}

#[derive(Debug)]
struct ParsedAudioTranscriptionJobRequest {
    model_label: String,
    model_selector: ModelRefSelector,
    input_path: PathBuf,
    input_original_filename: Option<String>,
    input_media_type: String,
    input_chunk_count: u64,
    input_state: String,
    output_format: AudioTranscriptionOutputFormat,
    output_filename: String,
    language: Option<String>,
    timestamps: bool,
}

impl ParsedAudioTranscriptionJobRequest {
    fn from_request(
        state: &RestState,
        request: AudioTranscriptionJobRequest,
    ) -> Result<Self, RestError> {
        let model_label = request.model_ref.trim().to_string();
        let model_selector = model_selector(state, &model_label)?;
        let input_path = canonical_audio_input_path(&request.path)?;
        let output_format = request
            .output_format
            .as_deref()
            .unwrap_or(AudioTranscriptionOutputFormat::Text.as_str())
            .parse::<AudioTranscriptionOutputFormat>()
            .map_err(|error| RestError::bad_request("bad_request", error.to_string()))?;
        let output_filename = result_filename(request.output_filename, output_format)?;
        let language = optional_trimmed_string(request.language);
        let input_original_filename = input_path
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::to_string);
        let input_media_type = audio_media_type(&input_path).to_string();

        Ok(Self {
            model_label,
            model_selector,
            input_path,
            input_original_filename,
            input_media_type,
            input_chunk_count: 0,
            input_state: "path".to_string(),
            output_format,
            output_filename,
            language,
            timestamps: request.timestamps.unwrap_or(false),
        })
    }
}

#[derive(Debug, Default)]
struct UploadTranscriptionFields {
    model_ref: Option<String>,
    language: Option<String>,
    output_format: Option<String>,
    output_filename: Option<String>,
    timestamps: Option<bool>,
    file: Option<UploadedAudioFile>,
}

#[derive(Debug)]
struct UploadedAudioFile {
    path: PathBuf,
    original_filename: Option<String>,
    media_type: String,
    chunk_count: u64,
}

struct AudioUploadError {
    error: RestError,
    summary: String,
}

impl AudioUploadError {
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
            error: RestError::payload_too_large("upload_too_large", message.clone()),
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

async fn parse_uploaded_transcription_request(
    state: &RestState,
    job_id: &JobId,
    workspace_dir: &StdPath,
    mut multipart: Multipart,
) -> Result<ParsedAudioTranscriptionJobRequest, AudioUploadError> {
    let mut fields = UploadTranscriptionFields::default();
    let max_upload_bytes = media_upload_max_bytes();

    while let Some(field) = multipart.next_field().await.map_err(|error| {
        let message = error.to_string();
        if media_upload_stream_limit_exceeded(&message) {
            AudioUploadError::payload_too_large(media_upload_too_large_message(
                "request body",
                max_upload_bytes,
            ))
        } else {
            AudioUploadError::bad_request(format!("invalid multipart request: {message}"))
        }
    })? {
        let name = field
            .name()
            .ok_or_else(|| AudioUploadError::bad_request("multipart field is missing a name"))?
            .to_string();
        match name.as_str() {
            "file" => {
                if fields.file.is_some() {
                    return Err(AudioUploadError::bad_request(
                        "`file` must appear exactly once",
                    ));
                }
                fields.file =
                    Some(write_uploaded_audio_file(workspace_dir, field, max_upload_bytes).await?);
            }
            "model_ref" => {
                set_text_field(&mut fields.model_ref, "model_ref", field).await?;
            }
            "language" => {
                set_text_field(&mut fields.language, "language", field).await?;
            }
            "output_format" => {
                set_text_field(&mut fields.output_format, "output_format", field).await?;
            }
            "output_filename" => {
                set_text_field(&mut fields.output_filename, "output_filename", field).await?;
            }
            "timestamps" => {
                let value = read_text_field("timestamps", field).await?;
                if fields.timestamps.is_some() {
                    return Err(AudioUploadError::bad_request(
                        "`timestamps` must not be provided more than once",
                    ));
                }
                fields.timestamps = Some(parse_bool_field("timestamps", &value)?);
            }
            _ => {
                return Err(AudioUploadError::bad_request(format!(
                    "unsupported audio transcription multipart field `{name}`"
                )));
            }
        }
    }

    let model_label = optional_trimmed_string(fields.model_ref)
        .ok_or_else(|| AudioUploadError::bad_request("`model_ref` is required"))?;
    let model_selector = model_selector(state, &model_label).map_err(|error| AudioUploadError {
        error,
        summary: format!("audio model `{model_label}` could not be resolved"),
    })?;
    let file = fields
        .file
        .ok_or_else(|| AudioUploadError::bad_request("`file` is required"))?;
    let output_format = fields
        .output_format
        .as_deref()
        .unwrap_or(AudioTranscriptionOutputFormat::Text.as_str())
        .parse::<AudioTranscriptionOutputFormat>()
        .map_err(|error| AudioUploadError::bad_request(error.to_string()))?;
    let output_filename =
        result_filename(fields.output_filename, output_format).map_err(|error| {
            AudioUploadError {
                error,
                summary: "invalid audio transcription output filename".to_string(),
            }
        })?;
    let request = ParsedAudioTranscriptionJobRequest {
        model_label,
        model_selector,
        input_path: file.path,
        input_original_filename: file.original_filename,
        input_media_type: file.media_type,
        input_chunk_count: file.chunk_count,
        input_state: "done".to_string(),
        output_format,
        output_filename,
        language: optional_trimmed_string(fields.language),
        timestamps: fields.timestamps.unwrap_or(false),
    };

    let store = FileJobWorkspaceStore::from_runtime_dir(state.app().layout().runtime_dir.clone());
    let workspace = store
        .finalize_stream(
            job_id,
            JobStreamKind::Input,
            input_stream_summary(&request).map_err(|error| {
                AudioUploadError::internal(
                    "audio_upload_failed",
                    format!(
                        "failed to inspect uploaded audio input `{}`: {error}",
                        request.input_path.display()
                    ),
                )
            })?,
        )
        .map_err(|error| AudioUploadError::internal("audio_upload_failed", error.to_string()))?;
    state.app().jobs().update_workspace(job_id, workspace);

    Ok(request)
}

async fn set_text_field(
    slot: &mut Option<String>,
    name: &'static str,
    field: axum::extract::multipart::Field<'_>,
) -> Result<(), AudioUploadError> {
    if slot.is_some() {
        return Err(AudioUploadError::bad_request(format!(
            "`{name}` must not be provided more than once"
        )));
    }
    *slot = Some(read_text_field(name, field).await?);
    Ok(())
}

async fn read_text_field(
    name: &'static str,
    mut field: axum::extract::multipart::Field<'_>,
) -> Result<String, AudioUploadError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = field.chunk().await.map_err(|error| {
        AudioUploadError::bad_request(format!("invalid `{name}` field: {error}"))
    })? {
        let next_len = bytes.len().saturating_add(chunk.len());
        if next_len > MAX_UPLOAD_METADATA_FIELD_BYTES {
            return Err(AudioUploadError::bad_request(format!(
                "`{name}` must be at most {MAX_UPLOAD_METADATA_FIELD_BYTES} bytes"
            )));
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|error| {
        AudioUploadError::bad_request(format!("`{name}` must be valid UTF-8: {error}"))
    })
}

async fn write_uploaded_audio_file(
    workspace_dir: &StdPath,
    mut field: axum::extract::multipart::Field<'_>,
    max_upload_bytes: usize,
) -> Result<UploadedAudioFile, AudioUploadError> {
    let original_filename = field.file_name().map(str::to_string);
    let media_type = field.content_type().map(str::to_string).unwrap_or_else(|| {
        original_filename
            .as_deref()
            .map(|name| audio_media_type(StdPath::new(name)).to_string())
            .unwrap_or_else(|| "application/octet-stream".to_string())
    });
    let filename = safe_upload_filename(original_filename.as_deref());
    let input_dir = workspace_dir.join("input");
    tokio::fs::create_dir_all(&input_dir)
        .await
        .map_err(|error| {
            AudioUploadError::internal(
                "audio_upload_failed",
                format!("create `{}` failed: {error}", input_dir.display()),
            )
        })?;
    let final_path = input_dir.join(&filename);
    let partial_path = input_dir.join(format!("{filename}.part"));
    let mut file = tokio::fs::File::create(&partial_path)
        .await
        .map_err(|error| {
            AudioUploadError::internal(
                "audio_upload_failed",
                format!("create `{}` failed: {error}", partial_path.display()),
            )
        })?;
    let mut chunk_count = 0u64;
    let mut total_bytes = 0u64;

    while let Some(chunk) = field.chunk().await.map_err(|error| {
        let message = error.to_string();
        if media_upload_stream_limit_exceeded(&message) {
            AudioUploadError::payload_too_large(media_upload_too_large_message(
                "file",
                max_upload_bytes,
            ))
        } else {
            AudioUploadError::bad_request(format!("invalid `file` upload stream: {message}"))
        }
    })? {
        if chunk.is_empty() {
            continue;
        }
        total_bytes = total_bytes.saturating_add(chunk.len() as u64);
        if total_bytes > max_upload_bytes as u64 {
            let _ = tokio::fs::remove_file(&partial_path).await;
            return Err(AudioUploadError::payload_too_large(
                media_upload_too_large_message("file", max_upload_bytes),
            ));
        }
        chunk_count = chunk_count.saturating_add(1);
        file.write_all(&chunk).await.map_err(|error| {
            AudioUploadError::internal(
                "audio_upload_failed",
                format!("write `{}` failed: {error}", partial_path.display()),
            )
        })?;
    }
    file.flush().await.map_err(|error| {
        AudioUploadError::internal(
            "audio_upload_failed",
            format!("flush `{}` failed: {error}", partial_path.display()),
        )
    })?;
    drop(file);

    if total_bytes == 0 {
        let _ = tokio::fs::remove_file(&partial_path).await;
        return Err(AudioUploadError::bad_request("`file` must not be empty"));
    }

    tokio::fs::rename(&partial_path, &final_path)
        .await
        .map_err(|error| {
            AudioUploadError::internal(
                "audio_upload_failed",
                format!(
                    "replace `{}` with `{}` failed: {error}",
                    partial_path.display(),
                    final_path.display()
                ),
            )
        })?;

    Ok(UploadedAudioFile {
        path: final_path,
        original_filename,
        media_type,
        chunk_count,
    })
}

fn parse_bool_field(name: &'static str, value: &str) -> Result<bool, AudioUploadError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" | "" => Ok(false),
        _ => Err(AudioUploadError::bad_request(format!(
            "`{name}` must be a boolean value"
        ))),
    }
}

fn safe_upload_filename(original_filename: Option<&str>) -> String {
    let candidate = original_filename
        .and_then(|name| StdPath::new(name).file_name())
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("audio-input");
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
        "audio-input".to_string()
    } else {
        sanitized.to_string()
    }
}

fn run_audio_transcription_job(
    state: RestState,
    layout: RuntimeLayoutInput,
    request: ParsedAudioTranscriptionJobRequest,
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
            "failed to inspect audio input `{}`: {error}",
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
            stage: Some("running audio transcription".to_string()),
            progress: JobProgressPatch {
                files_total: Some(1),
                files_done: Some(0),
                ..JobProgressPatch::default()
            },
            output: vec![JobOutputLine::new(
                JobStream::Event,
                format!("reading {}", request.input_path.display()),
            )],
            ..JobProgressUpdate::default()
        },
    );

    let output_path = workspace
        .workspace_dir
        .join("files")
        .join(&request.output_filename);
    let output_format = request.output_format;
    let input_path = request.input_path.clone();
    let execution = handle
        .block_on(async {
            state
                .app()
                .services()
                .kernel()
                .audio_transcription_usecase()
                .transcribe_audio(AudioTranscriptionPreparationRequest {
                    layout,
                    runtime: PythonRuntimeResolutionInput::default(),
                    model_selector: request.model_selector,
                    input_path,
                    output_path,
                    output_format,
                    language: request.language,
                    timestamps: request.timestamps,
                })
                .await
        })
        .map_err(|error| error.to_string())?;

    let output_path = execution.response.output_path.clone();
    let result_bytes = fs::read(&output_path).map_err(|error| {
        format!(
            "failed to read audio transcription output `{}`: {error}",
            output_path.display()
        )
    })?;
    store
        .write_chunk(
            &job_id,
            JobChunkWrite {
                stream: JobStreamKind::Result,
                index: 0,
                bytes: result_bytes.clone(),
            },
        )
        .and_then(|_| store.commit_chunk(&job_id, JobStreamKind::Result, 0))
        .map_err(|error| error.to_string())?;
    let result_summary = JobWorkspaceStreamSummary {
        state: "done".to_string(),
        done: true,
        failed: false,
        chunk_count: 1,
        total_bytes: result_bytes.len() as u64,
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
            stage: Some("audio transcription finished".to_string()),
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
        "audio transcription wrote {}",
        request.output_filename
    ))
    .with_artifact(
        JobArtifact::new("audio_transcription").with_path(output_path.display().to_string()),
    ))
}

fn input_stream_summary(
    request: &ParsedAudioTranscriptionJobRequest,
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

fn canonical_audio_input_path(value: &str) -> Result<PathBuf, RestError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(RestError::bad_request(
            "bad_request",
            "`path` must not be empty",
        ));
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        return Err(RestError::bad_request(
            "bad_request",
            "`path` must be an absolute audio file path",
        ));
    }
    let canonical = path.canonicalize().map_err(|error| {
        RestError::bad_request(
            "bad_request",
            format!(
                "audio input path `{}` is not readable: {error}",
                path.display()
            ),
        )
    })?;
    if !canonical.is_file() {
        return Err(RestError::bad_request(
            "bad_request",
            format!("audio input path `{}` is not a file", canonical.display()),
        ));
    }
    Ok(canonical)
}

fn result_filename(
    value: Option<String>,
    output_format: AudioTranscriptionOutputFormat,
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
            error: RestError::store_lookup("audio_model_failed", error.to_string()),
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
            error: RestError::internal("audio_model_failed", err.to_string()),
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

fn audio_media_type(path: &StdPath) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "mp4" | "m4a" => "audio/mp4",
        "flac" => "audio/flac",
        "ogg" | "oga" => "audio/ogg",
        "webm" => "audio/webm",
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
                "audio_result_failed",
                format!("invalid media type: {error}"),
            )
        })?,
    );
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{}\"", filename)).map_err(
            |error| {
                RestError::internal(
                    "audio_result_failed",
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
