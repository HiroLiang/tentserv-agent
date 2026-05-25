from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Protocol


SCHEMA_ID = "tentgent.chat.v1"
IGNORE_INDEX = -100
TRAINING_SPLITS = ("train", "valid", "test")
EVAL_SPLIT = "eval_cases"


class TokenizerLike(Protocol):
    eos_token: str | None

    def __call__(self, text: str, **kwargs: Any) -> Any: ...


@dataclass(frozen=True, slots=True)
class RenderedRecord:
    text: str
    prompt_text: str | None
    completion_text: str | None
    schema: str


@dataclass(frozen=True, slots=True)
class RenderedSplitSummary:
    name: str
    examples: int
    path: Path


@dataclass(frozen=True, slots=True)
class RenderedDatasetSummary:
    output_dir: Path
    splits: tuple[RenderedSplitSummary, ...]
    eval_cases: int


@dataclass(frozen=True, slots=True)
class TokenizedExample:
    input_ids: list[int]
    attention_mask: list[int]
    labels: list[int]

    @property
    def token_count(self) -> int:
        return len(self.input_ids)


@dataclass(frozen=True, slots=True)
class TokenizedSplit:
    name: str
    path: Path
    examples: list[TokenizedExample]
    truncated_count: int

    @property
    def token_count(self) -> int:
        return sum(example.token_count for example in self.examples)


@dataclass(frozen=True, slots=True)
class PeftTokenizedDataset:
    train: TokenizedSplit
    validation: TokenizedSplit | None
    max_seq_length: int
    mask_prompt: bool


def render_training_dataset(
    *,
    source_dir: Path,
    output_dir: Path,
    mask_prompt: bool,
) -> RenderedDatasetSummary:
    output_dir.mkdir(parents=True, exist_ok=True)
    summaries: list[RenderedSplitSummary] = []

    for split in TRAINING_SPLITS:
        source_path = source_dir / f"{split}.jsonl"
        if not source_path.exists():
            continue
        output_path = output_dir / f"{split}.jsonl"
        count = render_training_split(
            source_path,
            output_path,
            mask_prompt=mask_prompt,
        )
        summaries.append(RenderedSplitSummary(split, count, output_path))

    eval_cases = validate_eval_cases(source_dir / f"{EVAL_SPLIT}.jsonl")
    return RenderedDatasetSummary(
        output_dir=output_dir,
        splits=tuple(summaries),
        eval_cases=eval_cases,
    )


def render_training_split(
    source_path: Path,
    output_path: Path,
    *,
    mask_prompt: bool,
) -> int:
    count = 0
    with source_path.open("r", encoding="utf-8") as source, output_path.open(
        "w",
        encoding="utf-8",
    ) as output:
        for line_number, line in enumerate(source, start=1):
            if not line.strip():
                continue
            record = parse_record(line, source_path, line_number)
            rendered = render_backend_record(record, mask_prompt=mask_prompt)
            output.write(json.dumps(rendered, ensure_ascii=False, sort_keys=True) + "\n")
            count += 1
    return count


def prepare_peft_datasets(
    *,
    dataset_dir: Path,
    tokenizer: TokenizerLike,
    max_seq_length: int,
    mask_prompt: bool,
) -> PeftTokenizedDataset:
    train_path = dataset_dir / "train.jsonl"
    if not train_path.exists():
        raise ValueError(f"missing required PEFT train split: {train_path}")

    validation_path = first_existing(dataset_dir / "valid.jsonl", dataset_dir / "val.jsonl")
    return PeftTokenizedDataset(
        train=tokenize_split(
            name="train",
            path=train_path,
            tokenizer=tokenizer,
            max_seq_length=max_seq_length,
            mask_prompt=mask_prompt,
        ),
        validation=tokenize_split(
            name="validation",
            path=validation_path,
            tokenizer=tokenizer,
            max_seq_length=max_seq_length,
            mask_prompt=mask_prompt,
        )
        if validation_path
        else None,
        max_seq_length=max_seq_length,
        mask_prompt=mask_prompt,
    )


