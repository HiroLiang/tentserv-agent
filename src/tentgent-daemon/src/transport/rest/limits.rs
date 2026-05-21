pub(crate) const MEDIA_UPLOAD_MAX_BYTES_ENV: &str = "TENTGENT_MEDIA_UPLOAD_MAX_BYTES";
pub(crate) const DEFAULT_MEDIA_UPLOAD_MAX_BYTES: usize = 20 * 1024 * 1024;
pub(crate) const VIDEO_UPLOAD_MAX_BYTES_ENV: &str = "TENTGENT_VIDEO_UPLOAD_MAX_BYTES";
pub(crate) const DEFAULT_VIDEO_UPLOAD_MAX_BYTES: usize = 512 * 1024 * 1024;

const MULTIPART_BODY_OVERHEAD_BYTES: usize = 1024 * 1024;

pub(crate) fn media_upload_max_bytes() -> usize {
    upload_max_bytes(
        MEDIA_UPLOAD_MAX_BYTES_ENV,
        DEFAULT_MEDIA_UPLOAD_MAX_BYTES,
        "media",
    )
}

pub(crate) fn video_upload_max_bytes() -> usize {
    upload_max_bytes(
        VIDEO_UPLOAD_MAX_BYTES_ENV,
        DEFAULT_VIDEO_UPLOAD_MAX_BYTES,
        "video",
    )
}

fn upload_max_bytes(env: &'static str, default: usize, label: &'static str) -> usize {
    let raw = std::env::var(env).ok();
    match parse_upload_max_bytes(raw.as_deref()) {
        Some(limit) => limit,
        None => {
            if let Some(raw) = raw {
                tracing::warn!(
                    env = env,
                    value = %raw,
                    default = default,
                    kind = label,
                    "invalid upload limit; using default"
                );
            }
            default
        }
    }
}

pub(crate) fn media_upload_body_limit_bytes() -> usize {
    media_upload_max_bytes().saturating_add(MULTIPART_BODY_OVERHEAD_BYTES)
}

pub(crate) fn video_upload_body_limit_bytes() -> usize {
    video_upload_max_bytes().saturating_add(MULTIPART_BODY_OVERHEAD_BYTES)
}

pub(crate) fn media_upload_too_large_message(field: &str, max_bytes: usize) -> String {
    format!(
        "`{field}` upload exceeds the daemon media upload limit of {max_bytes} bytes; set {MEDIA_UPLOAD_MAX_BYTES_ENV} to adjust this limit"
    )
}

pub(crate) fn video_upload_too_large_message(field: &str, max_bytes: usize) -> String {
    format!(
        "`{field}` upload exceeds the daemon video upload limit of {max_bytes} bytes; set {VIDEO_UPLOAD_MAX_BYTES_ENV} to adjust this limit"
    )
}

pub(crate) fn media_upload_stream_limit_exceeded(message: &str) -> bool {
    message.contains("length limit exceeded")
}

fn parse_upload_max_bytes(raw: Option<&str>) -> Option<usize> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }
    let value = raw.parse::<usize>().ok()?;
    (value > 0).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_positive_media_upload_limit_bytes() {
        assert_eq!(parse_upload_max_bytes(Some("4096")), Some(4096));
        assert_eq!(parse_upload_max_bytes(Some(" 8192 ")), Some(8192));
        assert_eq!(parse_upload_max_bytes(Some("0")), None);
        assert_eq!(parse_upload_max_bytes(Some("ten")), None);
        assert_eq!(parse_upload_max_bytes(None), None);
    }

    #[test]
    fn upload_limit_message_names_env_override() {
        let message = media_upload_too_large_message("image", 123);

        assert!(message.contains("image"));
        assert!(message.contains("123 bytes"));
        assert!(message.contains(MEDIA_UPLOAD_MAX_BYTES_ENV));
    }

    #[test]
    fn video_upload_limit_message_names_video_env_override() {
        let message = video_upload_too_large_message("video", 456);

        assert!(message.contains("video"));
        assert!(message.contains("456 bytes"));
        assert!(message.contains(VIDEO_UPLOAD_MAX_BYTES_ENV));
    }

    #[test]
    fn detects_axum_body_limit_errors() {
        assert!(media_upload_stream_limit_exceeded(
            "failed to read stream: length limit exceeded"
        ));
        assert!(!media_upload_stream_limit_exceeded("invalid boundary"));
    }
}
