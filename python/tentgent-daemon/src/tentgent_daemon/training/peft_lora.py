"""PEFT LoRA runner wiring for safetensors-backed training plans."""

from __future__ import annotations

from pathlib import Path
from typing import Any

from .events import emit
from .peft_data import PeftTokenizedDataset, prepare_peft_datasets
from .peft_loop import run_peft_training


def run_peft_lora(
    *,
    plan: dict[str, Any],
    plan_ref: str,
    run_ref: str,
    run_dir: Path,
) -> int:
    adapter_path = run_dir / "adapter-output"
    adapter_path.mkdir(parents=True, exist_ok=True)

    emit({"type": "stage", "name": "launch_peft", "status": "started"})
    model_path = resolve_plan_path(plan, "model", "source_path", "model source")
    if model_path is None:
        return 2
    dataset_path = resolve_plan_path(plan, "dataset", "source_path", "dataset source")
    if dataset_path is None:
        return 2

    try:
        tokenizer = load_tokenizer(model_path)
        tokenized = prepare_peft_datasets(
            dataset_dir=dataset_path,
            tokenizer=tokenizer,
            max_seq_length=int(plan.get("dataset", {}).get("max_seq_length") or 2048),
            mask_prompt=bool(plan.get("dataset", {}).get("mask_prompt") or False),
        )
    except Exception as exc:
        emit(
            {
                "type": "error",
                "backend": "peft",
                "message": f"PEFT dataset/tokenization failed: {exc}",
            }
        )
        return 2

    emit_dataset_summary(tokenized)
    emit({"type": "stage", "name": "prepare_output", "status": "completed"})

    try:
        return run_peft_training(
            plan=plan,
            tokenizer=tokenizer,
            tokenized=tokenized,
            model_path=model_path,
            adapter_path=adapter_path,
            run_dir=run_dir,
            plan_ref=plan_ref,
            run_ref=run_ref,
        )
    except Exception as exc:
        emit(
            {
                "type": "error",
                "backend": "peft",
                "run_ref": run_ref,
                "plan_ref": plan_ref,
                "adapter_path": str(adapter_path),
                "message": f"PEFT training failed: {exc}",
            }
        )
        return 1


def load_tokenizer(model_path: Path) -> Any:
    from transformers import AutoTokenizer

    tokenizer = AutoTokenizer.from_pretrained(str(model_path), trust_remote_code=True)
    if tokenizer.pad_token_id is None and tokenizer.eos_token_id is not None:
        tokenizer.pad_token = tokenizer.eos_token
    emit({"type": "stage", "name": "load_tokenizer", "status": "completed"})
    return tokenizer


def emit_dataset_summary(tokenized: PeftTokenizedDataset) -> None:
    validation_examples = len(tokenized.validation.examples) if tokenized.validation else 0
    validation_tokens = tokenized.validation.token_count if tokenized.validation else 0
    truncated = tokenized.train.truncated_count
    if tokenized.validation:
        truncated += tokenized.validation.truncated_count

    emit(
        {
            "type": "dataset",
            "backend": "peft",
            "train_examples": len(tokenized.train.examples),
            "validation_examples": validation_examples,
            "train_tokens": tokenized.train.token_count,
            "validation_tokens": validation_tokens,
            "truncated_examples": truncated,
            "max_seq_length": tokenized.max_seq_length,
            "mask_prompt": tokenized.mask_prompt,
        }
    )
    emit({"type": "stage", "name": "load_dataset", "status": "completed"})


def resolve_plan_path(
    plan: dict[str, Any],
    section: str,
    key: str,
    label: str,
) -> Path | None:
    value = plan.get(section, {}).get(key)
    if not value:
        emit(
            {
                "type": "error",
                "backend": "peft",
                "message": f"missing {label} path in plan section `{section}`",
            }
        )
        return None

    path = Path(value)
    if not path.exists():
        emit(
            {
                "type": "error",
                "backend": "peft",
                "message": f"{label} path does not exist: {path}",
            }
        )
        return None

    emit(
        {
            "type": "stage",
            "name": f"resolve_{section}",
            "status": "completed",
            "path": str(path),
            "backend": "peft",
        }
    )
    return path
