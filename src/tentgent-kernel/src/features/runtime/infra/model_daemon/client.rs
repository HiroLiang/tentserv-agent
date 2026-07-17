pub async fn http_error_detail(response: reqwest::Response) -> String {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if body.trim().is_empty() {
        return format!("model runtime HTTP request failed with status {status}");
    }
    match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(value) => match value.get("detail") {
            Some(serde_json::Value::String(detail)) => {
                format!("model runtime HTTP {status}: {detail}")
            }
            Some(detail) => format!("model runtime HTTP {status}: {detail}"),
            None => format!("model runtime HTTP {status}: {value}"),
        },
        Err(_) => format!("model runtime HTTP {status}: {body}"),
    }
}
