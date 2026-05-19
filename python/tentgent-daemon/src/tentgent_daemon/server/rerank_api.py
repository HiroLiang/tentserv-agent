from __future__ import annotations

import json
from dataclasses import dataclass
from http import HTTPStatus
from typing import Any

from .session import RuntimeSession


@dataclass(frozen=True)
class RerankRequestPayload:
    query: str
    documents: tuple[str, ...]
    top_n: int | None = None


def handle_rerank_request(
    raw_body: bytes,
    session: RuntimeSession,
) -> tuple[HTTPStatus, dict[str, Any]]:
    try:
        request = decode_rerank_request(raw_body)
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

    return handle_parsed_rerank_request(request, session)


def decode_rerank_request(raw_body: bytes) -> RerankRequestPayload:
    payload = json.loads(raw_body.decode("utf-8"))
    if not isinstance(payload, dict):
        raise ValueError("request body must be a JSON object")
    return parse_rerank_request(payload)


def handle_parsed_rerank_request(
    request: RerankRequestPayload,
    session: RuntimeSession,
) -> tuple[HTTPStatus, dict[str, Any]]:
    try:
        results = session.rerank(request.query, request.documents, request.top_n)
    except Exception as exc:  # pragma: no cover - backend runtime surface.
        return rerank_error_response(exc)

    return (
        HTTPStatus.OK,
        {
            "model_ref": getattr(session, "model_ref", None),
            "data": [
                {"index": item.index, "score": item.score}
                for item in results
            ],
        },
    )


def parse_rerank_request(payload: dict[str, Any]) -> RerankRequestPayload:
    unknown_fields = sorted(set(payload) - {"query", "documents", "top_n"})
    if unknown_fields:
        fields = ", ".join(f"`{field}`" for field in unknown_fields)
        raise ValueError(f"unsupported rerank request fields: {fields}")

    query = payload.get("query")
    if not isinstance(query, str) or not query.strip():
        raise ValueError("`query` must be a non-empty string")

    raw_documents = payload.get("documents")
    if not isinstance(raw_documents, list):
        raise ValueError("`documents` must be a non-empty string array")
    documents = tuple(raw_documents)
    if not documents:
        raise ValueError("`documents` must not be empty")
    if any(not isinstance(document, str) or not document.strip() for document in documents):
        raise ValueError("`documents` strings must not be empty")

    raw_top_n = payload.get("top_n")
    top_n: int | None
    if raw_top_n is None:
        top_n = None
    elif isinstance(raw_top_n, int) and not isinstance(raw_top_n, bool):
        top_n = raw_top_n
    else:
        raise ValueError("`top_n` must be a positive integer")
    if top_n is not None and (top_n < 1 or top_n > len(documents)):
        raise ValueError("`top_n` must be between 1 and document count")

    return RerankRequestPayload(query=query, documents=documents, top_n=top_n)


def rerank_error_response(exc: Exception) -> tuple[HTTPStatus, dict[str, Any]]:
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
        {"error": "rerank_failed", "message": str(exc)},
    )
