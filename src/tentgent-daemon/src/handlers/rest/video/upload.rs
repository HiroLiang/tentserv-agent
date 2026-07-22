use super::*;

#[derive(Debug, Deserialize)]
pub struct VideoUnderstandingResultQuery {
    pub cursor: Option<u64>,
    pub max_chunks: Option<usize>,
}

#[derive(Debug)]
pub(super) struct ParsedVideoUnderstandingJobRequest {
    pub(super) model_label: String,
    pub(super) model_selector: ModelRefSelector,
    pub(super) input_path: PathBuf,
    pub(super) input_original_filename: Option<String>,
    pub(super) input_media_type: String,
    pub(super) input_chunk_count: u64,
    pub(super) input_state: String,
    pub(super) prompt: String,
    pub(super) system_prompt: Option<String>,
    pub(super) output_format: VideoUnderstandingOutputFormat,
    pub(super) output_filename: String,
    pub(super) options: VideoUnderstandingGenerationOptions,
    pub(super) sampling: VideoSamplingOptions,
}

#[derive(Debug, Default)]
pub(super) struct UploadVideoUnderstandingFields {
    pub(super) model_ref: Option<String>,
    pub(super) prompt: Option<String>,
    pub(super) system_prompt: Option<String>,
    pub(super) output_format: Option<String>,
    pub(super) output_filename: Option<String>,
    pub(super) max_tokens: Option<u32>,
    pub(super) temperature: Option<f32>,
    pub(super) sample_fps: Option<f32>,
    pub(super) max_frames: Option<u32>,
    pub(super) max_frame_edge: Option<u32>,
    pub(super) clip_start_seconds: Option<f32>,
    pub(super) clip_duration_seconds: Option<f32>,
    pub(super) file: Option<UploadedVideoFile>,
}

#[derive(Debug)]
pub(super) struct UploadedVideoFile {
    pub(super) path: PathBuf,
    pub(super) original_filename: Option<String>,
    pub(super) media_type: String,
    pub(super) chunk_count: u64,
}

pub(super) struct VideoUploadError {
    pub(super) error: RestError,
    pub(super) summary: String,
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

pub(super) async fn parse_uploaded_understanding_request(
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

pub(super) async fn set_text_field(
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

pub(super) fn set_u32_field(
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

pub(super) fn set_f32_field(
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

pub(super) async fn read_text_field(
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

pub(super) async fn write_uploaded_video_file(
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
