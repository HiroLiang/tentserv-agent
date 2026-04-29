"""File-first dataset synthesis helpers."""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Sequence

from .provider import DatasetSplitKind

DATASET_TEMPLATE_VERSION = "tentgent.dataset.synth.v1"
DATASET_SYNTH_MANIFEST_SCHEMA = "tentgent.dataset.synth.manifest.v1"


@dataclass(frozen=True)
class DatasetSynthPackageOutcome:
    output_dir: Path
    split_path: Path
    manifest_path: Path
    record_count: int
    splits: tuple["DatasetSynthSplitOutcome", ...] = ()


@dataclass(frozen=True)
class DatasetSynthSplitInput:
    split: DatasetSplitKind
    jsonl: str
    record_count: int
    warnings: tuple[str, ...]


@dataclass(frozen=True)
class DatasetSynthSplitOutcome:
    split: DatasetSplitKind
    split_path: Path
    record_count: int
    warnings: tuple[str, ...]


def build_dataset_generation_prompt(
    *,
    brief: str | None = None,
    spec: str | None = None,
    split: DatasetSplitKind = "train",
    record_count: int | None = None,
) -> str:
    source_kind, source_text = prompt_source(brief=brief, spec=spec)
    split_rule = split_generation_rule(split)
    format_rule = split_format_rule(split)
    split_examples = split_jsonl_examples(split)
    count_line = f"Requested records: `{record_count}`\n" if record_count is not None else ""
    count_rule = (
        f"Generate exactly {record_count} JSONL record(s)."
        if record_count is not None
        else "Generate the number of records requested by the user."
    )
    return f"""Generate Tentgent dataset JSONL.

Template version: `{DATASET_TEMPLATE_VERSION}`
Target split: `{split}`
{count_line}Input kind: `{source_kind}`

Return only JSONL. Do not wrap the output in Markdown fences. Each line must be one complete JSON object.

Required output rules:

- {count_rule}
- Use `schema: "tentgent.chat.v1"` on every record.
- Use `messages` as the only conversation body.
- Supported message roles are `system`, `user`, `assistant`, and `tool`.
- Use assistant `tool_calls` for tool requests.
- Use `tool` messages for tool results.
- Generate plain assistant-answer records unless the user request explicitly asks for tool-use examples.
- Keep `metadata` factual and non-training-critical.
- Do not output MLX, PEFT, ChatML, OpenAI-specific, or Anthropic-specific rendered prompt text.
- Do not use top-level `completion`, `answer`, `prompt`, `input`, or `output` fields.
- {format_rule}
- Make every line parse as JSON independently.
- {split_rule}

Valid JSONL shape examples for this split:

{split_examples}

Use the examples only as shape references. Generate new records that match the user request.

User request:

{source_text}
"""


def write_dataset_synth_package(
    *,
    output_dir: Path,
    provider: str,
    model: str,
    split: DatasetSplitKind,
    jsonl: str,
    record_count: int,
    prompt_source_kind: str,
    prompt_source_text: str,
    prompt_source_path: str | None,
    warnings: tuple[str, ...],
    max_tokens: int | None,
    temperature: float | None,
    retries: int | None = None,
) -> DatasetSynthPackageOutcome:
    return write_dataset_synth_package_multi(
        output_dir=output_dir,
        provider=provider,
        model=model,
        split_inputs=(
            DatasetSynthSplitInput(
                split=split,
                jsonl=jsonl,
                record_count=record_count,
                warnings=warnings,
            ),
        ),
        prompt_source_kind=prompt_source_kind,
        prompt_source_text=prompt_source_text,
        prompt_source_path=prompt_source_path,
        max_tokens=max_tokens,
        temperature=temperature,
        retries=retries,
    )


