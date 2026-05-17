# LoRA Training

This document defines the managed LoRA training-plan and training-run boundary.

## Command Shape

Implemented plan commands:

```text
tentgent train lora plan create --model <MODEL_REF> --dataset <DATASET_REF> [--backend <BACKEND>] [--review] [--interactive]
tentgent train lora plan ls
tentgent train lora plan inspect <PLAN_REF>
tentgent train lora plan rm <PLAN_REF>
```

Implemented execution scaffold:

```text
tentgent train lora run <PLAN_REF>
tentgent train lora run-worker --home <HOME> --run-ref <RUN_REF>
```

The `run-worker` command is hidden. It is an internal detached worker entry for
the HTTP daemon and should not be treated as user-facing CLI.

Kernel migration state:

- CLI plan management now uses `tentgent-kernel` train use cases for plan
  preview/create/list/inspect/remove.
- CLI run startup and foreground execution use `tentgent-kernel` train use
  cases for durable run records and adapter use cases for successful adapter
  imports.
- HTTP train routes are intentionally left on the legacy path until the CLI
  migration is complete.

Current slice memory:

- Slice 1: Rust run orchestration, durable run artifacts, clean CLI events, Python skeleton runner
- Slice 2: MLX runner emits real Tentgent events through `mlx_lm.lora`
- Slice 3a: PEFT runner preflight routes `safetensors` plans to a dedicated backend without creating fake adapters
- Slice 3b: PEFT runner loads tokenizer, reads `train.jsonl` plus optional validation split, and builds causal-LM labels
- Slice 3c: PEFT runner runs a minimal Transformers plus PEFT training loop and emits train, eval, checkpoint, memory, and done events
- Slice 4: successful MLX and PEFT runs import adapters into the adapter store

## Managed Layout

LoRA training state lives under `TENTGENT_HOME/train` unless `TENTGENT_TRAIN_DIR` is set.

```text
train/
└── lora/
    ├── plans/
    │   └── <plan_ref>/
    │       ├── plan.toml
    │       └── runs/
    │           └── <run_ref>/
    │               ├── run.toml
    │               ├── metrics.jsonl
    │               └── raw.log
    └── staging/
```

Each run creates a new `run_ref`. A successful run creates a new `adapter_ref`;
runs never overwrite prior adapters.

The HTTP daemon refuses to delete plans with existing run records. CLI `plan rm`
may remove the plan directory and stored run records, but it does not remove
adapters already imported into `adapters/store`.

`--review` previews the generated plan and asks before saving. Answering `n` writes nothing.

`--interactive` first previews the generated plan, asks for common override values, then runs the same save review. Pressing Enter keeps the shown value.

## Run Observability

`train lora run <PLAN_REF>` must separate user-facing progress from raw backend logs.

Default CLI output should show:

- stage transitions: resolve plan, load model, load dataset, train, eval, checkpoint, import adapter
- current training progress: step, max steps, elapsed time, and percentage when known
- training metrics: train loss, validation loss, learning rate, iterations per second, tokens per second
- memory signal: current or peak memory when the backend reports it
- parameter summary: trainable parameters and trainable percentage
- final result: run ref, adapter ref, adapter path, and final status
- final summary: peak training memory, final train loss, best eval loss, throughput, and trained tokens when reported

Local run state should record:

- process state: pid, start time, end time, exit code, and signal or interrupt status when available
- identity inputs: plan ref, model ref, dataset ref, backend, recipe hash, and selected config
- disk safety state: runtime home and any preflight free-space warning
- checkpoint state: latest checkpoint step, latest successful checkpoint path, and save interval
- backend environment: Python executable, backend package versions when available, and relevant runtime flags
- adapter import state: imported adapter ref and adapter store path on success

The minimum run artifact contract is:

- `run.toml`: durable run status, refs, backend, timestamps, process info, exit info, paths, and result adapter ref
- `metrics.jsonl`: one JSON object per train, eval, checkpoint, or lifecycle event
- `raw.log`: combined raw backend stdout/stderr for debugging

Persisted run statuses are `starting`, `running`, `succeeded`, and `failed`.
HTTP inspection derives `stale` when a run is recorded as live but the recorded
process is no longer running. `stale` is an effective HTTP status, not a
persisted terminal state.

The HTTP daemon starts LoRA runs through a detached worker process. The worker
uses the saved plan as the source of truth; start requests do not accept run
overrides. `TENTGENT_DAEMON_TOKEN` must not be inherited by the worker. Adapter
import is part of run success: if training completes but adapter import fails,
the run is marked `failed`.

