from __future__ import annotations

import itertools
import resource
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from tentgent.runtime.backends.errors import missing_backend_dependency
from tentgent.runtime.backends.lora_tuning import (
    LoraTuningBackendKind,
    LoraTuningBackendModel,
    LoraTuningEventSink,
    LoraTuningRequest,
    LoraTuningResult,
    ensure_lora_trainable_model,
)
from tentgent.runtime.backends.records import ModelRecord
from tentgent.runtime.training.datasets import (
    IGNORE_INDEX,
    PeftTokenizedDataset,
    TokenizedExample,
    prepare_peft_datasets,
)

from .base import (
    TransformersBackendModel,
    clear_torch_device_cache,
    require_safetensors_model,
)


@dataclass(frozen=True, slots=True)
class TrainBatch:
    input_ids: Any
    attention_mask: Any
    labels: Any
    token_count: int


class TransformersPeftLoraTuningModel(
    TransformersBackendModel,
    LoraTuningBackendModel,
):
    def __init__(self) -> None:
        self._record: ModelRecord | None = None
        self._torch: Any | None = None

    def load(self, record: ModelRecord) -> None:
        require_safetensors_model(record, "PEFT LoRA tuning")
        ensure_lora_trainable_model(record, LoraTuningBackendKind.PEFT)
        self._record = record

    @property
    def is_loaded(self) -> bool:
        return self._record is not None

    def release(self) -> None:
        self._record = None
        if self._torch is not None:
            clear_torch_device_cache(self._torch)
        self._torch = None

    def run_lora_tuning(
        self,
        request: LoraTuningRequest,
        *,
        emit: LoraTuningEventSink,
    ) -> LoraTuningResult:
        record = self._require_loaded()
        if record.model_ref != request.model.model_ref:
            raise RuntimeError(
                f"loaded PEFT model `{record.model_ref}` does not match request "
                f"`{request.model.model_ref}`"
            )

        adapter_path = request.output_dir / "adapter-output"
        adapter_path.mkdir(parents=True, exist_ok=True)

        emit({"type": "stage", "name": "launch_peft", "status": "started"})
        tokenizer = load_peft_tokenizer(record.source_path, emit=emit)
        tokenized = prepare_peft_datasets(
            dataset_dir=request.dataset.source_path,
            tokenizer=tokenizer,
            max_seq_length=request.dataset.max_seq_length,
            mask_prompt=request.dataset.mask_prompt,
        )
        emit_peft_dataset_summary(tokenized, emit=emit)
        emit({"type": "stage", "name": "prepare_output", "status": "completed"})

        return run_peft_training(
            request=request,
            tokenizer=tokenizer,
            tokenized=tokenized,
            model_path=record.source_path,
            adapter_path=adapter_path,
            emit=emit,
            torch_loaded=self._remember_torch,
        )

    def _require_loaded(self) -> ModelRecord:
        if self._record is None:
            raise RuntimeError("PEFT LoRA tuning model is not loaded yet; call load() first.")
        return self._record

    def _remember_torch(self, torch: Any) -> None:
        self._torch = torch


def load_peft_tokenizer(model_path: Path, *, emit: LoraTuningEventSink) -> Any:
    try:
        from transformers import AutoTokenizer
    except ModuleNotFoundError as exc:
        if exc.name == "transformers":
            raise missing_backend_dependency(exc.name) from exc
        raise

    tokenizer = AutoTokenizer.from_pretrained(str(model_path), trust_remote_code=True)
    if tokenizer.pad_token_id is None and tokenizer.eos_token_id is not None:
        tokenizer.pad_token = tokenizer.eos_token
    emit({"type": "stage", "name": "load_tokenizer", "status": "completed"})
    return tokenizer


