"""File-first dataset synthesis helpers."""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .provider import DatasetSplitKind

DATASET_TEMPLATE_VERSION = "tentgent.dataset.synth.v1"
DATASET_SYNTH_MANIFEST_SCHEMA = "tentgent.dataset.synth.manifest.v1"


@dataclass(frozen=True)
class DatasetSynthPackageOutcome:
    output_dir: Path
    split_path: Path
    manifest_path: Path
    record_count: int


def build_dataset_generation_prompt(
    *,
    brief: str | None = None,
    spec: str | None = None,
    split: DatasetSplitKind = "train",
) -> str:
    source_kind, source_text = prompt_source(brief=brief, spec=spec)
    split_rule = split_generation_rule(split)
    return f"""Generate Tentgent dataset JSONL.

Template version: `{DATASET_TEMPLATE_VERSION}`
Target split: `{split}`
Input kind: `{source_kind}`

Return only JSONL. Do not wrap the output in Markdown fences. Each line must be one complete JSON object.

Required output rules:

- Use `schema: "tentgent.chat.v1"` on every record.
- Use `messages` as the only training conversation body.
- Supported message roles are `system`, `user`, `assistant`, and `tool`.
- Use assistant `tool_calls` for tool requests.
- Use `tool` messages for tool results.
- Keep `metadata` factual and non-training-critical.
- Do not output MLX, PEFT, ChatML, OpenAI-specific, or Anthropic-specific rendered prompt text.
- Make every line parse as JSON independently.
- {split_rule}

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
) -> DatasetSynthPackageOutcome:
    ensure_empty_output_dir(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    split_path = output_dir / split_file_name(split)
    split_path.write_text(jsonl, encoding="utf-8")

    manifest_path = output_dir / "manifest.json"
    manifest = {
        "schema": DATASET_SYNTH_MANIFEST_SCHEMA,
        "generated_by": {
            "provider": provider,
            "provider_model": model,
        },
        "template_version": DATASET_TEMPLATE_VERSION,
        "split": split,
        "record_count": record_count,
        "options": {
            "max_tokens": max_tokens,
            "temperature": temperature,
        },
        "prompt_source": {
            "kind": prompt_source_kind,
            "sha256": sha256_text(prompt_source_text),
            **({"path": prompt_source_path} if prompt_source_path else {}),
        },
        "warnings": list(warnings),
    }
    manifest_path.write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )

    return DatasetSynthPackageOutcome(
        output_dir=output_dir,
        split_path=split_path,
        manifest_path=manifest_path,
        record_count=record_count,
    )


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
            "`eval_cases` may contain prompt-only canonical records with "
            "`expected_behavior`, or legacy local eval cases with `case_id`, "
            "`user_prompt`, and `expected_behaviors`."
        )
    return "Each record must end with a final assistant answer."


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
