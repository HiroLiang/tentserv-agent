# LoRA Training

Train a managed chat model on a managed [dataset](./datasets.md), then select
the imported adapter for chat. MLX chat training requires Apple Silicon macOS;
safetensors causal-LM models use PEFT. GGUF and media-model training are outside
this path. Prepare dependencies with `tentgent runtime bootstrap --profile
training` and check `tentgent doctor`.

## Plan Commands

```bash
tentgent train lora plan create \
  --model <model-ref> --dataset <dataset-ref> --interactive
tentgent train lora plan ls
tentgent train lora plan inspect <plan-ref>
tentgent train lora plan rm <plan-ref>
```

Creating a plan stores configuration without training. `--review` previews and
asks before saving; `--interactive` also prompts for common overrides.
`--backend auto` selects a compatible backend from the model and platform.
Defaults depend on the selected profile; inspect the plan for resolved values.
Changing only its name reuses the same recipe. Removing a plan is blocked by
live or unverifiable runs and does not remove previously imported adapters.

For an Apple Silicon MLX chat model, a small explicit recipe can be created
without interactive prompts:

```bash
tentgent train lora plan create \
  --model <model-ref> --dataset <dataset-ref> --name small-helper \
  --backend mlx --rank 8 --learning-rate 0.0001 \
  --batch-size 1 --grad-accum 1 --max-seq-length 256 \
  --num-layers 4 --grad-checkpoint --max-steps 200 --seed 42
```

This is a starting configuration, not a quality guarantee. Select a compatible
MLX model, validate your data, and inspect the resolved plan before running.

## CLI Parameters And HTTP Fields

Plan creation and preview share these inputs:

| CLI option | JSON field | Type and meaning |
| --- | --- | --- |
| `-m`, `--model` | `model_ref` | Required string; managed base model ref. |
| `-d`, `--dataset` | `dataset_ref` | Required string; managed dataset with a train split. |
| `-n`, `--name` | `name` | Optional display name string; excluded from recipe identity. |
| `-B`, `--backend` | `backend` | `auto` (default), `mlx`, or `peft`. |
| `-R`, `--review`; `-i`, `--interactive` | No JSON field | CLI interaction; HTTP uses the preview endpoint. |

HTTP tuning fields belong inside `overrides`. All are optional:

| CLI option | `overrides` field | Type and meaning |
| --- | --- | --- |
| `-L`, `--max-seq-length` | `max_seq_length` | Integer token-length cap. |
| `-p`, `--mask-prompt` | `mask_prompt: true` | Default: train only the final assistant answer while keeping context visible. |
| `--no-mask-prompt` | `mask_prompt: false` | Train the complete rendered text. |
| `-r`, `--rank` | `rank` | Integer LoRA rank. |
| `-l`, `--learning-rate` | `learning_rate` | Numeric learning rate. |
| `-b`, `--batch-size` | `batch_size` | Integer per-device batch size. |
| `-g`, `--grad-accum` | `gradient_accumulation_steps` | Integer gradient accumulation steps. |
| `-s`, `--max-steps` | `max_steps` | Integer training-step limit. |
| `-S`, `--seed` | `seed` | Integer random seed. |
| `-N`, `--num-layers` | `mlx_num_layers` | Integer MLX tuned layer count. |
| `-c`, `--grad-checkpoint` | `mlx_grad_checkpoint` | Boolean MLX gradient checkpointing flag. |
| `--load-in-4bit` | `peft_load_in_4bit` | Boolean PEFT loading flag; current execution limitation below. |
| `--load-in-8bit` | `peft_load_in_8bit` | Boolean PEFT loading flag; cannot combine with 4-bit loading. |

Backend-specific overrides for the other backend are ignored with a warning.
MLX quantized weights do not require a PEFT loading flag. For recipe identity,
stored TOML fields, and readiness rules, see the
[training contract](../contracts/training-lora.md).

## Run And Select The Adapter

```bash
tentgent train lora run <plan-ref> --verbose
tentgent adapter ls
tentgent adapter inspect <adapter-ref>
tentgent chat <model-ref> \
  --adapter-ref <adapter-ref> \
  --message 'user:Give one practical suggestion for organizing my desk.' \
  --max-tokens 128 --temperature 0
```

