//! Pure network endpoint formatting rules.

pub fn http_url_from_host_port(host: &str, port: u16) -> String {
    format!("http://{}:{port}", format_host_for_url_authority(host))
}

pub fn format_host_for_url_authority(host: &str) -> String {
    let trimmed = host.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        trimmed.to_string()
    } else if trimmed.contains(':') {
        format!("[{trimmed}]")
    } else {
        trimmed.to_string()
    }
}
