"""Render canonical Tentgent datasets into backend-compatible files."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .schema import render_backend_record, render_record

TRAINING_SPLITS = ("train", "valid", "test")
EVAL_SPLIT = "eval_cases"


@dataclass(frozen=True)
class RenderedSplitSummary:
    name: str
    examples: int
    path: Path


@dataclass(frozen=True)
class RenderedDatasetSummary:
    output_dir: Path
    splits: tuple[RenderedSplitSummary, ...]
    eval_cases: int


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
        count = render_training_split(source_path, output_path, mask_prompt=mask_prompt)
        summaries.append(RenderedSplitSummary(split, count, output_path))

    eval_cases = validate_eval_cases(source_dir / f"{EVAL_SPLIT}.jsonl")
    return RenderedDatasetSummary(
        output_dir=output_dir,
        splits=tuple(summaries),
        eval_cases=eval_cases,
    )


def render_training_split(source_path: Path, output_path: Path, *, mask_prompt: bool) -> int:
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


def require_non_empty_string(record: dict[str, Any], key: str) -> None:
    value = record.get(key)
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"`{key}` must be a non-empty string")


def validate_optional_string(record: dict[str, Any], key: str) -> None:
    value = record.get(key)
    if value is not None and (not isinstance(value, str) or not value.strip()):
        raise ValueError(f"`{key}` must be a non-empty string when present")


def validate_optional_string_list(record: dict[str, Any], key: str, *, required: bool) -> None:
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


def parse_record(line: str, path: Path, line_number: int) -> dict[str, Any]:
    record = json.loads(line)
    if not isinstance(record, dict):
        raise ValueError(f"JSONL row must be an object at {path}:{line_number}")
    return record
