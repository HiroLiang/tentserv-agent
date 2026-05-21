use std::path::{Path as StdPath, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{multipart::Field, Multipart};
use tentgent_kernel::features::{
    adapter::domain::{AdapterRefSelector, LoraScale},
    image_generation::domain::{
        ImageGenerationDimensions, ImageGenerationInput, ImageGenerationOptions,
        ImageGenerationOutputFormat, ImageTransformStrength,
    },
    job::{
        domain::JobWorkspaceStreamSummary, infra::FileJobWorkspaceStore, ports::JobWorkspacePort,
    },
};
use tokio::io::AsyncWriteExt;

use super::{
    model_selector, optional_trimmed_string, result_filename, ParsedImageGenerationJobRequest,
};
use crate::{
    runtime::JobId,
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

#[derive(Debug)]
pub(super) struct ParsedImageTransformUpload {
    pub(super) request: ParsedImageGenerationJobRequest,
    temp_dir: PathBuf,
}

#[derive(Debug, Default)]
struct ImageTransformFields {
    image: Option<UploadedTransformImage>,
    mask: Option<UploadedTransformImage>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImageEditWorkflow {
    Transform,
    Inpaint,
}

impl ImageEditWorkflow {
    const fn temp_dir_name(self) -> &'static str {
        match self {
            Self::Transform => "image-transform",
            Self::Inpaint => "image-inpaint",
        }
    }

    const fn upload_error_code(self) -> &'static str {
        match self {
            Self::Transform => "image_transform_upload_failed",
            Self::Inpaint => "image_inpaint_upload_failed",
        }
    }

    const fn unsupported_field_label(self) -> &'static str {
        match self {
            Self::Transform => "image transform",
            Self::Inpaint => "image inpaint",
        }
    }

    const fn default_strength(self) -> f32 {
        match self {
            Self::Transform => ImageTransformStrength::DEFAULT,
            Self::Inpaint => 1.0,
        }
    }
}

pub(super) async fn parse_image_transform_upload(
    state: &RestState,
    multipart: Multipart,
) -> Result<ParsedImageTransformUpload, RestError> {
    parse_image_edit_upload(state, multipart, ImageEditWorkflow::Transform).await
}

pub(super) async fn parse_image_inpaint_upload(
    state: &RestState,
    multipart: Multipart,
) -> Result<ParsedImageTransformUpload, RestError> {
    parse_image_edit_upload(state, multipart, ImageEditWorkflow::Inpaint).await
}

async fn parse_image_edit_upload(
    state: &RestState,
    multipart: Multipart,
    workflow: ImageEditWorkflow,
) -> Result<ParsedImageTransformUpload, RestError> {
    let temp_dir = state
        .app()
        .layout()
        .runtime_dir
        .join("tmp")
        .join(workflow.temp_dir_name())
        .join(unique_suffix());
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_err(|error| {
            RestError::internal(
                workflow.upload_error_code(),
                format!(
                    "failed to create {} upload temp dir: {error}",
                    workflow.unsupported_field_label()
                ),
            )
        })?;
    let result = parse_image_transform_upload_in_dir(state, &temp_dir, multipart, workflow).await;
    if result.is_err() {
        cleanup_temp_dir(&temp_dir).await;
    }
    result.map(|request| ParsedImageTransformUpload { request, temp_dir })
}

