from __future__ import annotations

import json
from collections.abc import Iterator
from dataclasses import dataclass
from typing import Any, Protocol
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

from tentgent_daemon.runtime.chat import Message

DEFAULT_ANTHROPIC_MAX_TOKENS = 1024
OPENAI_CHAT_COMPLETIONS_URL = "https://api.openai.com/v1/chat/completions"
ANTHROPIC_MESSAGES_URL = "https://api.anthropic.com/v1/messages"
ANTHROPIC_VERSION = "2023-06-01"


@dataclass(frozen=True)
class ProviderChatRequest:
    model: str
    messages: tuple[Message, ...]
    max_tokens: int | None = None
    temperature: float | None = None


@dataclass(frozen=True)
class ProviderChatResponse:
    text: str


class ProviderChatError(Exception):
    """Base error for cloud provider chat calls."""


class ProviderRequestError(ProviderChatError):
    """The normalized request cannot be sent to the selected provider."""


class ProviderResponseError(ProviderChatError):
    """The provider returned an unsuccessful or unexpected response."""


class ProviderTransportError(ProviderChatError):
    """The HTTP transport failed before a provider response was available."""


class ProviderTransport(Protocol):
    def post_json(
        self,
        url: str,
        headers: dict[str, str],
        payload: dict[str, Any],
    ) -> tuple[int, dict[str, Any]]:
        ...

    def post_sse_json(
        self,
        url: str,
        headers: dict[str, str],
        payload: dict[str, Any],
    ) -> tuple[int, dict[str, Any] | None, Iterator[dict[str, Any]]]:
        ...


class ProviderChatClient(Protocol):
    def generate(self, request: ProviderChatRequest) -> ProviderChatResponse:
        ...

    def stream_generate(self, request: ProviderChatRequest) -> Iterator[str]:
        ...


class UrlLibProviderTransport:
    def __init__(self, timeout_seconds: float = 60.0) -> None:
        self._timeout_seconds = timeout_seconds

    def post_json(
        self,
        url: str,
        headers: dict[str, str],
        payload: dict[str, Any],
    ) -> tuple[int, dict[str, Any]]:
        body = json.dumps(payload).encode("utf-8")
        request = Request(
            url,
            data=body,
            headers=headers,
            method="POST",
        )

        try:
            with urlopen(request, timeout=self._timeout_seconds) as response:
                return response.status, _decode_json_body(response.read())
        except HTTPError as exc:
            return exc.code, _decode_json_body(exc.read())
        except URLError as exc:
            raise ProviderTransportError(f"provider request failed: {exc}") from exc

    def post_sse_json(
        self,
        url: str,
        headers: dict[str, str],
        payload: dict[str, Any],
    ) -> tuple[int, dict[str, Any] | None, Iterator[dict[str, Any]]]:
        body = json.dumps(payload).encode("utf-8")
        request = Request(
            url,
            data=body,
            headers=headers,
            method="POST",
        )

        try:
            response = urlopen(request, timeout=self._timeout_seconds)
        except HTTPError as exc:
            return exc.code, _decode_json_body(exc.read()), iter(())
        except URLError as exc:
            raise ProviderTransportError(f"provider request failed: {exc}") from exc

        return response.status, None, _iter_sse_json_response(response)


class OpenAIChatClient:
    def __init__(
        self,
        api_key: str,
        transport: ProviderTransport | None = None,
        url: str = OPENAI_CHAT_COMPLETIONS_URL,
    ) -> None:
        self._api_key = _require_secret(api_key, "OpenAI")
        self._transport = transport or UrlLibProviderTransport()
        self._url = url

    def generate(self, request: ProviderChatRequest) -> ProviderChatResponse:
        payload = _openai_payload(request)

        status, body = self._transport.post_json(
            self._url,
            headers={
                "Authorization": f"Bearer {self._api_key}",
                "Content-Type": "application/json",
            },
            payload=payload,
        )
        _ensure_success("OpenAI", status, body)
        return ProviderChatResponse(text=_parse_openai_text(body))

    def stream_generate(self, request: ProviderChatRequest) -> Iterator[str]:
        payload = _openai_payload(request)
        payload["stream"] = True

        status, error_body, events = self._transport.post_sse_json(
            self._url,
            headers={
                "Authorization": f"Bearer {self._api_key}",
                "Content-Type": "application/json",
                "Accept": "text/event-stream",
            },
            payload=payload,
        )
        _ensure_success("OpenAI", status, error_body or {})
        return _iter_openai_stream_text(events)


