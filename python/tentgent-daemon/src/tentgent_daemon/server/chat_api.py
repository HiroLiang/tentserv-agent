from __future__ import annotations

import json
from http import HTTPStatus
from typing import Any

from tentgent_daemon.providers import (
    ProviderRequestError,
    ProviderResponseError,
    ProviderTransportError,
)
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
        request = decode_chat_request(raw_body)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        return (
            HTTPStatus.BAD_REQUEST,
            {"error": "invalid_json", "message": str(exc)},
        )
    except ValueError as exc:
        return (
            HTTPStatus.BAD_REQUEST,
            {"error": "invalid_request", "message": str(exc)},
        )

    return handle_parsed_chat_request(request, session)


def decode_chat_request(raw_body: bytes) -> ChatRequestPayload:
    payload = json.loads(raw_body.decode("utf-8"))
    if not isinstance(payload, dict):
        raise ValueError("request body must be a JSON object")
    return parse_chat_request(payload)


def handle_parsed_chat_request(
    request: ChatRequestPayload,
    session: RuntimeSession,
) -> tuple[HTTPStatus, dict[str, Any]]:
    if request.stream:
        return stream_not_implemented_response(
            "HTTP chat streaming is handled by the SSE server path."
        )

    try:
        text = session.generate(request)
    except Exception as exc:  # pragma: no cover - backend runtime surface.
        return chat_generation_error_response(exc)

    return (
        HTTPStatus.OK,
        {
            "text": text,
            "stream": False,
        },
    )


def chat_generation_error_response(exc: Exception) -> tuple[HTTPStatus, dict[str, Any]]:
    if isinstance(exc, AdapterNotFoundError):
        return (
            HTTPStatus.NOT_FOUND,
            {"error": "adapter_not_found", "message": str(exc)},
        )
    if isinstance(exc, AdapterAmbiguousError):
        return (
            HTTPStatus.CONFLICT,
            {"error": "adapter_ambiguous", "message": str(exc)},
        )
    if isinstance(exc, AdapterIncompatibleError):
        return (
            HTTPStatus.CONFLICT,
            {"error": "adapter_incompatible", "message": str(exc)},
        )
    if isinstance(exc, AdapterBackendUnsupportedError):
        return (
            HTTPStatus.NOT_IMPLEMENTED,
            {"error": "adapter_backend_unsupported", "message": str(exc)},
        )
    if isinstance(exc, AdapterExecutionNotImplementedError):
        return (
            HTTPStatus.NOT_IMPLEMENTED,
            {"error": "adapter_execution_not_implemented", "message": str(exc)},
        )
    if isinstance(exc, ProviderRequestError):
        return (
            HTTPStatus.BAD_REQUEST,
            {"error": "provider_request_invalid", "message": str(exc)},
        )
    if isinstance(exc, ProviderResponseError):
        return (
            HTTPStatus.BAD_GATEWAY,
            {"error": "provider_response_failed", "message": str(exc)},
        )
    if isinstance(exc, ProviderTransportError):
        return (
            HTTPStatus.BAD_GATEWAY,
            {"error": "provider_transport_failed", "message": str(exc)},
        )
    if isinstance(exc, NotImplementedError):
        return (
            HTTPStatus.NOT_IMPLEMENTED,
            {"error": "not_implemented", "message": str(exc)},
        )
    return (
        HTTPStatus.INTERNAL_SERVER_ERROR,
        {"error": "generation_failed", "message": str(exc)},
    )


def stream_preflight_error_response(exc: Exception) -> tuple[HTTPStatus, dict[str, Any]]:
    if isinstance(
        exc,
        (
            AdapterNotFoundError,
            AdapterAmbiguousError,
            AdapterIncompatibleError,
            AdapterBackendUnsupportedError,
            AdapterExecutionNotImplementedError,
            ProviderRequestError,
            ProviderResponseError,
            ProviderTransportError,
        ),
    ):
        return chat_generation_error_response(exc)
    if isinstance(exc, NotImplementedError):
        return stream_not_implemented_response(str(exc))
    return chat_generation_error_response(exc)


def stream_not_implemented_response(
    message: str,
) -> tuple[HTTPStatus, dict[str, Any]]:
    return (
        HTTPStatus.NOT_IMPLEMENTED,
        {
            "error": "stream_not_implemented",
            "message": message,
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
