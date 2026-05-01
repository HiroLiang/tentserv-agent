use miette::IntoDiagnostic;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::mpsc,
};

const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 12 * 1024 * 1024;

pub(crate) async fn read_request(stream: &mut TcpStream) -> miette::Result<HttpRequest> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];

    loop {
        let read = stream.read(&mut chunk).await.into_diagnostic()?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if buffer.len() > MAX_HEADER_BYTES {
            return Ok(HttpRequest::header_too_large());
        }
    }

    let Some(header_end) = find_header_end(&buffer) else {
        return Ok(HttpRequest::invalid());
    };
    let headers = &buffer[..header_end];
    let request = String::from_utf8_lossy(headers);
    let Some(request_line) = request.lines().next() else {
        return Ok(HttpRequest::invalid());
    };

    let mut parts = request_line.split_whitespace();
    let Some(method) = parts.next() else {
        return Ok(HttpRequest::invalid());
    };
    let Some(target) = parts.next() else {
        return Ok(HttpRequest::invalid());
    };
    let Some(version) = parts.next() else {
        return Ok(HttpRequest::invalid());
    };

    let mut content_length = 0_usize;
    let mut request_headers = Vec::new();
    for header in request.lines().skip(1) {
        let Some((name, value)) = header.split_once(':') else {
            continue;
        };
        let name = name.trim().to_string();
        let value = value.trim().to_string();
        if name.eq_ignore_ascii_case("content-length") {
            content_length = match value.parse::<usize>() {
                Ok(length) => length,
                Err(_) => {
                    return Ok(HttpRequest::bad_request(
                        "invalid Content-Length header".to_string(),
                    ))
                }
            };
        }
        request_headers.push((name, value));
    }
    if content_length > MAX_BODY_BYTES {
        return Ok(HttpRequest::body_too_large());
    }

    let body_start = header_end + 4;
    let mut body = buffer[body_start..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut chunk).await.into_diagnostic()?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
        if body.len() > MAX_BODY_BYTES {
            return Ok(HttpRequest::body_too_large());
        }
    }
    body.truncate(content_length);

    let (path, query_params) = split_target(target);

    Ok(HttpRequest {
        method: method.to_string(),
        path,
        query_params,
        version: version.to_string(),
        headers: request_headers,
        body,
        parse_error: None,
    })
}

pub(crate) async fn write_response(
    stream: &mut TcpStream,
    response: HttpResponse,
) -> miette::Result<()> {
    let reason = reason_phrase(response.status_code);
    let content_length = match &response.body {
        HttpBody::Buffered(body) => Some(body.len()),
        HttpBody::Proxy(_) => None,
        HttpBody::Stream(_) => None,
    };
    let mut header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\n",
        response.status_code, reason, response.content_type
    );
    if let Some(cache_control) = &response.cache_control {
        header.push_str(&format!("Cache-Control: {cache_control}\r\n"));
    }
    for (name, value) in &response.headers {
        header.push_str(&format!("{name}: {value}\r\n"));
    }
    if let Some(content_length) = content_length {
        header.push_str(&format!("Content-Length: {content_length}\r\n"));
    }
    header.push_str("Connection: close\r\n\r\n");
    stream
        .write_all(header.as_bytes())
        .await
        .into_diagnostic()?;
    match response.body {
        HttpBody::Buffered(body) => {
            stream.write_all(&body).await.into_diagnostic()?;
        }
        HttpBody::Proxy(mut upstream) => {
            while let Some(chunk) = upstream.chunk().await.into_diagnostic()? {
                stream.write_all(&chunk).await.into_diagnostic()?;
            }
        }
        HttpBody::Stream(mut chunks) => {
            while let Some(chunk) = chunks.recv().await {
                stream.write_all(&chunk).await.into_diagnostic()?;
            }
        }
    }
    stream.shutdown().await.into_diagnostic()?;
    Ok(())
}

pub(crate) fn reason_phrase(status_code: u16) -> &'static str {
    match status_code {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        500 => "Internal Server Error",
        _ => "Status",
    }
}

pub(crate) fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn split_target(target: &str) -> (String, Vec<(String, String)>) {
    let Some((path, query)) = target.split_once('?') else {
        return (target.to_string(), Vec::new());
    };

    let query_params = query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (name, value) = part.split_once('=').unwrap_or((part, ""));
            (name.to_string(), value.to_string())
        })
        .collect();

    (path.to_string(), query_params)
}

#[derive(Debug)]
pub(crate) struct HttpRequest {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) query_params: Vec<(String, String)>,
    pub(crate) version: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
    pub(crate) parse_error: Option<HttpParseError>,
}

#[derive(Debug)]
pub(crate) struct HttpParseError {
    pub(crate) status_code: u16,
    pub(crate) message: String,
}

impl HttpRequest {
    pub(crate) fn method_label(&self) -> &str {
        if self.method.is_empty() {
            "(invalid)"
        } else {
            &self.method
        }
    }

    pub(crate) fn path_label(&self) -> &str {
        if self.path.is_empty() {
            "(invalid)"
        } else {
            &self.path
        }
    }

    pub(crate) fn query_values<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.query_params
            .iter()
            .filter(move |(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    fn invalid() -> Self {
        Self {
            method: String::new(),
            path: String::new(),
            query_params: Vec::new(),
            version: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parse_error: Some(HttpParseError {
                status_code: 400,
                message: "invalid HTTP request line".to_string(),
            }),
        }
    }

    fn bad_request(message: String) -> Self {
        Self {
            method: String::new(),
            path: String::new(),
            query_params: Vec::new(),
            version: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parse_error: Some(HttpParseError {
                status_code: 400,
                message,
            }),
        }
    }

    fn header_too_large() -> Self {
        Self {
            method: String::new(),
            path: String::new(),
            query_params: Vec::new(),
            version: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parse_error: Some(HttpParseError {
                status_code: 413,
                message: "request headers exceeded the size limit".to_string(),
            }),
        }
    }

    fn body_too_large() -> Self {
        Self {
            method: String::new(),
            path: String::new(),
            query_params: Vec::new(),
            version: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parse_error: Some(HttpParseError {
                status_code: 413,
                message: "request body exceeded the size limit".to_string(),
            }),
        }
    }
}

#[derive(Debug)]
pub(crate) struct HttpResponse {
    pub(crate) status_code: u16,
    pub(crate) content_type: String,
    pub(crate) cache_control: Option<String>,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: HttpBody,
}

pub(crate) enum HttpBody {
    Buffered(Vec<u8>),
    Proxy(reqwest::Response),
    Stream(mpsc::Receiver<Vec<u8>>),
}

impl std::fmt::Debug for HttpBody {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Buffered(body) => formatter
                .debug_tuple("Buffered")
                .field(&format_args!("{} bytes", body.len()))
                .finish(),
            Self::Proxy(_) => formatter.debug_tuple("Proxy").finish(),
            Self::Stream(_) => formatter.debug_tuple("Stream").finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_target_keeps_route_path_and_query_params() {
        let (path, query_params) =
            split_target("/v1/daemon/logs/stderr?tail_bytes=10&unused=value&flag");

        assert_eq!(path, "/v1/daemon/logs/stderr");
        assert_eq!(
            query_params,
            vec![
                ("tail_bytes".to_string(), "10".to_string()),
                ("unused".to_string(), "value".to_string()),
                ("flag".to_string(), String::new()),
            ]
        );
    }
}