def tokenize_split(
    *,
    name: str,
    path: Path,
    tokenizer: TokenizerLike,
    max_seq_length: int,
    mask_prompt: bool,
) -> TokenizedSplit:
    examples: list[TokenizedExample] = []
    truncated_count = 0

    for line_number, record in read_jsonl(path):
        example = tokenize_record(
            record,
            tokenizer=tokenizer,
            max_seq_length=max_seq_length,
            mask_prompt=mask_prompt,
            source=f"{path}:{line_number}",
        )
        if example.token_count >= max_seq_length:
            truncated_count += 1
        examples.append(example)

    if not examples:
        raise ValueError(f"PEFT {name} split is empty: {path}")

    return TokenizedSplit(
        name=name,
        path=path,
        examples=examples,
        truncated_count=truncated_count,
    )


def tokenize_record(
    record: dict[str, Any],
    *,
    tokenizer: TokenizerLike,
    max_seq_length: int,
    mask_prompt: bool,
    source: str,
) -> TokenizedExample:
    if record.get("messages"):
        return tokenize_messages_record(
            record,
            tokenizer=tokenizer,
            max_seq_length=max_seq_length,
            mask_prompt=mask_prompt,
            source=source,
        )
    if "prompt" in record and "completion" in record:
        return tokenize_prompt_completion(
            str(record["prompt"]),
            str(record["completion"]),
            tokenizer=tokenizer,
            max_seq_length=max_seq_length,
            mask_prompt=mask_prompt,
        )
    if "text" in record:
        ids = encode_text(str(record["text"]), tokenizer, max_seq_length)
        return example_from_ids(ids, 0 if mask_prompt else None)

    raise ValueError(f"unsupported PEFT dataset record shape at {source}")


def tokenize_messages_record(
    record: dict[str, Any],
    *,
    tokenizer: TokenizerLike,
    max_seq_length: int,
    mask_prompt: bool,
    source: str,
) -> TokenizedExample:
    rendered = render_record(record)
    mask_until = (
        len(encode_text(rendered.prompt_text, tokenizer, max_seq_length))
        if mask_prompt and rendered.prompt_text
        else None
    )
    ids = encode_text(rendered.text, tokenizer, max_seq_length)
    return example_from_ids(ids, mask_until)


def tokenize_prompt_completion(
    prompt: str,
    completion: str,
    *,
    tokenizer: TokenizerLike,
    max_seq_length: int,
    mask_prompt: bool,
) -> TokenizedExample:
    completion_text = completion + (tokenizer.eos_token or "")
    full_text = prompt + completion_text
    mask_until = len(encode_text(prompt, tokenizer, max_seq_length)) if mask_prompt else None
    ids = encode_text(full_text, tokenizer, max_seq_length)
    return example_from_ids(ids, mask_until)


def example_from_ids(ids: list[int], mask_until: int | None) -> TokenizedExample:
    labels = ids.copy()
    if mask_until is not None:
        labels[: min(mask_until, len(labels))] = [IGNORE_INDEX] * min(
            mask_until,
            len(labels),
        )
    return TokenizedExample(
        input_ids=ids,
        attention_mask=[1] * len(ids),
        labels=labels,
    )


def encode_text(text: str | None, tokenizer: TokenizerLike, max_seq_length: int) -> list[int]:
    encoded = tokenizer(
        text or "",
        add_special_tokens=True,
        truncation=True,
        max_length=max_seq_length,
    )
    return list(encoded["input_ids"])


def render_backend_record(record: dict[str, Any], *, mask_prompt: bool) -> dict[str, str]:
    rendered = render_record(record)
    if mask_prompt:
        if not rendered.prompt_text or rendered.completion_text is None:
            raise ValueError("mask_prompt=true requires a final assistant answer")
        return {"prompt": rendered.prompt_text, "completion": rendered.completion_text}
    return {"text": rendered.text}


