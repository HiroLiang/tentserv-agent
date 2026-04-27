from __future__ import annotations

import json
from http import HTTPStatus
from typing import Any

from tentgent_daemon.runtime.adapters import (
    AdapterAmbiguousError,
    AdapterBackendUnsupportedError,
    AdapterExecutionNotImplementedError,
    AdapterIncompatibleError,
    AdapterNotFoundError,
)
from tentgent_daemon.runtime.chat import Message

from .session import ChatRequestPayload, RuntimeSession


def handle_chat_request(raw_body: bytes, session: RuntimeSession) -> tuple[HTTPStatus, dict[str, Any]]:
    try:
        payload = json.loads(raw_body.decode("utf-8"))
    except json.JSONDecodeError as exc:
        return (
            HTTPStatus.BAD_REQUEST,
            {"error": "invalid_json", "message": str(exc)},
        )

    try:
        request = parse_chat_request(payload)
    except ValueError as exc:
        return (
            HTTPStatus.BAD_REQUEST,
            {"error": "invalid_request", "message": str(exc)},
        )

    if request.stream:
        return (
            HTTPStatus.NOT_IMPLEMENTED,
            {
                "error": "stream_not_implemented",
                "message": "Slice 5 does not define the HTTP streaming protocol yet.",
            },
        )

    try:
        text = session.generate(request)
    except AdapterNotFoundError as exc:
        return (
            HTTPStatus.NOT_FOUND,
            {"error": "adapter_not_found", "message": str(exc)},
        )
    except AdapterAmbiguousError as exc:
        return (
            HTTPStatus.CONFLICT,
            {"error": "adapter_ambiguous", "message": str(exc)},
        )
    except AdapterIncompatibleError as exc:
        return (
            HTTPStatus.CONFLICT,
            {"error": "adapter_incompatible", "message": str(exc)},
        )
    except AdapterBackendUnsupportedError as exc:
        return (
            HTTPStatus.NOT_IMPLEMENTED,
            {"error": "adapter_backend_unsupported", "message": str(exc)},
        )
    except AdapterExecutionNotImplementedError as exc:
        return (
            HTTPStatus.NOT_IMPLEMENTED,
            {"error": "adapter_execution_not_implemented", "message": str(exc)},
        )
    except NotImplementedError as exc:
        return (
            HTTPStatus.NOT_IMPLEMENTED,
            {"error": "not_implemented", "message": str(exc)},
        )
    except Exception as exc:  # pragma: no cover - backend runtime surface.
        return (
            HTTPStatus.INTERNAL_SERVER_ERROR,
            {"error": "generation_failed", "message": str(exc)},
        )

    return (
        HTTPStatus.OK,
        {
            "text": text,
            "stream": False,
        },
    )


def parse_chat_request(payload: dict[str, Any]) -> ChatRequestPayload:
    messages_raw = payload.get("messages")
    if not isinstance(messages_raw, list) or not messages_raw:
        raise ValueError("`messages` must be a non-empty array")

    messages = tuple(parse_message(item) for item in messages_raw)

    max_tokens = payload.get("max_tokens")
    if max_tokens is not None and not isinstance(max_tokens, int):
        raise ValueError("`max_tokens` must be an integer when provided")

    temperature = payload.get("temperature")
    if temperature is not None and not isinstance(temperature, (int, float)):
        raise ValueError("`temperature` must be a number when provided")

    adapter_ref = payload.get("adapter_ref")
    if adapter_ref is not None and not isinstance(adapter_ref, str):
        raise ValueError("`adapter_ref` must be a string when provided")
    if isinstance(adapter_ref, str):
        adapter_ref = adapter_ref.strip()
        if not adapter_ref:
            raise ValueError("`adapter_ref` must not be empty when provided")

    stream = payload.get("stream", False)
    if not isinstance(stream, bool):
        raise ValueError("`stream` must be a boolean when provided")

    return ChatRequestPayload(
        messages=messages,
        max_tokens=max_tokens,
        temperature=float(temperature) if temperature is not None else None,
        adapter_ref=adapter_ref,
        stream=stream,
    )


def parse_message(payload: Any) -> Message:
    if not isinstance(payload, dict):
        raise ValueError("each message must be an object")

    role = payload.get("role")
    content = payload.get("content")

    if role not in {"system", "user", "assistant"}:
        raise ValueError("message role must be one of: system, user, assistant")
    if not isinstance(content, str) or not content.strip():
        raise ValueError("message content must be a non-empty string")

    return Message(role=role, content=content.strip())
