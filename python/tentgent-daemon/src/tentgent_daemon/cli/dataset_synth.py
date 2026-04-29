"""Provider-backed file-first dataset synthesis CLI."""

from __future__ import annotations

import argparse
from dataclasses import dataclass, replace
import json
import os
import sys
from pathlib import Path
from typing import Sequence

from tentgent_daemon.datasets.provider import (
    DatasetJsonlGenerationRequest,
    DatasetProviderError,
    DatasetProviderParseError,
    DatasetProviderRequestError,
    DatasetSplitKind,
    generate_dataset_jsonl,
)
from tentgent_daemon.datasets.synth import (
    DATASET_TEMPLATE_VERSION,
    DatasetSynthSplitInput,
    DatasetSynthSplitOutcome,
    build_dataset_generation_prompt,
    prepare_dataset_synth_output_dir,
    prompt_source,
    split_file_name,
    write_dataset_synth_manifest,
    write_dataset_synth_package,
    write_dataset_synth_split,
)
from tentgent_daemon.providers import ProviderChatError, ProviderRequestError


@dataclass(frozen=True)
class GenerationJob:
    split: DatasetSplitKind
    count: int | None = None


@dataclass(frozen=True)
class DatasetSynthRuntimeOutcome:
    output_dir: Path
    split_path: Path
    manifest_path: Path
    record_count: int
    splits: tuple[DatasetSynthSplitOutcome, ...]


RETRYABLE_GENERATION_ERRORS = (
    DatasetProviderError,
    ProviderChatError,
    TimeoutError,
    OSError,
)