def emit_peft_dataset_summary(
    tokenized: PeftTokenizedDataset,
    *,
    emit: LoraTuningEventSink,
) -> None:
    validation_examples = len(tokenized.validation.examples) if tokenized.validation else 0
    validation_tokens = tokenized.validation.token_count if tokenized.validation else 0
    truncated = tokenized.train.truncated_count
    if tokenized.validation:
        truncated += tokenized.validation.truncated_count

    emit(
        {
            "type": "dataset",
            "backend": LoraTuningBackendKind.PEFT.value,
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


def run_peft_training(
    *,
    request: LoraTuningRequest,
    tokenizer: Any,
    tokenized: PeftTokenizedDataset,
    model_path: Path,
    adapter_path: Path,
    emit: LoraTuningEventSink,
    torch_loaded: Any | None = None,
) -> LoraTuningResult:
    try:
        import torch
        from peft import get_peft_model
        from transformers import AutoModelForCausalLM
    except ModuleNotFoundError as exc:
        if exc.name in {"torch", "peft", "transformers"}:
            raise missing_backend_dependency(exc.name) from exc
        raise

    if callable(torch_loaded):
        torch_loaded(torch)

    peft_config = request.backend_config.peft
    if peft_config.get("load_in_4bit") or peft_config.get("load_in_8bit"):
        raise RuntimeError("PEFT quantized loading is not supported in the minimal loop yet")
    if peft_config.get("save_safetensors") is False:
        raise RuntimeError("PEFT adapter import requires adapter_model.safetensors")

    device = detect_device(torch)
    torch.manual_seed(request.optimization.seed)

    emit({"type": "stage", "name": "load_model", "status": "started", "backend": "peft"})
    model = AutoModelForCausalLM.from_pretrained(
        str(model_path),
        trust_remote_code=True,
        torch_dtype=torch_dtype(torch, peft_config.get("torch_dtype")),
    )
    model.to(device)
    if peft_config.get("gradient_checkpointing") and hasattr(
        model,
        "gradient_checkpointing_enable",
    ):
        model.gradient_checkpointing_enable()
        if hasattr(model, "config"):
            model.config.use_cache = False

    model = get_peft_model(model, lora_config(request))
    model.train()
    emit({"type": "stage", "name": "load_model", "status": "completed", "backend": "peft"})
    emit_params(model, emit=emit)

    optimizer = torch.optim.AdamW(
        (param for param in model.parameters() if param.requires_grad),
        lr=request.optimization.learning_rate,
        weight_decay=request.optimization.weight_decay,
    )

    max_steps = request.optimization.max_steps
    batch_size = request.optimization.batch_size
    grad_accum = max(1, request.optimization.gradient_accumulation_steps)
    log_every = max(1, request.checkpoint.log_every_steps)
    eval_every = max(1, request.checkpoint.eval_every_steps)
    save_every = max(1, request.checkpoint.save_every_steps)
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
            emit_train(
                step,
                max_steps,
                loss_total,
                optimizer,
                elapsed,
                step_tokens,
                trained_tokens,
                peak_memory,
                emit=emit,
            )
        if tokenized.validation and (
            step == 1 or step % eval_every == 0 or step == max_steps
        ):
            emit_eval(
                model,
                tokenized.validation.examples,
                tokenizer,
                torch,
                device,
                step,
                batch_size,
                emit=emit,
            )
        if step % save_every == 0 and step != max_steps:
            save_checkpoint(model, request.output_dir, step, emit=emit)

    adapter_path.mkdir(parents=True, exist_ok=True)
    model.save_pretrained(adapter_path, safe_serialization=True)
    tokenizer.save_pretrained(adapter_path)
    adapter_file = adapter_path / "adapter_model.safetensors"
    emit(
        {
            "type": "checkpoint",
            "step": max_steps,
            "path": str(adapter_path),
            "adapter_file": str(adapter_file),
            "final": True,
        }
    )
    emit(
        {
            "type": "done",
            "run_ref": request.run_ref,
            "plan_ref": request.plan_ref,
            "backend": LoraTuningBackendKind.PEFT.value,
            "adapter_path": str(adapter_path),
            "adapter_file": str(adapter_file),
        }
    )
    return LoraTuningResult(
        backend=LoraTuningBackendKind.PEFT,
        model_ref=request.model.model_ref,
        output_dir=request.output_dir,
        adapter_path=adapter_path,
        adapter_file=adapter_file,
    )


def lora_config(request: LoraTuningRequest) -> Any:
    try:
        from peft import LoraConfig, TaskType
    except ModuleNotFoundError as exc:
        if exc.name == "peft":
            raise missing_backend_dependency(exc.name) from exc
        raise

    return LoraConfig(
        task_type=TaskType.CAUSAL_LM,
        r=request.lora.rank,
        lora_alpha=request.lora.alpha or request.lora.rank * 2,
        lora_dropout=request.lora.dropout,
        target_modules=list(request.lora.target_modules) or None,
    )


def batch_cursor(examples: list[TokenizedExample], batch_size: int) -> Any:
    iterator = itertools.cycle(examples)
    while True:
        yield [next(iterator) for _ in range(batch_size)]


def collate(
    examples: list[TokenizedExample],
    tokenizer: Any,
    torch: Any,
    device: Any,
) -> TrainBatch:
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


def emit_params(model: Any, *, emit: LoraTuningEventSink) -> None:
    trainable = sum(param.numel() for param in model.parameters() if param.requires_grad)
    total = sum(param.numel() for param in model.parameters())
    emit(
        {
            "type": "params",
            "backend": LoraTuningBackendKind.PEFT.value,
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
    *,
    emit: LoraTuningEventSink,
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


def emit_eval(
    model: Any,
    examples: list[TokenizedExample],
    tokenizer: Any,
    torch: Any,
    device: Any,
    step: int,
    batch_size: int,
    *,
    emit: LoraTuningEventSink,
) -> None:
    started = time.perf_counter()
    losses: list[float] = []
    model.eval()
    with torch.no_grad():
        for offset in range(0, len(examples), batch_size):
            batch = collate(
                examples[offset : offset + batch_size],
                tokenizer,
                torch,
                device,
            )
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


def save_checkpoint(
    model: Any,
    run_dir: Path,
    step: int,
    *,
    emit: LoraTuningEventSink,
) -> None:
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
    usage = resource.getrusage(resource.RUSAGE_SELF)
    max_rss = float(usage.ru_maxrss)
    if sys.platform != "darwin":
        max_rss *= 1024.0
    return max_rss / 1_000_000_000
