use axum::http::StatusCode;

use super::error::LocalServerError;

/// Lifecycle operations belong to the managed runtime, never its public proxy.
/// Normalize the forms that URL forwarding / ASGI path decoding can interpret,
/// without changing the request that ordinary inference forwarding receives.
pub(super) fn reject_lifecycle_path(path: &str) -> Result<(), LocalServerError> {
    // URL forwarding resolves dot segments before ASGI percent-decodes them.
    // Doing these in the opposite order can hide a lifecycle destination.
    let forwarded_url = reqwest::Url::parse(&format!("http://runtime.invalid{path}"));
    let path = forwarded_url.as_ref().map_or(path, |url| url.path());
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2])) {
                decoded.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    let mut segments: Vec<&[u8]> = Vec::new();
    for segment in decoded.split(|byte| matches!(byte, b'/' | b'\\')) {
        match segment {
            b"" | b"." => {}
            b".." => {
                segments.pop();
            }
            _ => segments.push(segment),
        }
    }
    if segments.starts_with(&[b"v1", b"lifecycle"])
        || segments.starts_with(&[b"internal", b"v1", b"lifecycle"])
    {
        return Err(LocalServerError {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: "runtime lifecycle operations are not public server routes".to_string(),
        });
    }
    Ok(())
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
