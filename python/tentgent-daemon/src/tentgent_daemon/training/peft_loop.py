"""Minimal Transformers + PEFT LoRA training loop."""

from __future__ import annotations

import itertools
import resource
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .events import emit
from .peft_data import IGNORE_INDEX, PeftTokenizedDataset, TokenizedExample
from ..runtime.profile_deps import missing_profile_dependency


@dataclass(frozen=True)
class TrainBatch:
    input_ids: Any
    attention_mask: Any
    labels: Any
    token_count: int


def run_peft_training(
    *,
    plan: dict[str, Any],
    tokenizer: Any,
    tokenized: PeftTokenizedDataset,
    model_path: Path,
    adapter_path: Path,
    run_dir: Path,
    plan_ref: str,
    run_ref: str,
) -> int:
    try:
        import torch
        from peft import get_peft_model
        from transformers import AutoModelForCausalLM
    except ModuleNotFoundError as exc:
        if exc.name in {"torch", "peft", "transformers"}:
            raise missing_profile_dependency("training", exc.name) from exc
        raise

    peft_config = plan.get("backend_config", {}).get("peft", {})
    if peft_config.get("load_in_4bit") or peft_config.get("load_in_8bit"):
        raise RuntimeError("PEFT quantized loading is not supported in the minimal loop yet")
    if peft_config.get("save_safetensors") is False:
        raise RuntimeError("PEFT adapter import requires adapter_model.safetensors")

    optimization = plan.get("optimization", {})
    checkpoint = plan.get("checkpoint", {})
    device = detect_device(torch)
    torch.manual_seed(int(optimization.get("seed") or 0))

    emit({"type": "stage", "name": "load_model", "status": "started", "backend": "peft"})
    model = AutoModelForCausalLM.from_pretrained(
        str(model_path),
        trust_remote_code=True,
        torch_dtype=torch_dtype(torch, peft_config.get("torch_dtype")),
    )
    model.to(device)
    if peft_config.get("gradient_checkpointing") and hasattr(model, "gradient_checkpointing_enable"):
        model.gradient_checkpointing_enable()
        if hasattr(model, "config"):
            model.config.use_cache = False

    model = get_peft_model(model, lora_config(plan))
    model.train()
    emit({"type": "stage", "name": "load_model", "status": "completed", "backend": "peft"})
    emit_params(model)

    optimizer = torch.optim.AdamW(
        (param for param in model.parameters() if param.requires_grad),
        lr=float(optimization.get("learning_rate") or 2e-4),
        weight_decay=float(optimization.get("weight_decay") or 0.0),
    )

    max_steps = int(optimization.get("max_steps") or 1)
    batch_size = int(optimization.get("batch_size") or 1)
    grad_accum = max(1, int(optimization.get("gradient_accumulation_steps") or 1))
    log_every = max(1, int(checkpoint.get("log_every_steps") or 10))
    eval_every = max(1, int(checkpoint.get("eval_every_steps") or max_steps))
    save_every = max(1, int(checkpoint.get("save_every_steps") or max_steps))
    train_cursor = batch_cursor(tokenized.train.examples, batch_size)

    emit({"type": "stage", "name": "train", "status": "started", "max_steps": max_steps})
    trained_tokens = 0
    peak_memory = 0.0
    for step in range(1, max_steps + 1):
        started = time.perf_counter()
        optimizer.zero_grad(set_to_none=True)
        loss_total = 0.0
        step_tokens = 0

        for _ in range(grad_accum):
            batch = collate(next(train_cursor), tokenizer, torch, device)
            outputs = model(**batch_tensors(batch))
            loss = outputs.loss / grad_accum
            loss.backward()
            loss_total += float(loss.detach().cpu()) * grad_accum
            step_tokens += batch.token_count

        optimizer.step()
        elapsed = max(time.perf_counter() - started, 1e-9)
        trained_tokens += step_tokens
        peak_memory = max(peak_memory, memory_gb(torch, device))

        if step == 1 or step % log_every == 0 or step == max_steps:
            emit_train(step, max_steps, loss_total, optimizer, elapsed, step_tokens, trained_tokens, peak_memory)
        if tokenized.validation and (step == 1 or step % eval_every == 0 or step == max_steps):
            emit_eval(model, tokenized.validation.examples, tokenizer, torch, device, step, batch_size)
        if step % save_every == 0 and step != max_steps:
            save_checkpoint(model, run_dir, step)

    adapter_path.mkdir(parents=True, exist_ok=True)
    model.save_pretrained(adapter_path, safe_serialization=True)
    tokenizer.save_pretrained(adapter_path)
    emit(
        {
            "type": "checkpoint",
            "step": max_steps,
            "path": str(adapter_path),
            "adapter_file": str(adapter_path / "adapter_model.safetensors"),
            "final": True,
        }
    )
    emit(
        {
            "type": "done",
            "run_ref": run_ref,
            "plan_ref": plan_ref,
            "adapter_path": str(adapter_path),
            "adapter_file": str(adapter_path / "adapter_model.safetensors"),
        }
    )
    return 0


