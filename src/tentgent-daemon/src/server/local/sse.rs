use std::{collections::VecDeque, convert::Infallible};

use axum::body::Bytes;
use futures_util::{stream, Stream, StreamExt};
use serde_json::{json, Value};

use crate::time::unix_timestamp_seconds;

pub(super) fn openai_stream_from_local_sse<S, E>(
    upstream: S,
    bound_model_ref: String,
) -> impl Stream<Item = Result<Bytes, Infallible>>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
    E: std::fmt::Display,
{
    let mut pending = VecDeque::new();
    pending.push_back(openai_sse_json_string(&openai_stream_chunk(
        &bound_model_ref,
        Some(json!({"role": "assistant"})),
        None,
    )));
    stream::unfold(
        LocalOpenAiStreamState {
            upstream,
            bound_model_ref,
            buffer: String::new(),
            pending,
            upstream_done: false,
            sent_done: false,
        },
        |mut state| async move {
            loop {
                if let Some(chunk) = state.pending.pop_front() {
                    return Some((Ok(Bytes::from(chunk)), state));
                }
                if state.upstream_done {
                    if !state.sent_done {
                        state.sent_done = true;
                        return Some((
                            Ok(Bytes::from(openai_stream_done_string(
                                &state.bound_model_ref,
                                "stop",
                            ))),
                            state,
                        ));
                    }
                    return None;
                }
                match state.upstream.next().await {
                    Some(Ok(bytes)) => match std::str::from_utf8(&bytes) {
                        Ok(text) => {
                            state.buffer.push_str(text);
                            state.drain_complete_events();
                        }
                        Err(error) => {
                            state.push_error("chat_stream_failed", error.to_string());
                            state.upstream_done = true;
                        }
                    },
                    Some(Err(error)) => {
                        state.push_error("chat_stream_failed", error.to_string());
                        state.upstream_done = true;
                    }
                    None => {
                        state.upstream_done = true;
                        state.drain_remainder();
                    }
                }
            }
        },
    )
}

pub(super) fn claude_stream_from_local_sse<S, E>(
    upstream: S,
    bound_model_ref: String,
) -> impl Stream<Item = Result<Bytes, Infallible>>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
    E: std::fmt::Display,
{
    let mut pending = VecDeque::new();
    let context = ClaudeLocalStreamContext::new(bound_model_ref);
    context.push_start(&mut pending);
    stream::unfold(
        LocalClaudeStreamState {
            upstream,
            context,
            buffer: String::new(),
            pending,
            upstream_done: false,
            sent_stop: false,
        },
        |mut state| async move {
            loop {
                if let Some(chunk) = state.pending.pop_front() {
                    return Some((Ok(Bytes::from(chunk)), state));
                }
                if state.upstream_done {
                    if !state.sent_stop {
                        state.context.push_stop(&mut state.pending, "end_turn");
                        state.sent_stop = true;
                        continue;
                    }
                    return None;
                }
                match state.upstream.next().await {
                    Some(Ok(bytes)) => match std::str::from_utf8(&bytes) {
                        Ok(text) => {
                            state.buffer.push_str(text);
                            state.drain_complete_events();
                        }
                        Err(error) => {
                            state.push_error("chat_stream_failed", error.to_string());
                            state.upstream_done = true;
                        }
                    },
                    Some(Err(error)) => {
                        state.push_error("chat_stream_failed", error.to_string());
                        state.upstream_done = true;
                    }
                    None => {
                        state.upstream_done = true;
                        state.drain_remainder();
                    }
                }
            }
        },
    )
}

