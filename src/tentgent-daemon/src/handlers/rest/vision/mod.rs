use std::{
    path::{Path as StdPath, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::{multipart::MultipartRejection, Multipart, State},
    Json,
};
use serde::Serialize;
use tentgent_kernel::{
    features::{
        model::{
            domain::ModelRefSelector,
            usecases::{ModelCatalogReadUseCase, ModelListRequest},
        },
        runtime::domain::PythonRuntimeResolutionInput,
        vision::{
            domain::{VisionChatGenerationOptions, VisionChatOutputFormat},
            usecases::{
                VisionChatExecutionResult, VisionChatPreparationRequest, VisionChatUseCase,
            },
        },
    },
    foundation::{error::KernelError, layout::LayoutResolveMode},
};
use tokio::io::AsyncWriteExt;

use crate::transport::rest::{
    error::RestError,
    limits::{
        media_upload_max_bytes, media_upload_stream_limit_exceeded, media_upload_too_large_message,
    },
    state::RestState,
};

const MAX_METADATA_FIELD_BYTES: usize = 8 * 1024;

pub async fn chat(
    State(state): State<RestState>,
    multipart: Result<Multipart, MultipartRejection>,
) -> Result<Json<VisionChatResponseBody>, RestError> {
    let multipart = multipart.map_err(|error| {
        RestError::bad_request("bad_request", format!("invalid multipart request: {error}"))
    })?;
    let request = parse_vision_chat_request(&state, multipart).await?;
    let temp_dir = request.temp_dir.clone();
    let result = generate_vision_chat(state, request).await;
    let _ = tokio::fs::remove_dir_all(temp_dir).await;

    Ok(Json(vision_chat_response(result?)))
}

async fn generate_vision_chat(
    state: RestState,
    request: ParsedVisionChatRequest,
) -> Result<VisionChatExecutionResult, RestError> {
    let handle = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        handle.block_on(async {
            state
                .app()
                .services()
                .kernel()
                .vision_chat_usecase()
                .generate_vision_chat(VisionChatPreparationRequest {
                    layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
                    runtime: PythonRuntimeResolutionInput::default(),
                    model_selector: request.model_selector,
                    image_path: request.image_path,
                    image_media_type: Some(request.image_media_type),
                    prompt: request.prompt,
                    system_prompt: request.system_prompt,
                    output_format: request.output_format,
                    options: VisionChatGenerationOptions {
                        max_tokens: request.max_tokens,
                        temperature: request.temperature,
                    },
                })
                .await
        })
    })
    .await
    .map_err(|error| {
        RestError::internal(
            "vision_chat_failed",
            format!("vision chat task failed: {error}"),
        )
    })?
    .map_err(vision_chat_error)
}

#[derive(Debug)]
struct ParsedVisionChatRequest {
    model_selector: ModelRefSelector,
    image_path: PathBuf,
    image_media_type: String,
    prompt: String,
    system_prompt: Option<String>,
    output_format: VisionChatOutputFormat,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    temp_dir: PathBuf,
}

#[derive(Debug, Default)]
struct VisionChatFields {
    image: Option<UploadedImageFile>,
    model_ref: Option<String>,
    prompt: Option<String>,
    system_prompt: Option<String>,
    output_format: Option<String>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
}

#[derive(Debug)]
struct UploadedImageFile {
    path: PathBuf,
    media_type: String,
}

async fn parse_vision_chat_request(
    state: &RestState,
    multipart: Multipart,
) -> Result<ParsedVisionChatRequest, RestError> {
    let temp_dir = state
        .app()
        .layout()
        .runtime_dir
        .join("tmp")
        .join("vision-chat")
        .join(unique_suffix());
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_err(|error| {
            RestError::internal(
                "vision_chat_upload_failed",
                format!("failed to create vision upload temp dir: {error}"),
            )
        })?;
    let result = parse_vision_chat_request_in_dir(state, &temp_dir, multipart).await;
    if result.is_err() {
        cleanup_temp_dir(&temp_dir).await;
    }
    result
}

