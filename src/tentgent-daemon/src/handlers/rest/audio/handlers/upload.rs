use super::*;

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
pub(super) struct ParsedAudioTranscriptionJobRequest {
    pub(super) model_label: String,
    pub(super) model_selector: ModelRefSelector,
    pub(super) input_path: PathBuf,
    pub(super) input_original_filename: Option<String>,
    pub(super) input_media_type: String,
    pub(super) input_chunk_count: u64,
    pub(super) input_state: String,
    pub(super) output_format: AudioTranscriptionOutputFormat,
    pub(super) output_filename: String,
    pub(super) language: Option<String>,
    pub(super) timestamps: bool,
}

impl ParsedAudioTranscriptionJobRequest {
    pub(super) fn from_request(
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
pub(super) struct UploadTranscriptionFields {
    pub(super) model_ref: Option<String>,
    pub(super) language: Option<String>,
    pub(super) output_format: Option<String>,
    pub(super) output_filename: Option<String>,
    pub(super) timestamps: Option<bool>,
    pub(super) file: Option<UploadedAudioFile>,
}

#[derive(Debug)]
pub(super) struct UploadedAudioFile {
    pub(super) path: PathBuf,
    pub(super) original_filename: Option<String>,
    pub(super) media_type: String,
    pub(super) chunk_count: u64,
}

pub(super) struct AudioUploadError {
    pub(super) error: RestError,
    pub(super) summary: String,
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

pub(super) async fn parse_uploaded_transcription_request(
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

pub(super) async fn set_text_field(
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

pub(super) async fn read_text_field(
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

pub(super) async fn write_uploaded_audio_file(
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

pub(super) fn parse_bool_field(name: &'static str, value: &str) -> Result<bool, AudioUploadError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" | "" => Ok(false),
        _ => Err(AudioUploadError::bad_request(format!(
            "`{name}` must be a boolean value"
        ))),
    }
}

pub(super) fn safe_upload_filename(original_filename: Option<&str>) -> String {
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