Only one live LoRA run is allowed globally in the daemon MVP. Additional starts
return a conflict until the existing live run reaches a terminal state or is
derived as stale.

Training logs are local diagnostics. They may contain local paths, dataset text,
or backend output, and they are not redacted.

CLI output modes:

- default: clean step lines plus one live progress line for the active long-running stage
- `--verbose`: include eval, checkpoint, and backend summary events in the user-facing stream
- `--debug`: stream raw backend logs in addition to writing `raw.log`

Backends may emit noisy progress bars or logs. Tentgent should parse or summarize known signals and keep raw output out of the default CLI unless a failure occurs.

## PEFT Dataset Tokenization

The canonical chat and tool-use dataset schema is [dataset-schema.md](./dataset-schema.md).

PEFT and MLX should both consume `tentgent.chat.v1` records through the shared Tentgent renderer. They must render the same canonical record consistently, including tool calls and tool results, before applying backend-specific tokenization or file-writing details.

`mask_prompt = true` is the default for new LoRA plans. Records should end with a final assistant answer; prompt and context tokens remain visible to the model but are excluded from loss. Only the final assistant answer is trainable. This keeps role labels, tool context, and generation prompts out of the target output while still providing them as input context. Use `--no-mask-prompt` only when intentionally training a plain full-text continuation dataset.

Legacy `prompt` plus `completion` and plain `text` records may remain accepted for simple datasets, but new generated datasets should use `messages`.

The minimal PEFT loop saves final adapters as `adapter_model.safetensors` so the existing adapter store can import successful runs. `save_safetensors = false` is rejected for now.

## Plan Identity

`plan_ref` is derived from the normalized recipe, not from creation time.

The recipe includes:

- model ref
- dataset ref
- requested backend and selected backend
- profile
- dataset training settings
- shared LoRA settings
- optimization settings
- checkpoint settings
- backend-specific settings
- output adapter name

The initial automatic profile uses model size and detected train-example count. Current profiles are `auto-default`, `auto-lowmem`, and `auto-small-data`.

`plan create` may override selected fields before identity is computed:

- shared: `--max-seq-length`, `--mask-prompt`, `--no-mask-prompt`, `--rank`, `--learning-rate`, `--batch-size`, `--grad-accum`, `--max-steps`, `--seed`
- MLX: `--num-layers`, `--grad-checkpoint`
- PEFT: `--load-in-4bit`, `--load-in-8bit`

Override meanings:

- `--rank`: LoRA adapter capacity
- `--learning-rate`: optimizer step size
- `--batch-size`: examples per training step
- `--grad-accum`: gradient accumulation steps before optimizer update
- `--max-steps`: training step limit
- `--seed`: reproducibility seed
- `--max-seq-length`: token-length cap for training examples
- `--mask-prompt`: keep prompt/context visible to the model but train loss only on assistant output; this is the default for new plans
- `--no-mask-prompt`: train full rendered text, including prompt/context framing tokens
- `--num-layers`: MLX layer count to tune
- `--grad-checkpoint`: MLX memory reduction at a speed cost
- `--load-in-4bit` / `--load-in-8bit`: PEFT base-model quantized loading; mutually exclusive

The recipe excludes display name, creation time, command hint, warnings, blockers, and output paths containing `<RUN_REF>`.

## Backend Selection

`--backend auto` is the default.

Auto-selection rules:

- `model.primary_format = "mlx"` selects `mlx` only when the current platform supports MLX
- `model.primary_format = "safetensors"` selects `peft`
- `model.primary_format = "gguf"` is blocked for LoRA training

Manual backend rules:

- `--backend mlx` requires an `mlx` model and an MLX-capable platform
- `--backend peft` requires a `safetensors` model

Platform capability rules are defined in [platform-backends.md](./platform-backends.md).

## Config Shape

Common plan sections:

- `[dataset]`: split names, example counts, `max_seq_length`, and `mask_prompt`
- `[lora]`: rank, alpha, dropout, scale, and target modules
- `[optimization]`: optimizer, learning rate, batch size, gradient accumulation, max steps, warmup, weight decay, and seed
- `[checkpoint]`: log, eval, save intervals, and save limit
- `[output]`: adapter name and per-run adapter output template

Backend-specific settings live under:

- `[backend_config.mlx]`
- `[backend_config.peft]`

Only the selected backend section is populated.

## Readiness Rules

A LoRA plan is ready only when:

- the model format supports the selected backend
- the dataset is tuning-ready
- the dataset has a train split

Dataset package warnings are surfaced as plan warnings.

## Current Execution Gaps

- PEFT quantized loading flags are planned but not supported by the minimal PEFT loop yet
- automatic resume from checkpoints is not part of the first run slice