def write_dataset_synth_package_multi(
    *,
    output_dir: Path,
    provider: str,
    model: str,
    split_inputs: Sequence[DatasetSynthSplitInput],
    prompt_source_kind: str,
    prompt_source_text: str,
    prompt_source_path: str | None,
    max_tokens: int | None,
    temperature: float | None,
    retries: int | None = None,
) -> DatasetSynthPackageOutcome:
    ensure_empty_output_dir(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    split_outcomes = tuple(
        write_dataset_synth_split(output_dir, split_input) for split_input in split_inputs
    )
    manifest_path = write_dataset_synth_manifest(
        output_dir=output_dir,
        provider=provider,
        model=model,
        split_outcomes=split_outcomes,
        prompt_source_kind=prompt_source_kind,
        prompt_source_text=prompt_source_text,
        prompt_source_path=prompt_source_path,
        max_tokens=max_tokens,
        temperature=temperature,
        retries=retries,
    )
    first_split = split_outcomes[0]
    return DatasetSynthPackageOutcome(
        output_dir=output_dir,
        split_path=first_split.split_path,
        manifest_path=manifest_path,
        record_count=sum(split.record_count for split in split_outcomes),
        splits=split_outcomes,
    )


def prepare_dataset_synth_output_dir(output_dir: Path) -> None:
    ensure_empty_output_dir(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)


def write_dataset_synth_split(
    output_dir: Path,
    split_input: DatasetSynthSplitInput,
) -> DatasetSynthSplitOutcome:
    split_path = output_dir / split_file_name(split_input.split)
    split_path.write_text(split_input.jsonl, encoding="utf-8")
    return DatasetSynthSplitOutcome(
        split=split_input.split,
        split_path=split_path,
        record_count=split_input.record_count,
        warnings=split_input.warnings,
    )


def write_dataset_synth_manifest(
    *,
    output_dir: Path,
    provider: str,
    model: str,
    split_outcomes: Sequence[DatasetSynthSplitOutcome],
    prompt_source_kind: str,
    prompt_source_text: str,
    prompt_source_path: str | None,
    max_tokens: int | None,
    temperature: float | None,
    retries: int | None = None,
) -> Path:
    if not split_outcomes:
        raise ValueError("at least one split is required")
    manifest_path = output_dir / "manifest.json"
    warnings = tuple(
        warning for split in split_outcomes for warning in split.warnings
    )
    manifest: dict[str, Any] = {
        "schema": DATASET_SYNTH_MANIFEST_SCHEMA,
        "generated_by": {
            "provider": provider,
            "provider_model": model,
        },
        "template_version": DATASET_TEMPLATE_VERSION,
        "record_count": sum(split.record_count for split in split_outcomes),
        "splits": {
            split.split: {
                "path": split_file_name(split.split),
                "record_count": split.record_count,
                "warnings": list(split.warnings),
            }
            for split in split_outcomes
        },
        "options": {
            "max_tokens": max_tokens,
            "temperature": temperature,
            "retries": retries,
        },
        "prompt_source": {
            "kind": prompt_source_kind,
            "sha256": sha256_text(prompt_source_text),
            **({"path": prompt_source_path} if prompt_source_path else {}),
        },
        "warnings": list(warnings),
    }
    if len(split_outcomes) == 1:
        manifest["split"] = split_outcomes[0].split
    manifest_path.write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return manifest_path


def prompt_source(*, brief: str | None, spec: str | None) -> tuple[str, str]:
    has_brief = bool(brief and brief.strip())
    has_spec = bool(spec and spec.strip())
    if has_brief == has_spec:
        raise ValueError("exactly one of brief or spec is required")
    if has_brief:
        return "brief", brief.strip()
    return "spec", spec.strip()


def split_generation_rule(split: DatasetSplitKind) -> str:
    if split == "eval_cases":
        return (
            "`eval_cases` should use prompt-only canonical records with "
            "`expected_behavior`; it is not direct trainer input."
        )
    return "Each record must end with a final assistant answer."


def split_format_rule(split: DatasetSplitKind) -> str:
    if split == "eval_cases":
        return (
            "Put the prompt and context under `messages`; describe the desired "
            "result in `expected_behavior` instead of adding a final assistant answer."
        )
    return (
        "Put assistant answers inside `messages` as "
        '`{"role":"assistant","content":"..."}`; never use top-level `completion`.'
    )


def split_jsonl_examples(split: DatasetSplitKind) -> str:
    if split == "eval_cases":
        return (
            '{"schema":"tentgent.chat.v1","id":"eval-example-001",'
            '"messages":[{"role":"user","content":"請用繁體中文簡短說明退款流程。"}],'
            '"expected_behavior":{"answer_language":"zh-TW","checks":["answers the user directly","does not invent policy details"]},'
            '"metadata":{"split":"eval_cases","task":"support","language":"zh-TW"}}'
        )
    return "\n".join(
        (
            '{"schema":"tentgent.chat.v1","id":"example-001",'
            '"messages":[{"role":"user","content":"請用一句話說明退款流程。"},'
            '{"role":"assistant","content":"請先到訂單頁選擇退款原因，再依畫面指示送出申請。"}],'
            f'"metadata":{{"split":"{split}","task":"support","language":"zh-TW"}}}}',
        )
    )


def split_file_name(split: DatasetSplitKind) -> str:
    match split:
        case "train":
            return "train.jsonl"
        case "valid":
            return "valid.jsonl"
        case "test":
            return "test.jsonl"
        case "eval_cases":
            return "eval_cases.jsonl"


def ensure_empty_output_dir(output_dir: Path) -> None:
    if not output_dir.exists():
        return
    if not output_dir.is_dir():
        raise ValueError(f"output path exists but is not a directory: {output_dir}")
    if any(output_dir.iterdir()):
        raise ValueError(f"output directory must be empty: {output_dir}")


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()
