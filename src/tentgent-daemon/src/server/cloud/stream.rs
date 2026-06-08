use std::convert::Infallible;

use axum::response::{
    sse::{Event, Sse},
    IntoResponse, Response,
};
use futures_util::stream;
use serde_json::json;
use tentgent_kernel::features::cloud::{domain::CloudChatRequest, infra::ReqwestCloudModelClient};

use super::{error::CloudServerError, CloudServerState};

pub(super) async fn stream_response(
    state: CloudServerState,
    mut request: CloudChatRequest,
) -> Result<Response, CloudServerError> {
    request.stream = false;
    let client = ReqwestCloudModelClient::new()?;
    let response = client.complete_chat(request, &state.secret).await?;
    let mut events = Vec::new();
    if !response.text.is_empty() {
        events.push(Ok(Event::default()
            .event("delta")
            .data(json!({"delta": response.text}).to_string())));
    }
    events.push(Ok(Event::default()
        .event("done")
        .data(json!({"finish_reason": response.finish_reason}).to_string())));
    let stream = stream::iter(
        events
            .into_iter()
            .collect::<Vec<Result<Event, Infallible>>>(),
    );
    Ok(Sse::new(stream).into_response())
}