pub(super) fn gemini_stream_from_local_sse<S, E>(
    upstream: S,
    bound_model_ref: String,
) -> impl Stream<Item = Result<Bytes, Infallible>>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
    E: std::fmt::Display,
{
    stream::unfold(
        LocalGeminiStreamState {
            upstream,
            bound_model_ref,
            buffer: String::new(),
            pending: VecDeque::new(),
            upstream_done: false,
            sent_done: false,
        },
        |mut state| async move {
            loop {
                if let Some(chunk) = state.pending.pop_front() {
                    return Some((Ok(Bytes::from(chunk)), state));
                }
                if state.upstream_done {
                    if !state.sent_done {
                        state.push_done("STOP");
                        state.sent_done = true;
                        continue;
                    }
                    return None;
                }
                match state.upstream.next().await {
                    Some(Ok(bytes)) => match std::str::from_utf8(&bytes) {
                        Ok(text) => {
                            state.buffer.push_str(text);
                            state.drain_complete_events();
                        }
                        Err(error) => {
                            state.push_error("chat_stream_failed", error.to_string());
                            state.upstream_done = true;
                        }
                    },
                    Some(Err(error)) => {
                        state.push_error("chat_stream_failed", error.to_string());
                        state.upstream_done = true;
                    }
                    None => {
                        state.upstream_done = true;
                        state.drain_remainder();
                    }
                }
            }
        },
    )
}

struct LocalOpenAiStreamState<S> {
    upstream: S,
    bound_model_ref: String,
    buffer: String,
    pending: VecDeque<String>,
    upstream_done: bool,
    sent_done: bool,
}

impl<S> LocalOpenAiStreamState<S> {
    fn drain_complete_events(&mut self) {
        while let Some(index) = self.buffer.find("\n\n") {
            let block = self.buffer[..index].to_string();
            self.buffer.drain(..index + 2);
            self.push_event_block(&block);
        }
    }

    fn drain_remainder(&mut self) {
        if self.buffer.trim().is_empty() {
            self.buffer.clear();
            return;
        }
        let block = std::mem::take(&mut self.buffer);
        self.push_event_block(&block);
    }

    fn push_event_block(&mut self, block: &str) {
        if let Some((event, data)) = local_sse_event(block) {
            let done = openai_chunks_for_local_event(
                &mut self.pending,
                &self.bound_model_ref,
                &event,
                data.as_ref(),
            );
            self.sent_done |= done;
        }
    }

    fn push_error(&mut self, code: &str, message: String) {
        self.pending.push_back(openai_sse_json_string(&json!({
            "error": {
                "message": message,
                "type": code,
                "code": code
            }
        })));
        self.pending.push_back(openai_done_marker_string());
        self.sent_done = true;
    }
}

struct LocalClaudeStreamState<S> {
    upstream: S,
    context: ClaudeLocalStreamContext,
    buffer: String,
    pending: VecDeque<String>,
    upstream_done: bool,
    sent_stop: bool,
}

struct LocalGeminiStreamState<S> {
    upstream: S,
    bound_model_ref: String,
    buffer: String,
    pending: VecDeque<String>,
    upstream_done: bool,
    sent_done: bool,
}

impl<S> LocalClaudeStreamState<S> {
    fn drain_complete_events(&mut self) {
        while let Some(index) = self.buffer.find("\n\n") {
            let block = self.buffer[..index].to_string();
            self.buffer.drain(..index + 2);
            self.push_event_block(&block);
        }
    }

    fn drain_remainder(&mut self) {
        if self.buffer.trim().is_empty() {
            self.buffer.clear();
            return;
        }
        let block = std::mem::take(&mut self.buffer);
        self.push_event_block(&block);
    }

    fn push_event_block(&mut self, block: &str) {
        if let Some((event, data)) = local_sse_event(block) {
            let done = claude_events_for_local_event(
                &mut self.pending,
                &self.context,
                &event,
                data.as_ref(),
            );
            self.sent_stop |= done;
        }
    }

    fn push_error(&mut self, code: &str, message: String) {
        self.pending.push_back(claude_sse_json_string(
            "error",
            &json!({
                "type": "error",
                "error": {
                    "type": code,
                    "message": message
                }
            }),
        ));
        self.sent_stop = true;
    }
}

impl<S> LocalGeminiStreamState<S> {
    fn drain_complete_events(&mut self) {
        while let Some(index) = self.buffer.find("\n\n") {
            let block = self.buffer[..index].to_string();
            self.buffer.drain(..index + 2);
            self.push_event_block(&block);
        }
    }

