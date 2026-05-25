from __future__ import annotations

import json
import os
import queue
import re
import subprocess
import sys
import threading
from pathlib import Path
from typing import Any

from tentgent.runtime.backends.lora_tuning import (
    LoraTuningBackendKind,
    LoraTuningBackendModel,
    LoraTuningEvent,
    LoraTuningEventSink,
    LoraTuningRequest,
    LoraTuningResult,
    ensure_lora_trainable_model,
)
from tentgent.runtime.backends.records import ModelRecord
from tentgent.runtime.training.datasets import RenderedDatasetSummary, render_training_dataset

from .base import MlxBackendModel, clear_mlx_cache, require_mlx_model


PARAMS_RE = re.compile(
    r"Trainable parameters:\s+(?P<percent>[0-9.]+)%\s+"
    r"\((?P<trainable>[0-9.]+)M/(?P<total>[0-9.]+)M\)"
)
TRAIN_RE = re.compile(
    r"Iter (?P<step>\d+): Train loss (?P<loss>[0-9.]+), "
    r"Learning Rate (?P<lr>[0-9.eE+-]+), It/sec (?P<it_sec>[0-9.]+), "
    r"Tokens/sec (?P<tokens_sec>[0-9.]+), Trained Tokens (?P<trained>\d+), "
    r"Peak mem (?P<memory>[0-9.]+) GB"
)
EVAL_RE = re.compile(
    r"Iter (?P<step>\d+): Val loss (?P<loss>[0-9.]+), "
    r"Val took (?P<duration>[0-9.]+)s"
)
CHECKPOINT_RE = re.compile(
    r"Iter (?P<step>\d+): Saved adapter weights to (?P<adapter>.+?) "
    r"and (?P<checkpoint>.+)\.$"
)
FINAL_RE = re.compile(r"Saved final weights to (?P<path>.+)\.$")
START_RE = re.compile(r"Starting training\.\.\., iters: (?P<iters>\d+)")


class MlxLoraTuningModel(MlxBackendModel, LoraTuningBackendModel):
    def __init__(self) -> None:
        self._record: ModelRecord | None = None

    def load(self, record: ModelRecord) -> None:
        require_mlx_model(record, "MLX LoRA tuning")
        ensure_lora_trainable_model(record, LoraTuningBackendKind.MLX)
        self._record = record

    @property
    def is_loaded(self) -> bool:
        return self._record is not None

    def release(self) -> None:
        self._record = None
        clear_mlx_cache()

    def run_lora_tuning(
        self,
        request: LoraTuningRequest,
        *,
        emit: LoraTuningEventSink,
    ) -> LoraTuningResult:
        record = self._require_loaded()
        if record.model_ref != request.model.model_ref:
            raise RuntimeError(
                f"loaded MLX model `{record.model_ref}` does not match request "
                f"`{request.model.model_ref}`"
            )

        config_path = write_mlx_config(request=request, run_dir=request.output_dir, emit=emit)
        adapter_path = request.output_dir / "adapter-output"
        command = [sys.executable, "-m", "mlx_lm", "lora", "--config", str(config_path)]

        emit({"type": "stage", "name": "launch_mlx", "status": "started"})
        status = run_mlx_command(
            command,
            cwd=request.output_dir,
            request=request,
            emit=emit,
        )
        adapter_file = adapter_path / "adapters.safetensors"
        if status != 0:
            raise RuntimeError(f"mlx_lm.lora exited with a non-zero status: {status}")

        emit(
            {
                "type": "done",
                "run_ref": request.run_ref,
                "plan_ref": request.plan_ref,
                "backend": LoraTuningBackendKind.MLX.value,
                "adapter_path": str(adapter_path),
                "adapter_file": str(adapter_file),
            }
        )
        return LoraTuningResult(
            backend=LoraTuningBackendKind.MLX,
            model_ref=request.model.model_ref,
            output_dir=request.output_dir,
            adapter_path=adapter_path,
            adapter_file=adapter_file,
        )

    def _require_loaded(self) -> ModelRecord:
        if self._record is None:
            raise RuntimeError("MLX LoRA tuning model is not loaded yet; call load() first.")
        return self._record


def write_mlx_config(
    *,
    request: LoraTuningRequest,
    run_dir: Path,
    emit: LoraTuningEventSink,
) -> Path:
    run_dir.mkdir(parents=True, exist_ok=True)
    adapter_path = run_dir / "adapter-output"
    adapter_path.mkdir(parents=True, exist_ok=True)

    rendered_dataset = render_training_dataset(
        source_dir=request.dataset.source_path,
        output_dir=run_dir / "rendered-data",
        mask_prompt=request.dataset.mask_prompt,
    )
    emit_rendered_dataset_summary(rendered_dataset, emit=emit)

    mlx_config = request.backend_config.mlx
    config = {
        "model": str(request.model.source_path),
        "train": True,
        "test": False,
        "data": str(rendered_dataset.output_dir),
        "fine_tune_type": mlx_config.get("fine_tune_type", "lora"),
        "optimizer": request.optimization.optimizer,
        "seed": request.optimization.seed,
        "num_layers": mlx_config.get("num_layers", 16),
        "batch_size": request.optimization.batch_size,
        "iters": request.optimization.max_steps,
        "val_batches": mlx_config.get("val_batches", 25),
        "learning_rate": request.optimization.learning_rate,
        "steps_per_report": request.checkpoint.log_every_steps,
        "steps_per_eval": request.checkpoint.eval_every_steps,
        "grad_accumulation_steps": request.optimization.gradient_accumulation_steps,
        "adapter_path": str(adapter_path),
        "save_every": request.checkpoint.save_every_steps,
        "test_batches": mlx_config.get("test_batches", 500),
        "max_seq_length": request.dataset.max_seq_length,
        "grad_checkpoint": mlx_config.get("grad_checkpoint", False),
        "mask_prompt": request.dataset.mask_prompt,
        "lora_parameters": lora_parameters(request),
    }
    config_path = run_dir / "mlx-config.yaml"
    config_path.write_text(json.dumps(config, indent=2, sort_keys=True), encoding="utf-8")
    return config_path


