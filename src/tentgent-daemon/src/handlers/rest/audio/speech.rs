use std::{fs, path::Path as StdPath};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
    Json,
};
use serde::Deserialize;
use tentgent_kernel::{
    features::{
        audio::{
            domain::AudioSpeechOutputFormat,
            usecases::{AudioSpeechPreparationRequest, AudioSpeechUseCase},
        },
        job::{
            domain::{JobResultFile, JobWorkspaceStreamSummary},
            infra::FileJobWorkspaceStore,
            ports::{
                JobChunkCursor, JobChunkPort, JobChunkWrite, JobResultPort, JobStreamKind,
                JobWorkspacePort,
            },
        },
        model::domain::ModelRefSelector,
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

use super::{bytes_response, model_selector, optional_trimmed_string};

const DEFAULT_RESULT_MAX_CHUNKS: usize = 32;
const MAX_RESULT_CHUNKS: usize = 256;
const DEFAULT_AUDIO_SPEECH_MAX_TEXT_BYTES: usize = 64 * 1024;
const AUDIO_SPEECH_MAX_TEXT_BYTES_ENV: &str = "TENTGENT_AUDIO_SPEECH_MAX_TEXT_BYTES";

pub async fn create_speech_job(
    State(state): State<RestState>,
    Json(request): Json<AudioSpeechJobRequest>,
) -> Result<(StatusCode, Json<JobResponse>), RestError> {
    let request = ParsedAudioSpeechJobRequest::from_request(&state, request)?;
    let job = state.app().jobs().create(
        JobKind::audio_speech(),
        "synthesize speech",
        Some(JobTarget::new("audio").with_reference(request.model_label.clone())),
        Vec::<String>::new(),
    );
    let job_id = job.job_id.clone();
    spawn_speech_worker(state, job_id, request);

    Ok((
        StatusCode::ACCEPTED,
        Json(JobResponse { job: job_item(job) }),
    ))
}

pub async fn speech_job_result(
    State(state): State<RestState>,
    Path(job_id): Path<String>,
    Query(query): Query<AudioSpeechResultQuery>,
) -> Result<Response, RestError> {
    let job_id = JobId::new(job_id);
    let Some(job) = state.app().jobs().get(&job_id) else {
        return Err(RestError::not_found(
            "not_found",
            format!("job `{job_id}` was not found"),
        ));
    };
    if job.kind.as_str() != JobKind::AUDIO_SPEECH {
        return Err(RestError::conflict(
            "wrong_job_kind",
            format!("job `{job_id}` is not an audio speech job"),
        ));
    }

    let store = FileJobWorkspaceStore::from_runtime_dir(state.app().layout().runtime_dir.clone());
    let result_files = store
        .list_result_files(&job_id)
        .map_err(|error| RestError::kernel("audio_speech_result_failed", error))?;
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
        .map_err(|error| RestError::kernel("audio_speech_result_failed", error))?;

    if read.chunks_read == 0 && !read.done {
        if job.status.is_terminal() {
            match job.status {
                JobStatus::Failed => {
                    return Err(RestError::conflict(
                        "job_failed",
                        format!(
                            "audio speech job `{job_id}` failed before producing a result; inspect `/v1/jobs/{job_id}` for details"
                        ),
                    ));
                }
                JobStatus::Interrupted => {
                    return Err(RestError::conflict(
                        "job_interrupted",
                        format!(
                            "audio speech job `{job_id}` was interrupted before producing a result"
                        ),
                    ));
                }
                JobStatus::Canceled => {
                    return Err(RestError::conflict(
                        "job_canceled",
                        format!(
                            "audio speech job `{job_id}` was canceled before producing a result"
                        ),
                    ));
                }
                JobStatus::Succeeded => {}
                JobStatus::Queued | JobStatus::Running => {}
            }
            return Err(RestError::not_found(
                "result_not_found",
                format!("audio speech result for job `{job_id}` was not found"),
            ));
        }
        return Err(RestError::conflict(
            "result_pending",
            format!("audio speech result for job `{job_id}` is not ready yet"),
        ));
    }

    let media_type = result_file
        .and_then(|file| file.media_type.as_deref())
        .unwrap_or("application/octet-stream");
    let filename = result_file
        .map(|file| file.filename.as_str())
        .unwrap_or("speech.wav");
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
pub struct AudioSpeechJobRequest {
    pub model_ref: String,
    pub text: String,
    pub output_format: Option<String>,
    pub output_filename: Option<String>,
    pub language: Option<String>,
    pub voice: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AudioSpeechResultQuery {
    pub cursor: Option<u64>,
    pub max_chunks: Option<usize>,
}

#[derive(Debug)]
struct ParsedAudioSpeechJobRequest {
    model_label: String,
    model_selector: ModelRefSelector,
    text: String,
    output_format: AudioSpeechOutputFormat,
    output_filename: String,
    language: Option<String>,
    voice: Option<String>,
}

impl ParsedAudioSpeechJobRequest {
    fn from_request(state: &RestState, request: AudioSpeechJobRequest) -> Result<Self, RestError> {
        let model_label = request.model_ref.trim().to_string();
        let model_selector = model_selector(state, &model_label)?;
        let text = validate_text(request.text)?;
        let output_format = request
            .output_format
            .as_deref()
            .unwrap_or(AudioSpeechOutputFormat::Wav.as_str())
            .parse::<AudioSpeechOutputFormat>()
            .map_err(|error| RestError::bad_request("bad_request", error.to_string()))?;
        let output_filename = result_filename(request.output_filename, output_format)?;

        Ok(Self {
            model_label,
            model_selector,
            text,
            output_format,
            output_filename,
            language: optional_trimmed_string(request.language),
            voice: optional_trimmed_string(request.voice),
        })
    }
}

fn spawn_speech_worker(state: RestState, job_id: JobId, request: ParsedAudioSpeechJobRequest) {
    let registry = state.app().jobs().clone();
    let task_state = state.clone();
    let layout = state.app().layout_input(LayoutResolveMode::Create);
    let handle = tokio::runtime::Handle::current();

    state.app().job_runner().spawn_blocking(
        registry,
        job_id,
        "preparing audio speech",
        move |registry, job_id| {
            run_audio_speech_job(task_state, layout, request, handle, registry, job_id)
        },
    );
}

fn run_audio_speech_job(
    state: RestState,
    layout: RuntimeLayoutInput,
    request: ParsedAudioSpeechJobRequest,
    handle: tokio::runtime::Handle,
    registry: JobRegistry,
    job_id: JobId,
) -> Result<JobCompletion, String> {
    let store = FileJobWorkspaceStore::from_runtime_dir(state.app().layout().runtime_dir.clone());
    let workspace = store
        .open_workspace(&job_id)
        .map_err(|error| error.to_string())?;
    registry.update_progress(
        &job_id,
        JobProgressUpdate {
            stage: Some("running audio speech".to_string()),
            progress: JobProgressPatch {
                files_total: Some(1),
                files_done: Some(0),
                ..JobProgressPatch::default()
            },
            output: vec![JobOutputLine::new(JobStream::Event, "synthesizing speech")],
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
                .audio_speech_usecase()
                .synthesize_speech(AudioSpeechPreparationRequest {
                    layout,
                    runtime: PythonRuntimeResolutionInput::default(),
                    model_selector: request.model_selector,
                    text: request.text,
                    output_path,
                    output_format,
                    language: request.language,
                    voice: request.voice,
                })
                .await
        })
        .map_err(|error| error.to_string())?;

    let output_path = execution.response.output_path.clone();
    let result_bytes = fs::read(&output_path).map_err(|error| {
        format!(
            "failed to read audio speech output `{}`: {error}",
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
            stage: Some("audio speech finished".to_string()),
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

    Ok(
        JobCompletion::new(format!("audio speech wrote {}", request.output_filename))
            .with_artifact(
                JobArtifact::new("audio_speech").with_path(output_path.display().to_string()),
            ),
    )
}

fn validate_text(value: String) -> Result<String, RestError> {
    let Some(text) = optional_trimmed_string(Some(value)) else {
        return Err(RestError::bad_request(
            "bad_request",
            "`text` must not be empty",
        ));
    };
    let max_bytes = max_text_bytes()?;
    let byte_len = text.len();
    if byte_len > max_bytes {
        return Err(RestError::bad_request(
            "bad_request",
            format!("`text` is {byte_len} bytes, which exceeds the {max_bytes} byte limit"),
        ));
    }
    Ok(text)
}

fn max_text_bytes() -> Result<usize, RestError> {
    let raw = std::env::var(AUDIO_SPEECH_MAX_TEXT_BYTES_ENV).unwrap_or_default();
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(DEFAULT_AUDIO_SPEECH_MAX_TEXT_BYTES);
    }
    let value = raw.parse::<usize>().map_err(|error| {
        RestError::bad_request(
            "bad_request",
            format!("{AUDIO_SPEECH_MAX_TEXT_BYTES_ENV} must be a positive integer: {error}"),
        )
    })?;
    if value == 0 {
        return Err(RestError::bad_request(
            "bad_request",
            format!("{AUDIO_SPEECH_MAX_TEXT_BYTES_ENV} must be a positive integer"),
        ));
    }
    Ok(value)
}

fn result_filename(
    value: Option<String>,
    output_format: AudioSpeechOutputFormat,
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
