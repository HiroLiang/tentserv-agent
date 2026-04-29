from __future__ import annotations

import json
from typing import Any

SSE_CONTENT_TYPE = "text/event-stream; charset=utf-8"
SSE_CACHE_CONTROL = "no-cache"


def encode_sse_event(event: str, data: dict[str, Any]) -> bytes:
    event = validate_event_name(event)
    body = json.dumps(data, ensure_ascii=False, separators=(",", ":"))
    lines = [f"event: {event}"]
    lines.extend(f"data: {line}" for line in body.splitlines() or [""])
    lines.append("")
    lines.append("")
    return "\n".join(lines).encode("utf-8")


def delta_event(delta: str) -> bytes:
    return encode_sse_event("delta", {"delta": delta})


def done_event(finish_reason: str = "stop") -> bytes:
    return encode_sse_event("done", {"finish_reason": finish_reason})


def error_event(error: str, message: str) -> bytes:
    return encode_sse_event("error", {"error": error, "message": message})


def validate_event_name(event: str) -> str:
    event = event.strip()
    if not event:
        raise ValueError("SSE event name must not be empty")
    if "\n" in event or "\r" in event:
        raise ValueError("SSE event name must not contain newlines")
    return event
