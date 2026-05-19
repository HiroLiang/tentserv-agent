use std::{
    fs,
    path::{Path as StdPath, PathBuf},
};

use axum::{
    body::Body,
    extract::{Path, Query, State},
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
        JobProgressUpdate, JobRegistry, JobStream, JobTarget,
    },
    transport::rest::{error::RestError, state::RestState},
};

const DEFAULT_RESULT_MAX_CHUNKS: usize = 32;
const MAX_RESULT_CHUNKS: usize = 256;

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

    Ok((
        StatusCode::ACCEPTED,
        Json(JobResponse { job: job_item(job) }),
    ))
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

        Ok(Self {
            model_label,
            model_selector,
            input_path,
            output_format,
            output_filename,
            language,
            timestamps: request.timestamps.unwrap_or(false),
        })
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
    let input_summary = input_stream_summary(&request.input_path).map_err(|error| {
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

fn input_stream_summary(path: &StdPath) -> Result<JobWorkspaceStreamSummary, std::io::Error> {
    let metadata = fs::metadata(path)?;
    Ok(JobWorkspaceStreamSummary {
        state: "path".to_string(),
        done: true,
        failed: false,
        chunk_count: 0,
        total_bytes: metadata.len(),
        sha256: None,
        media_type: Some(audio_media_type(path).to_string()),
        original_filename: path
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::to_string),
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
