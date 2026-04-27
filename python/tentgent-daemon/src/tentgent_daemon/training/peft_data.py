"""Dataset loading and tokenization for PEFT LoRA training."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Protocol

from tentgent_daemon.datasets.schema import render_record


IGNORE_INDEX = -100


class TokenizerLike(Protocol):
    eos_token: str | None

    def __call__(self, text: str, **kwargs: Any) -> Any: ...


@dataclass(frozen=True)
class TokenizedExample:
    input_ids: list[int]
    attention_mask: list[int]
    labels: list[int]

    @property
    def token_count(self) -> int:
        return len(self.input_ids)


@dataclass(frozen=True)
class TokenizedSplit:
    name: str
    path: Path
    examples: list[TokenizedExample]
    truncated_count: int

    @property
    def token_count(self) -> int:
        return sum(example.token_count for example in self.examples)


@dataclass(frozen=True)
class PeftTokenizedDataset:
    train: TokenizedSplit
    validation: TokenizedSplit | None
    max_seq_length: int
    mask_prompt: bool


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
        labels[: min(mask_until, len(labels))] = [IGNORE_INDEX] * min(mask_until, len(labels))
    return TokenizedExample(
        input_ids=ids,
        attention_mask=[1] * len(ids),
        labels=labels,
    )


def encode_text(text: str, tokenizer: TokenizerLike, max_seq_length: int) -> list[int]:
    encoded = tokenizer(
        text,
        add_special_tokens=True,
        truncation=True,
        max_length=max_seq_length,
    )
    return list(encoded["input_ids"])


def read_jsonl(path: Path) -> list[tuple[int, dict[str, Any]]]:
    rows: list[tuple[int, dict[str, Any]]] = []
    with path.open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, start=1):
            if not line.strip():
                continue
            record = json.loads(line)
            if not isinstance(record, dict):
                raise ValueError(f"JSONL row must be an object at {path}:{line_number}")
            rows.append((line_number, record))
    return rows


def first_existing(*paths: Path) -> Path | None:
    return next((path for path in paths if path.exists()), None)
