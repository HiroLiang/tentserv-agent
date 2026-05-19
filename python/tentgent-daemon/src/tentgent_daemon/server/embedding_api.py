from __future__ import annotations

import json
from dataclasses import dataclass
from http import HTTPStatus
from typing import Any

from .session import RuntimeSession


@dataclass(frozen=True)
class EmbeddingRequestPayload:
    inputs: tuple[str, ...]


def handle_embedding_request(
    raw_body: bytes,
    session: RuntimeSession,
) -> tuple[HTTPStatus, dict[str, Any]]:
    try:
        request = decode_embedding_request(raw_body)
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

    return handle_parsed_embedding_request(request, session)


def decode_embedding_request(raw_body: bytes) -> EmbeddingRequestPayload:
    payload = json.loads(raw_body.decode("utf-8"))
    if not isinstance(payload, dict):
        raise ValueError("request body must be a JSON object")
    return parse_embedding_request(payload)


def handle_parsed_embedding_request(
    request: EmbeddingRequestPayload,
    session: RuntimeSession,
) -> tuple[HTTPStatus, dict[str, Any]]:
    try:
        vectors = session.embed(request.inputs)
    except Exception as exc:  # pragma: no cover - backend runtime surface.
        return embedding_error_response(exc)

    return (
        HTTPStatus.OK,
        {
            "model_ref": getattr(session, "model_ref", None),
            "data": [
                {"index": index, "embedding": vector}
                for index, vector in enumerate(vectors)
            ],
        },
    )


def parse_embedding_request(payload: dict[str, Any]) -> EmbeddingRequestPayload:
    unknown_fields = sorted(set(payload) - {"input"})
    if unknown_fields:
        fields = ", ".join(f"`{field}`" for field in unknown_fields)
        raise ValueError(f"unsupported embedding request fields: {fields}")

    raw_input = payload.get("input")
    if isinstance(raw_input, str):
        inputs = (raw_input,)
    elif isinstance(raw_input, list):
        inputs = tuple(raw_input)
    else:
        raise ValueError("`input` must be a string or non-empty string array")

    if not inputs:
        raise ValueError("`input` must not be empty")
    if any(not isinstance(item, str) or not item.strip() for item in inputs):
        raise ValueError("`input` strings must not be empty")

    return EmbeddingRequestPayload(inputs=inputs)


def embedding_error_response(exc: Exception) -> tuple[HTTPStatus, dict[str, Any]]:
    if isinstance(exc, NotImplementedError):
        return (
            HTTPStatus.NOT_IMPLEMENTED,
            {"error": "not_implemented", "message": str(exc)},
        )
    if isinstance(exc, ValueError):
        return (
            HTTPStatus.BAD_REQUEST,
            {"error": "invalid_request", "message": str(exc)},
        )
    return (
        HTTPStatus.INTERNAL_SERVER_ERROR,
        {"error": "embedding_failed", "message": str(exc)},
    )