async fn parse_image_transform_upload_in_dir(
    state: &RestState,
    temp_dir: &StdPath,
    mut multipart: Multipart,
    workflow: ImageEditWorkflow,
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
                fields.image = Some(
                    write_uploaded_transform_image(
                        temp_dir,
                        field,
                        max_upload_bytes,
                        "image",
                        "image-input",
                        workflow,
                    )
                    .await?,
                );
            }
            "mask" if workflow == ImageEditWorkflow::Inpaint => {
                if fields.mask.is_some() {
                    cleanup_temp_dir(temp_dir).await;
                    return Err(RestError::bad_request(
                        "bad_request",
                        "`mask` must appear exactly once",
                    ));
                }
                fields.mask = Some(
                    write_uploaded_transform_image(
                        temp_dir,
                        field,
                        max_upload_bytes,
                        "mask",
                        "mask-input",
                        workflow,
                    )
                    .await?,
                );
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
                    format!(
                        "unsupported {} multipart field `{name}`",
                        workflow.unsupported_field_label()
                    ),
                ));
            }
        }
    }

    let image = fields
        .image
        .ok_or_else(|| RestError::bad_request("bad_request", "`image` is required"))?;
    let mask = match workflow {
        ImageEditWorkflow::Transform => None,
        ImageEditWorkflow::Inpaint => Some(
            fields
                .mask
                .ok_or_else(|| RestError::bad_request("bad_request", "`mask` is required"))?,
        ),
    };
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
        ImageTransformStrength::new(fields.strength.unwrap_or(workflow.default_strength()))
            .map_err(|error| RestError::bad_request("bad_request", error.to_string()))?;
    let input_summary = edit_input_summary(&image, mask.as_ref());
    let input = match mask {
        Some(mask) => ImageGenerationInput::Inpaint {
            image_path: image.path,
            image_media_type: Some(image.media_type),
            mask_path: mask.path,
            mask_media_type: Some(mask.media_type),
            strength,
        },
        None => ImageGenerationInput::ImageToImage {
            image_path: image.path,
            media_type: Some(image.media_type),
            strength,
        },
    };

    Ok(ParsedImageGenerationJobRequest {
        model_label,
        model_selector,
        adapter_selector,
        lora_scale,
        input,
        input_summary,
        prompt,
        negative_prompt: optional_trimmed_string(fields.negative_prompt),
        output_format,
        output_filename,
        options,
    })
}

pub(super) async fn persist_edit_input(
    state: &RestState,
    job_id: &JobId,
    upload: ParsedImageTransformUpload,
) -> Result<ParsedImageGenerationJobRequest, RestError> {
    let store = FileJobWorkspaceStore::from_runtime_dir(state.app().layout().runtime_dir.clone());
    let workspace = store
        .open_workspace(job_id)
        .map_err(|error| RestError::kernel("image_edit_workspace_failed", error))?;
    let mut request = upload.request;
    let input_dir = workspace.workspace_dir.join("input");
    tokio::fs::create_dir_all(&input_dir)
        .await
        .map_err(|error| {
            RestError::internal(
                "image_edit_workspace_failed",
                format!("failed to create image edit input dir: {error}"),
            )
        })?;

    match &mut request.input {
        ImageGenerationInput::TextToImage => {}
        ImageGenerationInput::ImageToImage { image_path, .. } => {
            *image_path = move_uploaded_edit_file(&input_dir, image_path, "uploaded image").await?;
        }
        ImageGenerationInput::Inpaint {
            image_path,
            mask_path,
            ..
        } => {
            *image_path = move_uploaded_edit_file(&input_dir, image_path, "uploaded image").await?;
            *mask_path = move_uploaded_edit_file(&input_dir, mask_path, "uploaded mask").await?;
        }
    }

    cleanup_temp_dir(&upload.temp_dir).await;
    Ok(request)
}

async fn move_uploaded_edit_file(
    input_dir: &StdPath,
    source_path: &StdPath,
    label: &'static str,
) -> Result<PathBuf, RestError> {
    let filename = source_path
        .file_name()
        .and_then(|value| value.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| label.replace(' ', "-"));
    let final_path = input_dir.join(filename);
    tokio::fs::rename(source_path, &final_path)
        .await
        .map_err(|error| {
            RestError::internal(
                "image_edit_workspace_failed",
                format!(
                    "failed to move {label} `{}` into job workspace `{}`: {error}",
                    source_path.display(),
                    final_path.display()
                ),
            )
        })?;
    Ok(final_path)
}

