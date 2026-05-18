"""Provider-backed dataset evaluation reports."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Literal

from tentgent_daemon.runtime.chat import Message

from .provider import (
    DatasetProviderCallRequest,
    DatasetProviderError,
    call_dataset_provider,
    normalize_provider,
)
from .render import parse_record, validate_eval_case
from .schema import render_backend_record

DATASET_EVAL_REPORT_SCHEMA = "tentgent.dataset.eval.report.v1"
DATASET_EVAL_PROMPT_TEMPLATE = "tentgent.dataset.eval.prompt.v1"
DATASET_EVAL_SYSTEM_PROMPT = (
    "You evaluate Tentgent datasets. Return one JSON object that follows the "
    "requested report schema. Do not include Markdown."
)

DatasetEvalSplit = Literal["train", "valid", "test", "eval_cases", "all"]
SPLIT_FILE_NAMES = {
    "train": "train.jsonl",
    "valid": "valid.jsonl",
    "test": "test.jsonl",
    "eval_cases": "eval_cases.jsonl",
}
SEVERITIES = {"blocker", "warning", "info"}


@dataclass(frozen=True)
class SampledRecord:
    split: str
    path: Path
    line: int
    record_id: str
    record: dict[str, Any]
    local_issues: tuple[str, ...]


@dataclass(frozen=True)
class DatasetSample:
    dataset_path: Path
    split: str
    records: tuple[SampledRecord, ...]
    total_records: int
    local_issues: tuple[dict[str, Any], ...]


@dataclass(frozen=True)
class DatasetEvalOutcome:
    provider: str
    model: str
    split: str
    input_path: Path
    output_dir: Path
    report_json_path: Path
    report_md_path: Path
    prompt_path: Path
    raw_output_path: Path
    reviewed_records: int
    total_records: int
    local_issue_count: int
    finding_count: int
    overall_score: int | None
    warnings: tuple[str, ...]


def evaluate_dataset(
    *,
    provider: str,
    model: str,
    dataset_path: Path,
    output_dir: Path,
    split: DatasetEvalSplit = "train",
    max_records: int = 20,
    criteria: str | None = None,
    max_tokens: int | None = None,
    temperature: float | None = 0.0,
    timeout_seconds: float | None = None,
    api_key: str | None = None,
    client: Any | None = None,
) -> DatasetEvalOutcome:
    provider = normalize_provider(provider)
    ensure_empty_output_dir(output_dir)
    sample = load_dataset_sample(dataset_path, split=split, max_records=max_records)
    if not sample.records:
        raise DatasetProviderError("dataset eval found no JSON records to review")

    prompt = build_dataset_eval_prompt(sample, criteria=criteria)
    response = call_dataset_provider(
        DatasetProviderCallRequest(
            provider=provider,
            model=model,
            messages=(
                Message(role="system", content=DATASET_EVAL_SYSTEM_PROMPT),
                Message(role="user", content=prompt),
            ),
            max_tokens=max_tokens,
            temperature=temperature,
            timeout_seconds=timeout_seconds,
        ),
        api_key=api_key,
        client=client,
    )

    warnings: list[str] = []
    try:
        provider_report = parse_provider_eval_report(response.text)
    except DatasetProviderError as exc:
        provider_report = {
            "summary": "Provider output could not be parsed as the requested JSON report.",
            "overall_score": None,
            "findings": [
                {
                    "severity": "warning",
                    "category": "provider_output",
                    "message": str(exc),
                    "recommendation": "Inspect provider-output.raw.txt and rerun with clearer criteria.",
                }
            ],
            "recommendations": ["Inspect provider-output.raw.txt."],
        }
        warnings.append(str(exc))

    report = normalize_eval_report(
        provider_report,
        provider=provider,
        model=model,
        sample=sample,
        criteria=criteria,
        warnings=tuple(warnings),
    )
    return write_dataset_eval_report(
        output_dir=output_dir,
        report=report,
        prompt=prompt,
        raw_output=response.text,
    )


def load_dataset_sample(
    dataset_path: Path,
    *,
    split: DatasetEvalSplit = "train",
    max_records: int = 20,
) -> DatasetSample:
    if max_records <= 0:
        raise DatasetProviderError("max_records must be greater than 0")
    if not dataset_path.exists():
        raise DatasetProviderError(f"dataset path does not exist: {dataset_path}")

    records: list[SampledRecord] = []
    local_issues: list[dict[str, Any]] = []
    total_records = 0
    for split_name, path in dataset_split_paths(dataset_path, split):
        if not path.exists():
            raise DatasetProviderError(f"dataset split file does not exist: {path}")
        with path.open("r", encoding="utf-8") as source:
            for line_number, line in enumerate(source, start=1):
                if not line.strip():
                    continue
                total_records += 1
                try:
                    record = parse_record(line, path, line_number)
                    record_issues = tuple(validate_sample_record(record, split_name))
                except Exception as exc:
                    local_issues.append(issue(path, line_number, split_name, str(exc)))
                    continue

                for message in record_issues:
                    local_issues.append(issue(path, line_number, split_name, message))
                if len(records) < max_records:
                    records.append(
                        SampledRecord(
                            split=split_name,
                            path=path,
                            line=line_number,
                            record_id=record_id(record, line_number),
                            record=record,
                            local_issues=record_issues,
                        )
                    )

    return DatasetSample(
        dataset_path=dataset_path,
        split=split,
        records=tuple(records),
        total_records=total_records,
        local_issues=tuple(local_issues[:50]),
    )


def dataset_split_paths(dataset_path: Path, split: DatasetEvalSplit) -> list[tuple[str, Path]]:
    if dataset_path.is_file():
        return [(infer_split_name(dataset_path), dataset_path)]

    if split == "all":
        return [
            (split_name, dataset_path / file_name)
            for split_name, file_name in SPLIT_FILE_NAMES.items()
            if (dataset_path / file_name).exists()
        ]

    return [(split, dataset_path / SPLIT_FILE_NAMES[split])]


def validate_sample_record(record: dict[str, Any], split: str) -> list[str]:
    try:
        if split == "eval_cases":
            validate_eval_case(record)
        else:
            render_backend_record(record, mask_prompt=False)
            render_backend_record(record, mask_prompt=True)
    except Exception as exc:
        return [str(exc)]
    return []


def build_dataset_eval_prompt(sample: DatasetSample, *, criteria: str | None = None) -> str:
    request = {
        "schema": "tentgent.dataset.eval.request.v1",
        "report_schema": DATASET_EVAL_REPORT_SCHEMA,
        "template": DATASET_EVAL_PROMPT_TEMPLATE,
        "instructions": [
            "Evaluate whether records are useful and safe for model tuning.",
            "Flag language mismatch, hallucination risk, unsafe behavior, refusal mistakes, malformed tool-call semantics, format drift, duplication, and low-quality assistant answers.",
            "If criteria is provided, evaluate those criteria explicitly.",
            "Return only one JSON object with schema, summary, overall_score, findings, and recommendations.",
        ],
        "criteria": criteria or "",
        "sample": {
            "dataset_path": str(sample.dataset_path),
            "requested_split": sample.split,
            "total_records_in_selected_splits": sample.total_records,
            "reviewed_records": len(sample.records),
            "local_issues": list(sample.local_issues),
            "records": [record_payload(record, index) for index, record in enumerate(sample.records, 1)],
        },
        "required_report_shape": {
            "schema": DATASET_EVAL_REPORT_SCHEMA,
            "summary": "short human-readable summary",
            "overall_score": "integer 0-100 or null",
            "findings": [
                {
                    "severity": "blocker|warning|info",
                    "category": "schema|language|style|safety|hallucination|tool|duplication|quality|other",
                    "split": "split name",
                    "line": "source JSONL line number or null",
                    "record_id": "record id or null",
                    "message": "specific issue",
                    "recommendation": "concrete fix",
                }
            ],
            "recommendations": ["next action"],
        },
    }
    return json.dumps(request, ensure_ascii=False, indent=2, sort_keys=True)


def parse_provider_eval_report(text: str) -> dict[str, Any]:
    for candidate in json_candidates(text):
        try:
            parsed = json.loads(candidate)
        except json.JSONDecodeError:
            continue
        if isinstance(parsed, dict):
            return parsed
    raise DatasetProviderError("provider eval output did not contain a JSON object report")


def normalize_eval_report(
    provider_report: dict[str, Any],
    *,
    provider: str,
    model: str,
    sample: DatasetSample,
    criteria: str | None,
    warnings: tuple[str, ...],
) -> dict[str, Any]:
    findings = [normalize_finding(item) for item in list_value(provider_report.get("findings"))]
    recommendations = [
        str(item).strip()
        for item in list_value(provider_report.get("recommendations"))
        if str(item).strip()
    ]
    summary = str(provider_report.get("summary") or "").strip()
    if not summary:
        summary = "Provider did not include a summary."

    return {
        "schema": DATASET_EVAL_REPORT_SCHEMA,
        "generated_by": {
            "provider": provider,
            "provider_model": model,
        },
        "input": {
            "path": str(sample.dataset_path),
            "split": sample.split,
            "criteria": criteria or "",
            "reviewed_records": len(sample.records),
            "total_records": sample.total_records,
        },
        "local_issues": list(sample.local_issues),
        "summary": summary,
        "overall_score": normalize_score(provider_report.get("overall_score")),
        "findings": findings,
        "recommendations": recommendations,
        "warnings": list(warnings),
    }


def write_dataset_eval_report(
    *,
    output_dir: Path,
    report: dict[str, Any],
    prompt: str,
    raw_output: str,
) -> DatasetEvalOutcome:
    ensure_empty_output_dir(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    report_json_path = output_dir / "eval-report.json"
    report_md_path = output_dir / "eval-report.md"
    prompt_path = output_dir / "prompt.md"
    raw_output_path = output_dir / "provider-output.raw.txt"

    report_json_path.write_text(
        json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    report_md_path.write_text(render_markdown_report(report), encoding="utf-8")
    prompt_path.write_text(prompt, encoding="utf-8")
    raw_output_path.write_text(raw_output, encoding="utf-8")

    input_info = dict_value(report.get("input"))
    generated_by = dict_value(report.get("generated_by"))
    findings = list_value(report.get("findings"))
    local_issues = list_value(report.get("local_issues"))
    warnings = tuple(str(item) for item in list_value(report.get("warnings")))
    return DatasetEvalOutcome(
        provider=str(generated_by.get("provider") or ""),
        model=str(generated_by.get("provider_model") or ""),
        split=str(input_info.get("split") or ""),
        input_path=Path(str(input_info.get("path") or "")),
        output_dir=output_dir,
        report_json_path=report_json_path,
        report_md_path=report_md_path,
        prompt_path=prompt_path,
        raw_output_path=raw_output_path,
        reviewed_records=int(input_info.get("reviewed_records") or 0),
        total_records=int(input_info.get("total_records") or 0),
        local_issue_count=len(local_issues),
        finding_count=len(findings),
        overall_score=normalize_score(report.get("overall_score")),
        warnings=warnings,
    )


def render_markdown_report(report: dict[str, Any]) -> str:
    input_info = dict_value(report.get("input"))
    generated_by = dict_value(report.get("generated_by"))
    lines = [
        "# Tentgent Dataset Eval Report",
        "",
        f"- Provider: {generated_by.get('provider', '-')}",
        f"- Model: {generated_by.get('provider_model', '-')}",
        f"- Input: {input_info.get('path', '-')}",
        f"- Split: {input_info.get('split', '-')}",
        f"- Reviewed records: {input_info.get('reviewed_records', 0)} / {input_info.get('total_records', 0)}",
        f"- Overall score: {report.get('overall_score') if report.get('overall_score') is not None else '-'}",
        "",
        "## Summary",
        "",
        str(report.get("summary") or "-"),
        "",
    ]

    local_issues = list_value(report.get("local_issues"))
    if local_issues:
        lines.extend(["## Local Issues", ""])
        for item in local_issues:
            issue = dict_value(item)
            lines.append(
                f"- `{issue.get('split', '-')}` line {issue.get('line', '-')}: {issue.get('message', '-')}"
            )
        lines.append("")

    findings = list_value(report.get("findings"))
    lines.extend(["## Provider Findings", ""])
    if findings:
        for item in findings:
            finding = dict_value(item)
            location = location_label(finding)
            lines.append(
                f"- **{finding.get('severity', 'warning')}** `{finding.get('category', 'other')}` {location}: {finding.get('message', '-')}"
            )
            recommendation = str(finding.get("recommendation") or "").strip()
            if recommendation:
                lines.append(f"  Recommendation: {recommendation}")
    else:
        lines.append("- No provider findings.")
    lines.append("")

    recommendations = list_value(report.get("recommendations"))
    if recommendations:
        lines.extend(["## Recommendations", ""])
        for item in recommendations:
            lines.append(f"- {item}")
        lines.append("")

    warnings = list_value(report.get("warnings"))
    if warnings:
        lines.extend(["## Warnings", ""])
        for item in warnings:
            lines.append(f"- {item}")
        lines.append("")

    return "\n".join(lines)


def json_candidates(text: str) -> list[str]:
    candidates = [
        match.group("body").strip()
        for match in re.finditer(
            r"```(?:json)?\s*\n(?P<body>.*?)```",
            text,
            flags=re.IGNORECASE | re.DOTALL,
        )
        if match.group("body").strip()
    ]
    stripped = text.strip()
    if stripped:
        candidates.append(stripped)
    start = stripped.find("{")
    end = stripped.rfind("}")
    if 0 <= start < end:
        candidates.append(stripped[start : end + 1])
    return candidates


def record_payload(record: SampledRecord, index: int) -> dict[str, Any]:
    return {
        "index": index,
        "split": record.split,
        "path": str(record.path),
        "line": record.line,
        "record_id": record.record_id,
        "local_issues": list(record.local_issues),
        "record": shrink_record(record.record),
    }


def shrink_record(record: dict[str, Any]) -> dict[str, Any]:
    text = json.dumps(record, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    if len(text) <= 4000:
        return record
    return {
        "id": record.get("id"),
        "schema": record.get("schema"),
        "messages": record.get("messages"),
        "metadata": record.get("metadata"),
        "_truncated": True,
    }


def normalize_finding(value: Any) -> dict[str, Any]:
    raw = dict_value(value)
    severity = str(raw.get("severity") or "warning").strip().lower()
    if severity not in SEVERITIES:
        severity = "warning"
    return {
        "severity": severity,
        "category": str(raw.get("category") or "other").strip() or "other",
        "split": optional_string(raw.get("split")),
        "line": normalize_optional_int(raw.get("line")),
        "record_id": optional_string(raw.get("record_id")),
        "message": str(raw.get("message") or "").strip() or "Provider reported an issue.",
        "recommendation": str(raw.get("recommendation") or "").strip(),
    }


def normalize_score(value: Any) -> int | None:
    if value is None:
        return None
    try:
        score = int(round(float(value)))
    except (TypeError, ValueError):
        return None
    return min(100, max(0, score))


def normalize_optional_int(value: Any) -> int | None:
    if value is None:
        return None
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def optional_string(value: Any) -> str | None:
    if value is None:
        return None
    text = str(value).strip()
    return text or None


def record_id(record: dict[str, Any], line_number: int) -> str:
    value = record.get("id")
    if isinstance(value, str) and value.strip():
        return value.strip()
    metadata = record.get("metadata")
    if isinstance(metadata, dict):
        metadata_id = metadata.get("id")
        if isinstance(metadata_id, str) and metadata_id.strip():
            return metadata_id.strip()
    return f"line-{line_number}"


def infer_split_name(path: Path) -> str:
    for split_name, file_name in SPLIT_FILE_NAMES.items():
        if path.name == file_name:
            return split_name
    return "train"


def issue(path: Path, line: int, split: str, message: str) -> dict[str, Any]:
    return {
        "path": str(path),
        "line": line,
        "split": split,
        "message": message,
    }


def ensure_empty_output_dir(output_dir: Path) -> None:
    if not output_dir.exists():
        return
    if not output_dir.is_dir():
        raise DatasetProviderError(f"output path exists but is not a directory: {output_dir}")
    if any(output_dir.iterdir()):
        raise DatasetProviderError(f"output directory must be empty: {output_dir}")


def location_label(finding: dict[str, Any]) -> str:
    split = finding.get("split") or "-"
    line = finding.get("line")
    record = finding.get("record_id")
    pieces = [f"split {split}"]
    if line is not None:
        pieces.append(f"line {line}")
    if record:
        pieces.append(f"record {record}")
    return "(" + ", ".join(pieces) + ")"


def dict_value(value: Any) -> dict[str, Any]:
    return value if isinstance(value, dict) else {}


def list_value(value: Any) -> list[Any]:
    return value if isinstance(value, list) else []