A successful managed run imports its adapter and prints the adapter ref.
Reuse the compatible base model ref for chat. Omit `--adapter-ref` to use the
base model. Both MLX and PEFT chat adapters support per-request selection.
An external adapter can instead be imported with `tentgent adapter add
/path/to/adapter --base-model-ref <model-ref>`.

Run options are `-v` / `--verbose` for eval, checkpoint, and backend summary
events, and `-d` / `--debug` for raw backend output. The current Python endpoint
buffers events until training finishes, so these flags do not guarantee live
step-by-step output. Run artifacts include `run.toml`, `metrics.jsonl`, and
`raw.log`.

## HTTP Plans And Runs

See [API routes](#http-routes) for list, inspect, delete, metrics, and
log endpoints. Both `POST /v1/train/lora/plans/preview` and
`POST /v1/train/lora/plans` accept this JSON shape:

```json
{
  "model_ref": "<model-ref>",
  "dataset_ref": "<dataset-ref>",
  "name": "custom-chat",
  "backend": "mlx",
  "overrides": {
    "rank": 8,
    "learning_rate": 0.0001,
    "batch_size": 1,
    "gradient_accumulation_steps": 1,
    "max_seq_length": 256,
    "mlx_num_layers": 4,
    "mlx_grad_checkpoint": true,
    "max_steps": 200,
    "seed": 42
  }
}
```

Preview returns `plan` and `preview` metadata, including `would_reuse` and
`persisted: false`. Create returns `plan`, `created`, `deduplicated`,
`run_count`, `plan_dir`, and `plan_path`. Unknown request fields are rejected.
These endpoints do not execute training.

`POST /v1/train/lora/plans/{plan_ref}/runs` starts a detached worker and returns
a job; it uses the saved recipe and accepts no tuning overrides. Inspect the
job through `/v1/jobs/{job_id}` and run details through
`/v1/train/lora/runs/{run_ref}`. Add the
[daemon bearer token](./api.md) when configured.

For application use, start a [chat server](./servers.md) with the same base
model, then select `adapter_ref` in the request. See the
[streaming adapter example](./inference/chat.md#curl-streaming-and-adapter-selection).

## Current Execution Limits

- Only one live managed LoRA run is admitted at a time.
- PEFT 4-bit and 8-bit loading flags are accepted but unsupported by the
  current minimal PEFT training loop.
- There is no public LoRA stop/cancel command or automatic checkpoint resume.
  Repeating `run` creates a new run.
- Interrupting the CLI does not establish that the separate Python training
  process stopped. Generic daemon job cancellation is also not a guaranteed
  training-worker stop.
- Adapter import is part of success. If import fails after training, the
  managed run is failed; inspect its recorded error and output paths.

## HTTP Routes

| Method | Path | Body |
| --- | --- | --- |
| `GET` | `/v1/train/lora/plans` | None. |
| `POST` | `/v1/train/lora/plans` | `{"model_ref":"...","dataset_ref":"...","name":"optional","backend":"optional","overrides":{...}}` |
| `POST` | `/v1/train/lora/plans/preview` | Same as create, but does not persist. |
| `GET` | `/v1/train/lora/plans/{reference}` | None. |
| `DELETE` | `/v1/train/lora/plans/{reference}` | None. |
| `GET` | `/v1/train/lora/plans/{reference}/runs` | None. |
| `POST` | `/v1/train/lora/plans/{reference}/runs` | Starts a training run job. |
| `GET` | `/v1/train/lora/runs` | None. |
| `GET` | `/v1/train/lora/runs/{reference}` | None. |
| `GET` | `/v1/train/lora/runs/{reference}/metrics?tail=100` | Metrics tail. |
| `GET` | `/v1/train/lora/runs/{reference}/logs?tail_bytes=8192` | Log tail metadata and content. |
| `GET` | `/v1/train/lora/runs/{reference}/logs/raw?tail_bytes=8192` | Raw log tail. |

## Related Guides

[Models](./models.md) · [Datasets](./datasets.md) · [Adapters](./adapters.md) ·
[Chat and streaming](./inference/chat.md) · [Servers](./servers.md) · [Jobs](./jobs.md)