def main(argv: Sequence[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    prompt: str | None = None
    current_split: DatasetSplitKind | None = None

    try:
        source_kind, source_text = resolve_prompt_source(args)
        jobs = generation_jobs(args)
        prompts = tuple(
            (
                job,
                build_dataset_generation_prompt(
                    brief=source_text if source_kind == "brief" else None,
                    spec=source_text if source_kind == "spec" else None,
                    split=job.split,
                    record_count=job.count,
                ),
            )
            for job in jobs
        )
        prompt = prompts[0][1]
        if args.print_prompt:
            print(render_prompts(prompts), end="")
            return 0

        require_generation_args(args)

        def generate_job(
            job: GenerationJob,
            base_prompt: str,
            *,
            index: int,
            total: int,
        ):
            nonlocal current_split, prompt

            current_split = job.split
            attempt_prompt = base_prompt
            max_attempts = args.retries + 1
            for attempt in range(1, max_attempts + 1):
                prompt = attempt_prompt
                emit_progress(
                    args,
                    stage="start",
                    split=job.split,
                    index=index,
                    total=total,
                    attempt=attempt,
                    max_attempts=max_attempts,
                )
                try:
                    response = generate_dataset_jsonl(
                        DatasetJsonlGenerationRequest(
                            provider=args.provider,
                            model=args.model,
                            prompt=attempt_prompt,
                            split=job.split,
                            max_tokens=args.max_tokens,
                            temperature=args.temperature,
                            timeout_seconds=args.timeout_seconds,
                        ),
                        api_key=provider_api_key(args.provider),
                    )
                except RETRYABLE_GENERATION_ERRORS as exc:
                    if attempt >= max_attempts or not is_retryable_generation_error(exc):
                        raise
                    emit_progress(
                        args,
                        stage="retry",
                        split=job.split,
                        index=index,
                        total=total,
                        attempt=attempt + 1,
                        max_attempts=max_attempts,
                        reason=compact_error(str(exc)),
                    )
                    attempt_prompt = build_retry_generation_prompt(
                        base_prompt,
                        split=job.split,
                        error=str(exc),
                    )
                    continue

                if attempt > 1:
                    response = replace(
                        response,
                        warnings=(
                            *response.warnings,
                            f"{job.split} succeeded after {attempt} provider attempt(s)",
                        ),
                    )
                return response

            raise AssertionError("unreachable dataset synth retry loop")

        if len(prompts) == 1:
            job, prompt = prompts[0]
            response = generate_job(job, prompt, index=1, total=1)
            outcome = write_dataset_synth_package(
                output_dir=args.output,
                provider=response.provider,
                model=response.model,
                split=response.split,
                jsonl=response.jsonl,
                record_count=len(response.records),
                prompt_source_kind=source_kind,
                prompt_source_text=source_text,
                prompt_source_path=str(args.spec) if source_kind == "spec" else None,
                warnings=response.warnings,
                max_tokens=args.max_tokens,
                temperature=args.temperature,
                retries=args.retries,
            )
            split_outcomes = outcome.splits
            emit_progress(
                args,
                stage="written",
                split=response.split,
                index=1,
                total=1,
                records=len(response.records),
                path=str(outcome.split_path),
            )
            provider = response.provider
            model = response.model
        else:
            prepare_dataset_synth_output_dir(args.output)
            split_outcomes: list[DatasetSynthSplitOutcome] = []
            provider = args.provider
            model = args.model
            for index, (job, prompt) in enumerate(prompts, start=1):
                response = generate_job(job, prompt, index=index, total=len(prompts))
                provider = response.provider
                model = response.model
                split_outcome = write_dataset_synth_split(
                    args.output,
                    DatasetSynthSplitInput(
                        split=response.split,
                        jsonl=response.jsonl,
                        record_count=len(response.records),
                        warnings=response.warnings,
                    ),
                )
                split_outcomes.append(split_outcome)
                emit_progress(
                    args,
                    stage="written",
                    split=response.split,
                    index=index,
                    total=len(prompts),
                    records=len(response.records),
                    path=str(split_outcome.split_path),
                )
            manifest_path = write_dataset_synth_manifest(
                output_dir=args.output,
                provider=provider,
                model=model,
                split_outcomes=tuple(split_outcomes),
                prompt_source_kind=source_kind,
                prompt_source_text=source_text,
                prompt_source_path=str(args.spec) if source_kind == "spec" else None,
                max_tokens=args.max_tokens,
                temperature=args.temperature,
                retries=args.retries,
            )
            outcome = DatasetSynthRuntimeOutcome(
                output_dir=args.output,
                split_path=split_outcomes[0].split_path,
                manifest_path=manifest_path,
                record_count=sum(split.record_count for split in split_outcomes),
                splits=tuple(split_outcomes),
            )
            emit_progress(args, stage="manifest", path=str(manifest_path))
    except DatasetProviderParseError as exc:
        debug_dir = write_failure_debug(
            args.output,
            split=current_split,
            prompt=prompt,
            raw_text=exc.raw_text,
            error=str(exc),
        )
        print(str(exc), file=sys.stderr)
        if debug_dir is not None:
            print(f"provider debug written to {debug_dir}", file=sys.stderr)
        return 1
    except Exception as exc:
        debug_dir = write_failure_debug(
            args.output,
            split=current_split,
            prompt=prompt,
            raw_text=None,
            error=str(exc),
        )
        print(str(exc), file=sys.stderr)
        if debug_dir is not None:
            print(f"provider debug written to {debug_dir}", file=sys.stderr)
        return 1

    splits = tuple(split_summary(split) for split in outcome.splits)
    print(
        json.dumps(
            {
                "provider": provider,
                "model": model,
                "split": splits[0]["split"] if len(splits) == 1 else "multi",
                "splits": splits,
                "output_dir": str(outcome.output_dir),
                **({"split_path": str(outcome.split_path)} if len(splits) == 1 else {}),
                "manifest_path": str(outcome.manifest_path),
                "record_count": outcome.record_count,
                "template_version": DATASET_TEMPLATE_VERSION,
                "warnings": [
                    warning for split in outcome.splits for warning in split.warnings
                ],
            },
            ensure_ascii=False,
            sort_keys=True,
        )
    )
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--provider", choices=("openai", "anthropic", "claude"))
    parser.add_argument("--model")
    parser.add_argument("--output", type=Path)
    input_group = parser.add_mutually_exclusive_group(required=True)
    input_group.add_argument("--brief")
    input_group.add_argument("--spec", type=Path)
    parser.add_argument(
        "--split",
        default="train",
        choices=("train", "valid", "test", "eval_cases"),
    )
    parser.add_argument("--count", type=positive_int)
    parser.add_argument("--train-count", type=positive_int)
    parser.add_argument("--valid-count", type=positive_int)
    parser.add_argument("--test-count", type=positive_int)
    parser.add_argument("--eval-count", type=positive_int)
    parser.add_argument("--max-tokens", type=int)
    parser.add_argument("--temperature", type=float, default=0.0)
    parser.add_argument("--timeout-seconds", type=float, default=180.0)
    parser.add_argument("--retries", type=non_negative_int, default=1)
    parser.add_argument("--progress-json", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument(
        "--print-prompt",
        action="store_true",
        help="print the exact provider prompt and exit without auth or network calls",
    )
    return parser


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed <= 0:
        raise argparse.ArgumentTypeError("value must be greater than zero")
    return parsed


def non_negative_int(value: str) -> int:
    parsed = int(value)
    if parsed < 0:
        raise argparse.ArgumentTypeError("value must be zero or greater")
    return parsed


def resolve_prompt_source(args: argparse.Namespace) -> tuple[str, str]:
    spec_text = None
    if args.spec is not None:
        spec_text = args.spec.read_text(encoding="utf-8")
    return prompt_source(brief=args.brief, spec=spec_text)


def require_generation_args(args: argparse.Namespace) -> None:
    missing = [
        option
        for option, value in (
            ("--provider", args.provider),
            ("--model", args.model),
            ("--output", args.output),
        )
        if value is None
    ]
    if missing:
        raise DatasetProviderError(
            "dataset synth requires "
            + ", ".join(missing)
            + " unless --print-prompt is used"
        )


def generation_jobs(args: argparse.Namespace) -> tuple[GenerationJob, ...]:
    split_counts = (
        GenerationJob("train", args.train_count),
        GenerationJob("valid", args.valid_count),
        GenerationJob("test", args.test_count),
        GenerationJob("eval_cases", args.eval_count),
    )
    requested = tuple(job for job in split_counts if job.count is not None)
    if requested:
        if args.count is not None:
            raise DatasetProviderError("--count cannot be combined with split-specific counts")
        if args.split != "train":
            raise DatasetProviderError(
                "--split cannot be combined with split-specific counts"
            )
        return requested
    return (GenerationJob(args.split, args.count),)


def render_prompts(prompts: Sequence[tuple[GenerationJob, str]]) -> str:
    if len(prompts) == 1:
        prompt = prompts[0][1]
        return prompt if prompt.endswith("\n") else prompt + "\n"
    parts = []
    for job, prompt in prompts:
        count = f" ({job.count} records)" if job.count is not None else ""
        parts.append(f"# Split: {job.split}{count}\n\n{prompt.rstrip()}\n")
    return "\n---\n\n".join(parts)


def split_summary(split: DatasetSynthSplitOutcome) -> dict[str, object]:
    return {
        "split": split.split,
        "split_path": str(split.split_path),
        "file": split_file_name(split.split),
        "record_count": split.record_count,
        "warnings": list(split.warnings),
    }


def emit_progress(args: argparse.Namespace, **payload: object) -> None:
    if not args.progress_json:
        return
    print(
        json.dumps({"type": "progress", **payload}, ensure_ascii=False, sort_keys=True),
        file=sys.stderr,
        flush=True,
    )


def build_retry_generation_prompt(
    base_prompt: str,
    *,
    split: DatasetSplitKind,
    error: str,
) -> str:
    if split == "eval_cases":
        split_rule = (
            "For eval_cases, keep each record prompt-only under `messages` and "
            "describe checks under `expected_behavior`."
        )
    else:
        split_rule = (
            "For train, valid, and test, every line must put the final assistant "
            "answer inside `messages` as the last message."
        )
    return f"""{base_prompt.rstrip()}

Retry correction:

The previous provider output failed Tentgent local parsing or validation:
{compact_error(error, limit=1000)}

Generate a fresh complete `{split}` JSONL output now.

- Return only JSONL.
- Do not include Markdown fences, comments, partial records, or prose.
- Do not use top-level `completion`, `answer`, `prompt`, `input`, or `output`.
- {split_rule}
- Avoid tool-call examples unless the original user request explicitly asked for tool-use records.
"""


def is_retryable_generation_error(exc: BaseException) -> bool:
    if isinstance(exc, (DatasetProviderRequestError, ProviderRequestError)):
        return False
    message = str(exc).lower()
    if "returned http 4" in message and not any(
        retryable in message
        for retryable in (
            "returned http 408",
            "returned http 409",
            "returned http 425",
            "returned http 429",
        )
    ):
        return False
    return True


def compact_error(error: str, *, limit: int = 240) -> str:
    text = " ".join(error.strip().split())
    if len(text) <= limit:
        return text
    return text[: limit - 3] + "..."


def write_failure_debug(
    output_dir: Path | None,
    *,
    split: DatasetSplitKind | None = None,
    prompt: str | None,
    raw_text: str | None,
    error: str,
) -> Path | None:
    if output_dir is None or (raw_text is None and prompt is None):
        return None
    if output_dir.exists():
        if not output_dir.is_dir() or not safe_debug_output_dir(output_dir):
            return None

    debug_dir = output_dir / "_debug"
    if split is not None:
        debug_dir = debug_dir / split
    debug_dir.mkdir(parents=True, exist_ok=True)
    (debug_dir / "error.txt").write_text(error + "\n", encoding="utf-8")
    if raw_text is not None:
        (debug_dir / "provider-output.raw.txt").write_text(raw_text, encoding="utf-8")
    if prompt is not None:
        (debug_dir / "prompt.md").write_text(prompt, encoding="utf-8")
    return debug_dir


def safe_debug_output_dir(output_dir: Path) -> bool:
    allowed = {
        "_debug",
        "manifest.json",
        "train.jsonl",
        "valid.jsonl",
        "test.jsonl",
        "eval_cases.jsonl",
    }
    return all(child.name in allowed for child in output_dir.iterdir())


def provider_api_key(provider: str) -> str:
    env_name = provider_env_var(provider)
    secret = os.environ.get(env_name, "").strip()
    if not secret:
        raise DatasetProviderError(f"missing provider API key in {env_name}")
    return secret


def provider_env_var(provider: str) -> str:
    match provider:
        case "openai":
            return "OPENAI_API_KEY"
        case "anthropic" | "claude":
            return "ANTHROPIC_API_KEY"
        case _:
            raise DatasetProviderError(f"unsupported provider `{provider}`")


if __name__ == "__main__":
    raise SystemExit(main())