    fn drain_remainder(&mut self) {
        if self.buffer.trim().is_empty() {
            self.buffer.clear();
            return;
        }
        let block = std::mem::take(&mut self.buffer);
        self.push_event_block(&block);
    }

    fn push_event_block(&mut self, block: &str) {
        if let Some((event, data)) = local_sse_event(block) {
            let done = gemini_chunks_for_local_event(
                &mut self.pending,
                &self.bound_model_ref,
                &event,
                data.as_ref(),
            );
            self.sent_done |= done;
        }
    }

    fn push_done(&mut self, finish_reason: &str) {
        self.pending
            .push_back(gemini_sse_json_string(&gemini_stream_chunk(
                &self.bound_model_ref,
                None,
                Some(finish_reason),
            )));
    }

    fn push_error(&mut self, code: &str, message: String) {
        self.pending.push_back(gemini_sse_json_string(&json!({
            "error": {
                "code": code,
                "message": message
            }
        })));
        self.sent_done = true;
    }
}

fn local_sse_event(block: &str) -> Option<(String, Option<Value>)> {
    let mut event = None;
    let mut data_lines = Vec::new();
    for line in block.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.trim());
        }
    }
    event.map(|event| {
        let data = data_lines.join("\n");
        let data = if data.is_empty() {
            None
        } else {
            serde_json::from_str(&data).ok()
        };
        (event, data)
    })
}