class AnthropicChatClient:
    def __init__(
        self,
        api_key: str,
        transport: ProviderTransport | None = None,
        url: str = ANTHROPIC_MESSAGES_URL,
    ) -> None:
        self._api_key = _require_secret(api_key, "Anthropic")
        self._transport = transport or UrlLibProviderTransport()
        self._url = url

    def generate(self, request: ProviderChatRequest) -> ProviderChatResponse:
        payload = _anthropic_payload(request)

        status, body = self._transport.post_json(
            self._url,
            headers={
                "x-api-key": self._api_key,
                "anthropic-version": ANTHROPIC_VERSION,
                "Content-Type": "application/json",
            },
            payload=payload,
        )
        _ensure_success("Anthropic", status, body)
        return ProviderChatResponse(text=_parse_anthropic_text(body))

    def stream_generate(self, request: ProviderChatRequest) -> Iterator[str]:
        payload = _anthropic_payload(request)
        payload["stream"] = True

        status, error_body, events = self._transport.post_sse_json(
            self._url,
            headers={
                "x-api-key": self._api_key,
                "anthropic-version": ANTHROPIC_VERSION,
                "Content-Type": "application/json",
                "Accept": "text/event-stream",
            },
            payload=payload,
        )
        _ensure_success("Anthropic", status, error_body or {})
        return _iter_anthropic_stream_text(events)


def create_provider_chat_client(
    provider: str,
    api_key: str,
    transport: ProviderTransport | None = None,
) -> ProviderChatClient:
    match provider:
        case "openai":
            return OpenAIChatClient(api_key, transport=transport)
        case "anthropic" | "claude":
            return AnthropicChatClient(api_key, transport=transport)
        case _:
            raise ProviderRequestError(f"unsupported cloud provider `{provider}`")


def _decode_json_body(body: bytes) -> dict[str, Any]:
    try:
        payload = json.loads(body.decode("utf-8"))
    except json.JSONDecodeError as exc:
        raise ProviderTransportError("provider returned invalid JSON") from exc
    if not isinstance(payload, dict):
        raise ProviderTransportError("provider returned a non-object JSON payload")
    return payload


def _iter_sse_json_response(response: Any) -> Iterator[dict[str, Any]]:
    data_lines: list[str] = []
    try:
        for raw_line in response:
            line = raw_line.decode("utf-8").rstrip("\r\n")
            if not line:
                if data_lines:
                    data = "\n".join(data_lines)
                    data_lines = []
                    if data == "[DONE]":
                        return
                    yield _decode_sse_json_data(data)
                continue
            if line.startswith(":"):
                continue
            if line.startswith("data:"):
                data_lines.append(line.removeprefix("data:").lstrip())

        if data_lines:
            data = "\n".join(data_lines)
            if data != "[DONE]":
                yield _decode_sse_json_data(data)
    finally:
        close = getattr(response, "close", None)
        if callable(close):
            close()


def _decode_sse_json_data(data: str) -> dict[str, Any]:
    try:
        payload = json.loads(data)
    except json.JSONDecodeError as exc:
        raise ProviderTransportError("provider returned invalid SSE JSON") from exc
    if not isinstance(payload, dict):
        raise ProviderTransportError("provider returned a non-object SSE JSON payload")
    return payload


def _require_secret(secret: str, provider_name: str) -> str:
    secret = secret.strip()
    if not secret:
        raise ProviderRequestError(f"{provider_name} API key must not be empty")
    return secret


def _require_model(model: str) -> str:
    model = model.strip()
    if not model:
        raise ProviderRequestError("provider model name must not be empty")
    return model


def _message_payload(message: Message) -> dict[str, str]:
    if message.role not in {"system", "user", "assistant"}:
        raise ProviderRequestError("message role must be one of: system, user, assistant")
    return {"role": message.role, "content": _message_content(message)}


def _message_content(message: Message) -> str:
    content = message.content.strip()
    if not content:
        raise ProviderRequestError("message content must not be empty")
    return content


def _openai_payload(request: ProviderChatRequest) -> dict[str, Any]:
    payload = {
        "model": _require_model(request.model),
        "messages": [_message_payload(message) for message in request.messages],
    }
    _apply_optional_generation_args(payload, request)
    return payload