async fn parse_vision_chat_request_in_dir(
    state: &RestState,
    temp_dir: &StdPath,
    mut multipart: Multipart,
) -> Result<ParsedVisionChatRequest, RestError> {
    let mut fields = VisionChatFields::default();
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
                    Some(write_uploaded_image_file(temp_dir, field, max_upload_bytes).await?);
            }
            "model_ref" => {
                set_text_field(&mut fields.model_ref, "model_ref", field).await?;
            }
            "prompt" => {
                set_text_field(&mut fields.prompt, "prompt", field).await?;
            }
            "system_prompt" => {
                set_text_field(&mut fields.system_prompt, "system_prompt", field).await?;
            }
            "output_format" => {
                set_text_field(&mut fields.output_format, "output_format", field).await?;
            }
            "max_tokens" => {
                let value = read_text_field("max_tokens", field).await?;
                if fields.max_tokens.is_some() {
                    cleanup_temp_dir(temp_dir).await;
                    return Err(RestError::bad_request(
                        "bad_request",
                        "`max_tokens` must not be provided more than once",
                    ));
                }
                fields.max_tokens = Some(parse_u32_field("max_tokens", &value)?);
            }
            "temperature" => {
                let value = read_text_field("temperature", field).await?;
                if fields.temperature.is_some() {
                    cleanup_temp_dir(temp_dir).await;
                    return Err(RestError::bad_request(
                        "bad_request",
                        "`temperature` must not be provided more than once",
                    ));
                }
                fields.temperature = Some(parse_f32_field("temperature", &value)?);
            }
            _ => {
                cleanup_temp_dir(temp_dir).await;
                return Err(RestError::bad_request(
                    "bad_request",
                    format!("unsupported vision chat multipart field `{name}`"),
                ));
            }
        }
    }

    let model_label = optional_trimmed_string(fields.model_ref)
        .ok_or_else(|| RestError::bad_request("bad_request", "`model_ref` is required"))?;
    let model_selector = model_selector(state, &model_label)?;
    let prompt = optional_trimmed_string(fields.prompt)
        .ok_or_else(|| RestError::bad_request("bad_request", "`prompt` is required"))?;
    let image = fields
        .image
        .ok_or_else(|| RestError::bad_request("bad_request", "`image` is required"))?;
    let output_format = fields
        .output_format
        .as_deref()
        .unwrap_or(VisionChatOutputFormat::Text.as_str())
        .parse::<VisionChatOutputFormat>()
        .map_err(|error| RestError::bad_request("bad_request", error.to_string()))?;

    Ok(ParsedVisionChatRequest {
        model_selector,
        image_path: image.path,
        image_media_type: image.media_type,
        prompt,
        system_prompt: optional_trimmed_string(fields.system_prompt),
        output_format,
        max_tokens: fields.max_tokens,
        temperature: fields.temperature,
        temp_dir: temp_dir.to_path_buf(),
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

async fn write_uploaded_image_file(
    temp_dir: &StdPath,
    mut field: axum::extract::multipart::Field<'_>,
    max_upload_bytes: usize,
) -> Result<UploadedImageFile, RestError> {
    let original_filename = field.file_name().map(str::to_string);
    let media_type = image_media_type(field.content_type(), original_filename.as_deref())?;
    let filename = safe_upload_filename(original_filename.as_deref());
    let final_path = temp_dir.join(&filename);
    let partial_path = temp_dir.join(format!("{filename}.part"));
    let mut file = tokio::fs::File::create(&partial_path)
        .await
        .map_err(|error| {
            RestError::internal(
                "vision_chat_upload_failed",
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
                "vision_chat_upload_failed",
                format!("write `{}` failed: {error}", partial_path.display()),
            )
        })?;
    }
    file.flush().await.map_err(|error| {
        RestError::internal(
            "vision_chat_upload_failed",
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
                "vision_chat_upload_failed",
                format!(
                    "replace `{}` with `{}` failed: {error}",
                    partial_path.display(),
                    final_path.display()
                ),
            )
        })?;

    Ok(UploadedImageFile {
        path: final_path,
        media_type,
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
    if !parsed.is_finite() || parsed < 0.0 {
        return Err(RestError::bad_request(
            "bad_request",
            format!("`{name}` must be a finite non-negative float"),
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

    let inferred = original_filename
        .map(StdPath::new)
        .and_then(|path| image_media_type_from_extension(path).map(str::to_string))
        .ok_or_else(|| {
            RestError::bad_request(
                "bad_request",
                "`image` must be image/png, image/jpeg, or image/webp",
            )
        })?;
    Ok(inferred)
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

fn safe_upload_filename(original_filename: Option<&str>) -> String {
    let candidate = original_filename
        .and_then(|name| StdPath::new(name).file_name())
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("vision-input");
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
        "vision-input".to_string()
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
            error: RestError::store_lookup("vision_chat_model_failed", error.to_string()),
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
            error: RestError::internal("vision_chat_model_failed", err.to_string()),
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

#[derive(Debug, Serialize)]
pub struct VisionChatResponseBody {
    pub model_ref: String,
    pub output_format: String,
    pub text: String,
    pub finish_reason: String,
}

fn vision_chat_response(result: VisionChatExecutionResult) -> VisionChatResponseBody {
    VisionChatResponseBody {
        model_ref: result.prepared.model.metadata.model_ref.into_string(),
        output_format: result.response.output_format.as_str().to_string(),
        text: result.response.text,
        finish_reason: result.response.finish_reason,
    }
}

fn vision_chat_error(error: KernelError) -> RestError {
    match error {
        KernelError::ModelStoreUnavailable(message) => {
            RestError::store_lookup("vision_chat_model_failed", message)
        }
        KernelError::UnsupportedTarget(message) => {
            RestError::bad_request("unsupported_target", message)
        }
        KernelError::RuntimeStateUnavailable(message) => {
            RestError::internal("vision_chat_runtime_unavailable", message)
        }
        KernelError::VisionRuntimeUnavailable(message) => {
            RestError::internal("vision_chat_runtime_failed", message)
        }
        other => RestError::kernel("vision_chat_failed", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_media_type_accepts_content_type_or_filename() {
        assert_eq!(
            image_media_type(Some("image/png"), None).expect("content type"),
            "image/png"
        );
        assert_eq!(
            image_media_type(Some("application/octet-stream"), Some("photo.webp"))
                .expect("filename"),
            "image/webp"
        );
        assert!(image_media_type(Some("text/plain"), Some("photo.txt")).is_err());
    }

    #[test]
    fn safe_upload_filename_removes_path_and_unsupported_chars() {
        assert_eq!(
            safe_upload_filename(Some("../my photo!.png")),
            "my_photo_.png"
        );
    }
}