fn gemini_chunks_for_local_event(
    pending: &mut VecDeque<String>,
    bound_model_ref: &str,
    event: &str,
    data: Option<&Value>,
) -> bool {
    match event {
        "delta" => {
            let text = data
                .and_then(|value| value.get("text"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !text.is_empty() {
                pending.push_back(gemini_sse_json_string(&gemini_stream_chunk(
                    bound_model_ref,
                    Some(text),
                    None,
                )));
            }
            false
        }
        "done" => {
            pending.push_back(gemini_sse_json_string(&gemini_stream_chunk(
                bound_model_ref,
                None,
                Some("STOP"),
            )));
            true
        }
        "error" | "canceled" => {
            let code = data
                .and_then(|value| value.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("chat_model_failed");
            let message = data
                .and_then(|value| value.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("local chat stream failed");
            pending.push_back(gemini_sse_json_string(&json!({
                "error": {
                    "code": code,
                    "message": message
                }
            })));
            true
        }
        _ => false,
    }
}

fn claude_events_for_local_event(
    pending: &mut VecDeque<String>,
    context: &ClaudeLocalStreamContext,
    event: &str,
    data: Option<&Value>,
) -> bool {
    match event {
        "delta" => {
            let text = data
                .and_then(|value| value.get("text"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !text.is_empty() {
                pending.push_back(claude_sse_json_string(
                    "content_block_delta",
                    &json!({
                        "type": "content_block_delta",
                        "index": 0,
                        "delta": {
                            "type": "text_delta",
                            "text": text
                        }
                    }),
                ));
            }
            false
        }
        "done" => {
            context.push_stop(pending, "end_turn");
            true
        }
        "error" | "canceled" => {
            let code = data
                .and_then(|value| value.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("chat_model_failed");
            let message = data
                .and_then(|value| value.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("local chat stream failed");
            pending.push_back(claude_sse_json_string(
                "error",
                &json!({
                    "type": "error",
                    "error": {
                        "type": code,
                        "message": message
                    }
                }),
            ));
            true
        }
        _ => false,
    }
}

fn openai_chunks_for_local_event(
    pending: &mut VecDeque<String>,
    bound_model_ref: &str,
    event: &str,
    data: Option<&Value>,
) -> bool {
    match event {
        "delta" => {
            let text = data
                .and_then(|value| value.get("text"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !text.is_empty() {
                pending.push_back(openai_sse_json_string(&openai_stream_chunk(
                    bound_model_ref,
                    Some(json!({"content": text})),
                    None,
                )));
            }
            false
        }
        "done" => {
            pending.push_back(openai_stream_done_string(bound_model_ref, "stop"));
            true
        }
        "error" | "canceled" => {
            pending.push_back(openai_sse_json_string(&openai_stream_error(data)));
            pending.push_back(openai_done_marker_string());
            true
        }
        _ => false,
    }
}

fn openai_stream_done_string(bound_model_ref: &str, finish_reason: &str) -> String {
    let mut output = openai_sse_json_string(&openai_stream_chunk(
        bound_model_ref,
        Some(json!({})),
        Some(finish_reason),
    ));
    output.push_str(&openai_done_marker_string());
    output
}

#[derive(Debug, Clone)]
pub(super) struct ClaudeLocalStreamContext {
    id: String,
    model: String,
}

impl ClaudeLocalStreamContext {
    fn new(model: String) -> Self {
        Self {
            id: format!("msg-{}", unix_timestamp_seconds()),
            model,
        }
    }

    fn push_start(&self, pending: &mut VecDeque<String>) {
        pending.push_back(claude_sse_json_string(
            "message_start",
            &json!({
                "type": "message_start",
                "message": {
                    "id": self.id,
                    "type": "message",
                    "role": "assistant",
                    "model": self.model,
                    "content": [],
                    "stop_reason": null,
                    "stop_sequence": null,
                    "usage": null
                }
            }),
        ));
        pending.push_back(claude_sse_json_string(
            "content_block_start",
            &json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": {
                    "type": "text",
                    "text": ""
                }
            }),
        ));
    }

    fn push_stop(&self, pending: &mut VecDeque<String>, stop_reason: &str) {
        pending.push_back(claude_sse_json_string(
            "content_block_stop",
            &json!({
                "type": "content_block_stop",
                "index": 0
            }),
        ));
        pending.push_back(claude_sse_json_string(
            "message_delta",
            &json!({
                "type": "message_delta",
                "delta": {
                    "stop_reason": stop_reason,
                    "stop_sequence": null
                },
                "usage": null
            }),
        ));
        pending.push_back(claude_sse_json_string(
            "message_stop",
            &json!({
                "type": "message_stop"
            }),
        ));
    }
}

fn openai_done_marker_string() -> String {
    "data: [DONE]\n\n".to_string()
}

fn openai_sse_json_string(value: &Value) -> String {
    let mut output = String::new();
    output.push_str("data: ");
    output.push_str(&value.to_string());
    output.push_str("\n\n");
    output
}

fn claude_sse_json_string(event: &str, value: &Value) -> String {
    let mut output = String::new();
    output.push_str("event: ");
    output.push_str(event);
    output.push('\n');
    output.push_str("data: ");
    output.push_str(&value.to_string());
    output.push_str("\n\n");
    output
}

fn gemini_sse_json_string(value: &Value) -> String {
    let mut output = String::new();
    output.push_str("data: ");
    output.push_str(&value.to_string());
    output.push_str("\n\n");
    output
}

fn openai_stream_chunk(
    bound_model_ref: &str,
    delta: Option<Value>,
    finish_reason: Option<&str>,
) -> Value {
    json!({
        "id": format!("chatcmpl-{}", unix_timestamp_seconds()),
        "object": "chat.completion.chunk",
        "created": unix_timestamp_seconds(),
        "model": bound_model_ref,
        "choices": [{
            "index": 0,
            "delta": delta,
            "finish_reason": finish_reason,
            "logprobs": null
        }],
        "usage": null
    })
}

fn gemini_stream_chunk(
    bound_model_ref: &str,
    text: Option<&str>,
    finish_reason: Option<&str>,
) -> Value {
    let parts = text
        .map(|text| vec![json!({ "text": text })])
        .unwrap_or_default();
    json!({
        "candidates": [{
            "index": 0,
            "content": {
                "role": "model",
                "parts": parts
            },
            "finishReason": finish_reason
        }],
        "usageMetadata": null,
        "modelVersion": bound_model_ref
    })
}

fn openai_stream_error(data: Option<&Value>) -> Value {
    let code = data
        .and_then(|value| value.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("chat_model_failed");
    let message = data
        .and_then(|value| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("local chat stream failed");
    json!({
        "error": {
            "message": message,
            "type": code,
            "code": code
        }
    })
}
