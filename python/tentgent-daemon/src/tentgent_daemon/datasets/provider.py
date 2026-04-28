"""Provider-backed dataset generation helpers.

This module keeps dataset prompting and output parsing separate from provider
transport code. OpenAI and Anthropic HTTP details stay in
``tentgent_daemon.providers``.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from typing import Any, Literal

from tentgent_daemon.providers import (
    ProviderChatClient,
    ProviderChatRequest,
    UrlLibProviderTransport,
    create_provider_chat_client,
)
from tentgent_daemon.runtime.chat import Message

from .render import validate_eval_case
from .schema import render_backend_record

DATASET_PROVIDER_SYSTEM_PROMPT = (
    "You help generate Tentgent dataset artifacts. Return only the requested "
    "artifact content and avoid commentary unless explicitly asked."
)

DatasetSplitKind = Literal["train", "valid", "test", "eval_cases"]


@dataclass(frozen=True)
class DatasetProviderCallRequest:
    provider: str
    model: str
    messages: tuple[Message, ...]
    max_tokens: int | None = None
    temperature: float | None = None
    timeout_seconds: float | None = None


@dataclass(frozen=True)
class DatasetProviderCallResponse:
    provider: str
    model: str
    text: str


@dataclass(frozen=True)
class DatasetJsonlGenerationRequest:
    provider: str
    model: str
    prompt: str
    split: DatasetSplitKind = "train"
    max_tokens: int | None = None
    temperature: float | None = 0.0
    timeout_seconds: float | None = None


@dataclass(frozen=True)
class ParsedDatasetJsonl:
    records: tuple[dict[str, Any], ...]
    jsonl: str
    warnings: tuple[str, ...]


@dataclass(frozen=True)
class DatasetJsonlGenerationResponse:
    provider: str
    model: str
    split: DatasetSplitKind
    raw_text: str
    records: tuple[dict[str, Any], ...]
    jsonl: str
    warnings: tuple[str, ...]


class DatasetProviderError(Exception):
    """Base error for provider-backed dataset helpers."""


class DatasetProviderRequestError(DatasetProviderError):
    """The provider dataset request is invalid before transport."""


class DatasetProviderParseError(DatasetProviderError):
    """The provider returned content that cannot be used as a dataset."""


def call_dataset_provider(
    request: DatasetProviderCallRequest,
    *,
    api_key: str | None = None,
    client: ProviderChatClient | None = None,
) -> DatasetProviderCallResponse:
    provider = normalize_provider(request.provider)
    model = require_non_empty(request.model, "model")
    if not request.messages:
        raise DatasetProviderRequestError("provider dataset request requires at least one message")

    chat_client = client
    if chat_client is None:
        secret = require_non_empty(api_key or "", "api_key")
        chat_client = create_provider_chat_client(
            provider,
            secret,
            transport=UrlLibProviderTransport(
                timeout_seconds=request.timeout_seconds
                if request.timeout_seconds is not None
                else 180.0
            ),
        )

    response = chat_client.generate(
        ProviderChatRequest(
            model=model,
            messages=request.messages,
            max_tokens=request.max_tokens,
            temperature=request.temperature,
        )
    )
    return DatasetProviderCallResponse(provider=provider, model=model, text=response.text)


def generate_dataset_jsonl(
    request: DatasetJsonlGenerationRequest,
    *,
    api_key: str | None = None,
    client: ProviderChatClient | None = None,
) -> DatasetJsonlGenerationResponse:
    prompt = require_non_empty(request.prompt, "prompt")
    call_response = call_dataset_provider(
        DatasetProviderCallRequest(
            provider=request.provider,
            model=request.model,
            messages=(
                Message(role="system", content=DATASET_PROVIDER_SYSTEM_PROMPT),
                Message(role="user", content=prompt),
            ),
            max_tokens=request.max_tokens,
            temperature=request.temperature,
            timeout_seconds=request.timeout_seconds,
        ),
        api_key=api_key,
        client=client,
    )
    parsed = parse_dataset_jsonl(call_response.text, split=request.split)
    return DatasetJsonlGenerationResponse(
        provider=call_response.provider,
        model=call_response.model,
        split=request.split,
        raw_text=call_response.text,
        records=parsed.records,
        jsonl=parsed.jsonl,
        warnings=parsed.warnings,
    )


def parse_dataset_jsonl(text: str, *, split: DatasetSplitKind = "train") -> ParsedDatasetJsonl:
    text = text.strip()
    if not text:
        raise DatasetProviderParseError("provider output must not be empty")
    candidates = fenced_blocks(text)
    if candidates:
        errors: list[str] = []
        for candidate in candidates:
            try:
                return parse_jsonl_candidate(candidate, split=split, strict=True)
            except DatasetProviderParseError as exc:
                errors.append(str(exc))
        raise DatasetProviderParseError(
            "provider fenced output did not contain valid dataset JSONL: "
            + "; ".join(errors)
        )

    return parse_jsonl_candidate(text, split=split, strict=False)


def parse_jsonl_candidate(
    text: str,
    *,
    split: DatasetSplitKind,
    strict: bool,
) -> ParsedDatasetJsonl:
    records: list[dict[str, Any]] = []
    warnings: list[str] = []
    ignored_lines = 0

    for line_number, line in enumerate(text.splitlines(), start=1):
        stripped = line.strip()
        if not stripped:
            continue
        if not looks_like_json_object(stripped):
            if strict:
                raise DatasetProviderParseError(
                    f"expected JSON object line at provider output line {line_number}"
                )
            ignored_lines += 1
            continue

        try:
            record = json.loads(stripped)
        except json.JSONDecodeError as exc:
            raise DatasetProviderParseError(
                f"invalid JSON at provider output line {line_number}: {exc}"
            ) from exc
        if not isinstance(record, dict):
            raise DatasetProviderParseError(
                f"provider output line {line_number} must decode to a JSON object"
            )
        validate_provider_record(record, split=split, line_number=line_number)
        records.append(record)

    if not records:
        raise DatasetProviderParseError("provider output did not contain any JSONL records")
    if ignored_lines:
        warnings.append(f"ignored {ignored_lines} non-JSON provider output line(s)")

    return ParsedDatasetJsonl(
        records=tuple(records),
        jsonl=records_to_jsonl(records),
        warnings=tuple(warnings),
    )


def validate_provider_record(
    record: dict[str, Any],
    *,
    split: DatasetSplitKind,
    line_number: int,
) -> None:
    try:
        if split == "eval_cases":
            validate_eval_case(record)
        else:
            render_backend_record(record, mask_prompt=False)
            render_backend_record(record, mask_prompt=True)
    except Exception as exc:
        raise DatasetProviderParseError(
            f"provider output line {line_number} is not {split}-compatible: {exc}"
        ) from exc


def records_to_jsonl(records: list[dict[str, Any]] | tuple[dict[str, Any], ...]) -> str:
    return "".join(
        json.dumps(record, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n"
        for record in records
    )


def fenced_blocks(text: str) -> list[str]:
    return [
        match.group("body").strip()
        for match in re.finditer(
            r"```(?:jsonl|json|text)?\s*\n(?P<body>.*?)```",
            text,
            flags=re.IGNORECASE | re.DOTALL,
        )
        if match.group("body").strip()
    ]


def looks_like_json_object(line: str) -> bool:
    return line.startswith("{") and line.endswith("}")


def normalize_provider(provider: str) -> str:
    provider = require_non_empty(provider, "provider")
    if provider == "claude":
        return "anthropic"
    return provider


def require_non_empty(value: str, label: str) -> str:
    value = value.strip()
    if not value:
        raise DatasetProviderRequestError(f"{label} must not be empty")
    return value