def lora_config(plan: dict[str, Any]) -> Any:
    try:
        from peft import LoraConfig, TaskType
    except ModuleNotFoundError as exc:
        if exc.name == "peft":
            raise missing_profile_dependency("training", exc.name) from exc
        raise

    lora = plan.get("lora", {})
    return LoraConfig(
        task_type=TaskType.CAUSAL_LM,
        r=int(lora.get("rank") or 8),
        lora_alpha=int(lora.get("alpha") or (int(lora.get("rank") or 8) * 2)),
        lora_dropout=float(lora.get("dropout") or 0.0),
        target_modules=list(lora.get("target_modules") or []) or None,
    )


def batch_cursor(examples: list[TokenizedExample], batch_size: int) -> Any:
    iterator = itertools.cycle(examples)
    while True:
        yield [next(iterator) for _ in range(batch_size)]


def collate(examples: list[TokenizedExample], tokenizer: Any, torch: Any, device: Any) -> TrainBatch:
    pad_id = tokenizer.pad_token_id if tokenizer.pad_token_id is not None else tokenizer.eos_token_id
    width = max(example.token_count for example in examples)
    input_ids: list[list[int]] = []
    attention_mask: list[list[int]] = []
    labels: list[list[int]] = []
    token_count = 0

    for example in examples:
        pad = width - example.token_count
        input_ids.append(example.input_ids + ([pad_id] * pad))
        attention_mask.append(example.attention_mask + ([0] * pad))
        labels.append(example.labels + ([IGNORE_INDEX] * pad))
        token_count += sum(example.attention_mask)

    return TrainBatch(
        input_ids=torch.tensor(input_ids, dtype=torch.long, device=device),
        attention_mask=torch.tensor(attention_mask, dtype=torch.long, device=device),
        labels=torch.tensor(labels, dtype=torch.long, device=device),
        token_count=token_count,
    )


def batch_tensors(batch: TrainBatch) -> dict[str, Any]:
    return {
        "input_ids": batch.input_ids,
        "attention_mask": batch.attention_mask,
        "labels": batch.labels,
    }


def emit_params(model: Any) -> None:
    trainable = sum(param.numel() for param in model.parameters() if param.requires_grad)
    total = sum(param.numel() for param in model.parameters())
    emit(
        {
            "type": "params",
            "backend": "peft",
            "trainable": trainable,
            "total": total,
            "percent": (trainable / total * 100.0) if total else 0.0,
        }
    )


def emit_train(
    step: int,
    max_steps: int,
    loss: float,
    optimizer: Any,
    elapsed: float,
    step_tokens: int,
    trained_tokens: int,
    peak_memory: float,
) -> None:
    emit(
        {
            "type": "train",
            "step": step,
            "max_steps": max_steps,
            "loss": loss,
            "learning_rate": optimizer.param_groups[0]["lr"],
            "iterations_per_sec": 1.0 / elapsed,
            "tokens_per_sec": step_tokens / elapsed,
            "trained_tokens": trained_tokens,
            "peak_memory_gb": peak_memory,
        }
    )


def emit_eval(model: Any, examples: list[TokenizedExample], tokenizer: Any, torch: Any, device: Any, step: int, batch_size: int) -> None:
    started = time.perf_counter()
    losses: list[float] = []
    model.eval()
    with torch.no_grad():
        for offset in range(0, len(examples), batch_size):
            batch = collate(examples[offset : offset + batch_size], tokenizer, torch, device)
            losses.append(float(model(**batch_tensors(batch)).loss.detach().cpu()))
    model.train()
    emit(
        {
            "type": "eval",
            "step": step,
            "loss": sum(losses) / len(losses),
            "duration_sec": time.perf_counter() - started,
        }
    )


def save_checkpoint(model: Any, run_dir: Path, step: int) -> None:
    path = run_dir / "checkpoints" / f"step-{step:06d}"
    path.mkdir(parents=True, exist_ok=True)
    model.save_pretrained(path, safe_serialization=True)
    emit({"type": "checkpoint", "step": step, "path": str(path)})


def detect_device(torch: Any) -> Any:
    if torch.cuda.is_available():
        return torch.device("cuda")
    if torch.backends.mps.is_available():
        return torch.device("mps")
    return torch.device("cpu")


def torch_dtype(torch: Any, value: Any) -> Any:
    return {
        "float16": torch.float16,
        "fp16": torch.float16,
        "bfloat16": torch.bfloat16,
        "bf16": torch.bfloat16,
        "float32": torch.float32,
        "fp32": torch.float32,
    }.get(str(value or "auto").lower(), "auto")


def memory_gb(torch: Any, device: Any) -> float:
    if device.type == "cuda":
        return float(torch.cuda.max_memory_allocated(device) / 1_000_000_000)
    if device.type == "mps" and hasattr(torch, "mps"):
        return float(torch.mps.current_allocated_memory() / 1_000_000_000)
    return process_peak_memory_gb()


def process_peak_memory_gb() -> float:
    """Return the current process peak RSS in GB for CPU fallback reporting."""
    usage = resource.getrusage(resource.RUSAGE_SELF)
    max_rss = float(usage.ru_maxrss)
    if sys.platform != "darwin":
        max_rss *= 1024.0
    return max_rss / 1_000_000_000
