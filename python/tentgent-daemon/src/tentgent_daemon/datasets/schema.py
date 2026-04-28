"""Tentgent canonical dataset schema helpers."""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import Any

SCHEMA_ID = "tentgent.chat.v1"


@dataclass(frozen=True)
class RenderedRecord:
    text: str
    prompt_text: str | None
    completion_text: str | None
    schema: str


def render_record(record: dict[str, Any], *, add_generation_prompt: bool = False) -> RenderedRecord:
    messages = validate_messages(record.get("messages"), source=str(record.get("id") or "record"))
    text = render_messages(messages, add_generation_prompt=add_generation_prompt)
    prompt_text = None
    completion_text = None

    if messages[-1]["role"] == "assistant" and messages[-1].get("content"):
        prompt_text = render_messages(messages[:-1], add_generation_prompt=True)
        if text.startswith(prompt_text):
            completion_text = text[len(prompt_text) :].lstrip()

    return RenderedRecord(
        text=text,
        prompt_text=prompt_text,
        completion_text=completion_text,
        schema=str(record.get("schema") or SCHEMA_ID),
    )


def render_backend_record(record: dict[str, Any], *, mask_prompt: bool) -> dict[str, str]:
    rendered = render_record(record)
    if mask_prompt:
        if not rendered.prompt_text or rendered.completion_text is None:
            raise ValueError("mask_prompt=true requires a final assistant answer")
        return {"prompt": rendered.prompt_text, "completion": rendered.completion_text}
    return {"text": rendered.text}


def validate_messages(messages: Any, *, source: str) -> list[dict[str, Any]]:
    if not isinstance(messages, list) or not messages:
        raise ValueError(f"`messages` must be a non-empty list at {source}")

    seen_tool_calls: set[str] = set()
    normalized: list[dict[str, Any]] = []
    for index, item in enumerate(messages):
        if not isinstance(item, dict):
            raise ValueError(f"message must be an object at {source}:{index}")
        role = str(item.get("role", "")).strip().lower()
        if role not in {"system", "user", "assistant", "tool"}:
            raise ValueError(f"unsupported message role `{role}` at {source}:{index}")

        if role == "assistant":
            normalized.append(validate_assistant_message(item, source, index, seen_tool_calls))
        elif role == "tool":
            normalized.append(validate_tool_message(item, source, index, seen_tool_calls))
        else:
            normalized.append(validate_text_message(item, role, source, index))
    return normalized


def validate_text_message(item: dict[str, Any], role: str, source: str, index: int) -> dict[str, Any]:
    content = string_content(item.get("content"))
    if not content:
        raise ValueError(f"{role} content cannot be empty at {source}:{index}")
    return {"role": role, "content": content}


def validate_assistant_message(
    item: dict[str, Any],
    source: str,
    index: int,
    seen_tool_calls: set[str],
) -> dict[str, Any]:
    content = string_content(item.get("content"))
    tool_calls = normalize_tool_calls(item.get("tool_calls") or [], source, index)
    if not content and not tool_calls:
        raise ValueError(f"assistant content cannot be empty without tool_calls at {source}:{index}")
    for call in tool_calls:
        seen_tool_calls.add(call["id"])
    return {"role": "assistant", "content": content, "tool_calls": tool_calls}


def validate_tool_message(
    item: dict[str, Any],
    source: str,
    index: int,
    seen_tool_calls: set[str],
) -> dict[str, Any]:
    call_id = str(item.get("tool_call_id", "")).strip()
    name = str(item.get("name", "")).strip()
    if not call_id or not name:
        raise ValueError(f"tool messages require tool_call_id and name at {source}:{index}")
    if call_id not in seen_tool_calls:
        raise ValueError(f"tool message references unknown tool_call_id `{call_id}` at {source}:{index}")
    return {
        "role": "tool",
        "tool_call_id": call_id,
        "name": name,
        "content": canonical_json(item.get("content")),
    }


def normalize_tool_calls(raw_calls: Any, source: str, index: int) -> list[dict[str, Any]]:
    if not isinstance(raw_calls, list):
        raise ValueError(f"assistant tool_calls must be a list at {source}:{index}")

    calls: list[dict[str, Any]] = []
    seen_ids: set[str] = set()
    for call_index, raw in enumerate(raw_calls):
        call = normalize_tool_call(raw, source, index, call_index)
        if call["id"] in seen_ids:
            raise ValueError(f"duplicate tool_call id `{call['id']}` at {source}:{index}")
        seen_ids.add(call["id"])
        calls.append(call)
    return calls


def normalize_tool_call(raw: Any, source: str, index: int, call_index: int) -> dict[str, Any]:
    if not isinstance(raw, dict):
        raise ValueError(f"tool_call must be an object at {source}:{index}:{call_index}")

    function = raw.get("function")
    body = function if isinstance(function, dict) else raw
    call_id = str(raw.get("id", "")).strip()
    name = str(body.get("name", "")).strip()
    arguments = body.get("arguments", {})
    if isinstance(arguments, str):
        arguments = json.loads(arguments) if arguments.strip() else {}
    if not isinstance(arguments, dict):
        raise ValueError(f"tool_call arguments must be an object at {source}:{index}:{call_index}")
    if not call_id or not name:
        raise ValueError(f"tool_call requires id and name at {source}:{index}:{call_index}")
    return {"id": call_id, "name": name, "arguments": arguments}


def render_messages(messages: list[dict[str, Any]], *, add_generation_prompt: bool) -> str:
    lines: list[str] = []
    for message in messages:
        role = message["role"]
        if role == "assistant" and message.get("tool_calls"):
            content = message.get("content", "")
            if content:
                lines.append(f"Assistant: {content}")
            for call in message["tool_calls"]:
                lines.append(
                    f"Assistant tool_call {call['id']} {call['name']} "
                    f"{canonical_json(call['arguments'])}"
                )
        elif role == "tool":
            lines.append(
                f"Tool result {message['tool_call_id']} {message['name']} {message['content']}"
            )
        else:
            lines.append(f"{role.capitalize()}: {message.get('content', '')}")
    if add_generation_prompt:
        lines.append("Assistant:")
    return "\n\n".join(lines)


def string_content(value: Any) -> str:
    if value is None:
        return ""
    if isinstance(value, str):
        return value.strip()
    return canonical_json(value)


def canonical_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