fn edit_input_summary(
    image: &UploadedTransformImage,
    mask: Option<&UploadedTransformImage>,
) -> JobWorkspaceStreamSummary {
    let total_bytes = image.total_bytes + mask.map(|mask| mask.total_bytes).unwrap_or_default();
    let original_filename = match mask {
        Some(mask) => Some(format!(
            "image={}; mask={}",
            image.original_filename.as_deref().unwrap_or("image"),
            mask.original_filename.as_deref().unwrap_or("mask")
        )),
        None => image.original_filename.clone(),
    };

    JobWorkspaceStreamSummary {
        state: "done".to_string(),
        done: true,
        failed: false,
        chunk_count: if mask.is_some() { 2 } else { 1 },
        total_bytes,
        sha256: None,
        media_type: Some(
            mask.map(|_| "multipart/form-data")
                .unwrap_or(image.media_type.as_str())
                .to_string(),
        ),
        original_filename,
    }
}

async fn write_uploaded_transform_image(
    temp_dir: &StdPath,
    mut field: Field<'_>,
    max_upload_bytes: usize,
    field_name: &'static str,
    fallback_filename: &'static str,
    workflow: ImageEditWorkflow,
) -> Result<UploadedTransformImage, RestError> {
    let original_filename = field.file_name().map(str::to_string);
    let media_type = image_media_type(
        field_name,
        field.content_type(),
        original_filename.as_deref(),
    )?;
    let mut filename = safe_upload_filename(original_filename.as_deref(), fallback_filename);
    let original_ext = image_extension_for_media_type(&media_type);
    if temp_dir.join(&filename).exists() {
        filename = match original_ext {
            Some(ext) => format!("{fallback_filename}.{ext}"),
            None => fallback_filename.to_string(),
        };
    }
    let final_path = temp_dir.join(&filename);
    let partial_path = temp_dir.join(format!("{filename}.part"));
    let mut file = tokio::fs::File::create(&partial_path)
        .await
        .map_err(|error| {
            RestError::internal(
                workflow.upload_error_code(),
                format!("create `{}` failed: {error}", partial_path.display()),
            )
        })?;
    let mut total_bytes = 0u64;

    while let Some(chunk) = field.chunk().await.map_err(|error| {
        let message = error.to_string();
        if media_upload_stream_limit_exceeded(&message) {
            RestError::payload_too_large(
                "upload_too_large",
                media_upload_too_large_message(field_name, max_upload_bytes),
            )
        } else {
            RestError::bad_request(
                "bad_request",
                format!("invalid `{field_name}` upload stream: {message}"),
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
                media_upload_too_large_message(field_name, max_upload_bytes),
            ));
        }
        file.write_all(&chunk).await.map_err(|error| {
            RestError::internal(
                workflow.upload_error_code(),
                format!("write `{}` failed: {error}", partial_path.display()),
            )
        })?;
    }
    file.flush().await.map_err(|error| {
        RestError::internal(
            workflow.upload_error_code(),
            format!("flush `{}` failed: {error}", partial_path.display()),
        )
    })?;
    drop(file);

    if total_bytes == 0 {
        let _ = tokio::fs::remove_file(&partial_path).await;
        return Err(RestError::bad_request(
            "bad_request",
            format!("`{field_name}` must not be empty"),
        ));
    }

    tokio::fs::rename(&partial_path, &final_path)
        .await
        .map_err(|error| {
            RestError::internal(
                workflow.upload_error_code(),
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
    field: Field<'_>,
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

async fn read_text_field(name: &'static str, mut field: Field<'_>) -> Result<String, RestError> {
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
    field: Field<'_>,
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
    field: Field<'_>,
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
    field: Field<'_>,
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
    field_name: &'static str,
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
                format!("`{field_name}` must be image/png, image/jpeg, or image/webp"),
            )
        })
}

fn image_extension_for_media_type(media_type: &str) -> Option<&'static str> {
    match media_type {
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        _ => None,
    }
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