def render_record(
    record: dict[str, Any],
    *,
    add_generation_prompt: bool = False,
) -> RenderedRecord:
    messages = validate_messages(
        record.get("messages"),
        source=str(record.get("id") or "record"),
    )
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
            normalized.append(
                validate_assistant_message(item, source, index, seen_tool_calls)
            )
        elif role == "tool":
            normalized.append(validate_tool_message(item, source, index, seen_tool_calls))
        else:
            normalized.append(validate_text_message(item, role, source, index))
    return normalized


def validate_text_message(
    item: dict[str, Any],
    role: str,
    source: str,
    index: int,
) -> dict[str, Any]:
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
        raise ValueError(
            f"assistant content cannot be empty without tool_calls at {source}:{index}"
        )
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
        raise ValueError(
            f"tool message references unknown tool_call_id `{call_id}` at {source}:{index}"
        )
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


def normalize_tool_call(
    raw: Any,
    source: str,
    index: int,
    call_index: int,
) -> dict[str, Any]:
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
        raise ValueError(
            f"tool_call arguments must be an object at {source}:{index}:{call_index}"
        )
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
                f"Tool result {message['tool_call_id']} "
                f"{message['name']} {message['content']}"
            )
        else:
            lines.append(f"{role.capitalize()}: {message.get('content', '')}")
    if add_generation_prompt:
        lines.append("Assistant:")
    return "\n\n".join(lines)


def validate_eval_cases(path: Path) -> int:
    if not path.exists():
        return 0

    count = 0
    with path.open("r", encoding="utf-8") as source:
        for line_number, line in enumerate(source, start=1):
            if not line.strip():
                continue
            record = parse_record(line, path, line_number)
            validate_eval_case(record)
            count += 1
    return count


def validate_eval_case(record: dict[str, Any]) -> None:
    if is_legacy_eval_case(record):
        validate_legacy_eval_case(record)
        return
    render_record(record)


def is_legacy_eval_case(record: dict[str, Any]) -> bool:
    return all(key in record for key in ("case_id", "user_prompt", "expected_behaviors"))


def validate_legacy_eval_case(record: dict[str, Any]) -> None:
    require_non_empty_string(record, "case_id")
    require_non_empty_string(record, "user_prompt")
    validate_optional_string(record, "input_language")
    validate_optional_string_list(record, "tools_available", required=False)
    validate_optional_string_list(record, "expected_behaviors", required=True)


def read_jsonl(path: Path) -> list[tuple[int, dict[str, Any]]]:
    rows: list[tuple[int, dict[str, Any]]] = []
    with path.open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, start=1):
            if not line.strip():
                continue
            rows.append((line_number, parse_record(line, path, line_number)))
    return rows


def parse_record(line: str, path: Path, line_number: int) -> dict[str, Any]:
    record = json.loads(line)
    if not isinstance(record, dict):
        raise ValueError(f"JSONL row must be an object at {path}:{line_number}")
    return record


def require_non_empty_string(record: dict[str, Any], key: str) -> None:
    value = record.get(key)
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"`{key}` must be a non-empty string")


def validate_optional_string(record: dict[str, Any], key: str) -> None:
    value = record.get(key)
    if value is not None and (not isinstance(value, str) or not value.strip()):
        raise ValueError(f"`{key}` must be a non-empty string when present")


def validate_optional_string_list(
    record: dict[str, Any],
    key: str,
    *,
    required: bool,
) -> None:
    value = record.get(key)
    if value is None:
        if required:
            raise ValueError(f"`{key}` is required")
        return
    if not isinstance(value, list):
        raise ValueError(f"`{key}` must be a list")
    if required and not value:
        raise ValueError(f"`{key}` must not be empty")
    for index, item in enumerate(value):
        if not isinstance(item, str) or not item.strip():
            raise ValueError(f"`{key}`[{index}] must be a non-empty string")


def string_content(value: Any) -> str:
    if value is None:
        return ""
    if isinstance(value, str):
        return value.strip()
    return canonical_json(value)


def canonical_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def first_existing(*paths: Path) -> Path | None:
    return next((path for path in paths if path.exists()), None)