def _anthropic_payload(request: ProviderChatRequest) -> dict[str, Any]:
    system_messages: list[str] = []
    messages: list[dict[str, str]] = []
    for message in request.messages:
        if message.role == "system":
            system_messages.append(_message_content(message))
        elif message.role in {"user", "assistant"}:
            messages.append(_message_payload(message))
        else:
            raise ProviderRequestError(
                "message role must be one of: system, user, assistant"
            )

    if not messages:
        raise ProviderRequestError(
            "Anthropic requests require at least one user or assistant message"
        )

    payload: dict[str, Any] = {
        "model": _require_model(request.model),
        "messages": messages,
        "max_tokens": (
            request.max_tokens
            if request.max_tokens is not None
            else DEFAULT_ANTHROPIC_MAX_TOKENS
        ),
    }
    if system_messages:
        payload["system"] = "\n\n".join(system_messages)
    if request.temperature is not None:
        payload["temperature"] = request.temperature
    return payload


def _apply_optional_generation_args(
    payload: dict[str, Any],
    request: ProviderChatRequest,
) -> None:
    if request.max_tokens is not None:
        payload["max_tokens"] = request.max_tokens
    if request.temperature is not None:
        payload["temperature"] = request.temperature


def _ensure_success(provider_name: str, status: int, body: dict[str, Any]) -> None:
    if 200 <= status < 300:
        return
    raise ProviderResponseError(
        f"{provider_name} returned HTTP {status}: {_provider_error_detail(body)}"
    )


def _provider_error_detail(body: dict[str, Any]) -> str:
    error = body.get("error")
    if isinstance(error, dict):
        message = error.get("message")
        if isinstance(message, str) and message.strip():
            return message.strip()
    if isinstance(error, str) and error.strip():
        return error.strip()
    message = body.get("message")
    if isinstance(message, str) and message.strip():
        return message.strip()
    return "provider request failed"


def _parse_openai_text(body: dict[str, Any]) -> str:
    choices = body.get("choices")
    if not isinstance(choices, list) or not choices:
        raise ProviderResponseError("OpenAI response did not include any choices")
    first = choices[0]
    if not isinstance(first, dict):
        raise ProviderResponseError("OpenAI response choice is not an object")
    message = first.get("message")
    if not isinstance(message, dict):
        raise ProviderResponseError("OpenAI response choice did not include a message")
    text = _coerce_text_content(message.get("content"))
    if not text:
        raise ProviderResponseError("OpenAI response message content is empty")
    return text


def _parse_anthropic_text(body: dict[str, Any]) -> str:
    content = body.get("content")
    if not isinstance(content, list):
        raise ProviderResponseError("Anthropic response did not include content blocks")
    parts = []
    for block in content:
        if not isinstance(block, dict):
            continue
        if block.get("type") == "text" and isinstance(block.get("text"), str):
            parts.append(block["text"])
    text = "".join(parts).strip()
    if not text:
        raise ProviderResponseError("Anthropic response text content is empty")
    return text


def _iter_openai_stream_text(events: Iterator[dict[str, Any]]) -> Iterator[str]:
    for event in events:
        error = event.get("error")
        if error is not None:
            raise ProviderResponseError(
                f"OpenAI stream error: {_provider_error_detail(event)}"
            )

        choices = event.get("choices")
        if not isinstance(choices, list):
            raise ProviderResponseError("OpenAI stream event did not include choices")
        for choice in choices:
            if not isinstance(choice, dict):
                raise ProviderResponseError("OpenAI stream choice is not an object")
            delta = choice.get("delta")
            if not isinstance(delta, dict):
                continue
            text = _coerce_delta_text_content(delta.get("content"))
            if text:
                yield text


def _iter_anthropic_stream_text(events: Iterator[dict[str, Any]]) -> Iterator[str]:
    for event in events:
        event_type = event.get("type")
        if event_type == "error":
            raise ProviderResponseError(
                f"Anthropic stream error: {_provider_error_detail(event)}"
            )
        if event_type == "content_block_start":
            block = event.get("content_block")
            if isinstance(block, dict) and block.get("type") == "text":
                text = block.get("text")
                if isinstance(text, str) and text:
                    yield text
            continue
        if event_type != "content_block_delta":
            continue

        delta = event.get("delta")
        if not isinstance(delta, dict):
            raise ProviderResponseError("Anthropic stream delta is not an object")
        if delta.get("type") != "text_delta":
            continue
        text = delta.get("text")
        if isinstance(text, str) and text:
            yield text


def _coerce_text_content(content: Any) -> str:
    if isinstance(content, str):
        return content.strip()
    if isinstance(content, list):
        parts = []
        for part in content:
            if isinstance(part, dict) and isinstance(part.get("text"), str):
                parts.append(part["text"])
        return "".join(parts).strip()
    return ""


def _coerce_delta_text_content(content: Any) -> str:
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts = []
        for part in content:
            if isinstance(part, dict) and isinstance(part.get("text"), str):
                parts.append(part["text"])
        return "".join(parts)
    return ""