def emit_rendered_dataset_summary(
    rendered_dataset: RenderedDatasetSummary,
    *,
    emit: LoraTuningEventSink,
) -> None:
    split_counts = {split.name: split.examples for split in rendered_dataset.splits}
    emit(
        {
            "type": "dataset",
            "backend": LoraTuningBackendKind.MLX.value,
            "train_examples": split_counts.get("train", 0),
            "validation_examples": split_counts.get("valid", 0),
            "test_examples": split_counts.get("test", 0),
            "eval_cases": rendered_dataset.eval_cases,
            "rendered_path": str(rendered_dataset.output_dir),
        }
    )


def lora_parameters(request: LoraTuningRequest) -> dict[str, Any]:
    params: dict[str, Any] = {
        "rank": request.lora.rank,
        "dropout": request.lora.dropout,
        "scale": request.lora.scale,
    }
    if request.lora.target_modules:
        params["keys"] = list(request.lora.target_modules)
    return params


def run_mlx_command(
    command: list[str],
    *,
    cwd: Path,
    request: LoraTuningRequest,
    emit: LoraTuningEventSink,
) -> int:
    process = subprocess.Popen(
        command,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
        env={**os.environ, "PYTHONUNBUFFERED": "1"},
    )

    lines: queue.Queue[tuple[str, str | None]] = queue.Queue()
    threads = [
        threading.Thread(target=read_stream, args=("stdout", process.stdout, lines)),
        threading.Thread(target=read_stream, args=("stderr", process.stderr, lines)),
    ]
    for thread in threads:
        thread.daemon = True
        thread.start()

    live_streams = len(threads)
    while live_streams:
        stream, line = lines.get()
        if line is None:
            live_streams -= 1
            continue
        emit({"type": "raw_log", "backend": "mlx", "stream": stream, "line": line})
        for event in parse_mlx_line(line, request=request):
            emit(event)

    status = process.wait()
    for thread in threads:
        thread.join()
    return int(status)


def read_stream(
    stream_name: str,
    stream: Any,
    lines: queue.Queue[tuple[str, str | None]],
) -> None:
    if stream is None:
        lines.put((stream_name, None))
        return
    for raw in stream:
        for line in split_progress_line(raw):
            lines.put((stream_name, line))
    lines.put((stream_name, None))


def split_progress_line(raw: str) -> list[str]:
    return [part.strip() for part in raw.replace("\r", "\n").splitlines() if part.strip()]


def parse_mlx_line(line: str, *, request: LoraTuningRequest) -> list[LoraTuningEvent]:
    if line == "Loading pretrained model":
        return [{"type": "stage", "name": "load_model", "status": "completed"}]
    if line == "Loading datasets":
        return [{"type": "stage", "name": "load_dataset", "status": "completed"}]
    if match := START_RE.search(line):
        return [
            {
                "type": "stage",
                "name": "train",
                "status": "started",
                "max_steps": int(match["iters"]),
            }
        ]
    if match := PARAMS_RE.search(line):
        return [params_event(match)]
    if match := TRAIN_RE.search(line):
        return [train_event(match, request)]
    if match := EVAL_RE.search(line):
        return [eval_event(match)]
    if match := CHECKPOINT_RE.search(line):
        return [checkpoint_event(match)]
    if match := FINAL_RE.search(line):
        return [
            {
                "type": "checkpoint",
                "step": request.optimization.max_steps,
                "path": match["path"],
                "final": True,
            }
        ]
    return []


def params_event(match: re.Match[str]) -> LoraTuningEvent:
    return {
        "type": "params",
        "trainable": round(float(match["trainable"]) * 1_000_000),
        "total": round(float(match["total"]) * 1_000_000),
        "percent": float(match["percent"]),
        "backend": LoraTuningBackendKind.MLX.value,
    }


def train_event(match: re.Match[str], request: LoraTuningRequest) -> LoraTuningEvent:
    return {
        "type": "train",
        "step": int(match["step"]),
        "max_steps": request.optimization.max_steps,
        "loss": float(match["loss"]),
        "learning_rate": float(match["lr"]),
        "iterations_per_sec": float(match["it_sec"]),
        "tokens_per_sec": float(match["tokens_sec"]),
        "trained_tokens": int(match["trained"]),
        "peak_memory_gb": float(match["memory"]),
    }


def eval_event(match: re.Match[str]) -> LoraTuningEvent:
    return {
        "type": "eval",
        "step": int(match["step"]),
        "loss": float(match["loss"]),
        "duration_sec": float(match["duration"]),
    }


def checkpoint_event(match: re.Match[str]) -> LoraTuningEvent:
    return {
        "type": "checkpoint",
        "step": int(match["step"]),
        "path": match["checkpoint"],
        "adapter_file": match["adapter"],
    }
