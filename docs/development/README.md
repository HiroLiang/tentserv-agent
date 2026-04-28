# Development

This document collects repository-local developer commands that are useful while Tentgent is still source-first.

For user-facing setup and usage, start with the root [README.md](../../README.md).

## Local Runtime Home

Run manual tests from the repository root.

Use one stable repository-local runtime home:

```bash
export TENTGENT_HOME="$PWD/.tentgent-test"
```

Keep only `.tentgent-test/` as the long-lived local test store. Temporary one-off `.tentgent-*` directories should be deleted after experiments.

Installed binaries should fall back to the default platform-managed runtime home when `TENTGENT_HOME` and other path overrides are not set.

## Build And Check

Build the Rust workspace:

```bash
cargo build --workspace
```

Check the Rust workspace:

```bash
cargo check --workspace
```

Run Python unit tests that do not require provider network access:

```bash
PYTHONPATH=python/tentgent-daemon/src \
python3 -m unittest discover -s python/tentgent-daemon/tests
```

Use the Makefile wrappers:

```bash
make check
make run-cli ARGS='--help'
```

## CLI Help

Inspect layered help:

```bash
make run-cli ARGS='auth --help'
make run-cli ARGS='model --help'
make run-cli ARGS='adapter --help'
make run-cli ARGS='server run --help'
```

## Auth Commands

```bash
make run-cli ARGS='auth hf'
make run-cli ARGS='auth hf set'
make run-cli ARGS='auth openai'
make run-cli ARGS='auth anthropic'
```

## Model Commands

Pull test models:

```bash
./target/debug/tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
./target/debug/tentgent model pull mlx-community/Llama-3.2-1B-Instruct-4bit
./target/debug/tentgent model pull DravenBlack/gemma-3-1b-it-Q4_K_M-GGUF
```

Inspect model state:

```bash
./target/debug/tentgent model ls
./target/debug/tentgent model inspect <model-ref>
```

Remove a model:

```bash
./target/debug/tentgent model rm <model-ref>
```

Tentgent blocks model removal while any stored server spec still references that model.

## Adapter Commands

Import a local adapter:

```bash
./target/debug/tentgent adapter add test-data/exp_v2 --base-model-ref <model-ref>
```

Pull a PEFT adapter from Hugging Face:

```bash
./target/debug/tentgent adapter pull peft-internal-testing/tiny_T5ForSeq2SeqLM-lora
```

Bind an imported adapter:

```bash
./target/debug/tentgent adapter bind <adapter-ref> --base-model-ref <model-ref>
```

Inspect adapter state:

```bash
./target/debug/tentgent adapter ls
./target/debug/tentgent adapter inspect <adapter-ref>
```

Remove an adapter:

```bash
./target/debug/tentgent adapter rm <adapter-ref>
```

## Dataset Commands

Import a local `.jsonl` file or dataset directory:

```bash
./target/debug/tentgent dataset add /path/to/dataset.jsonl
./target/debug/tentgent dataset add /path/to/dataset-dir
```

Inspect dataset state:

```bash
./target/debug/tentgent dataset ls
./target/debug/tentgent dataset inspect <dataset-ref>
./target/debug/tentgent dataset diff <left-dataset-ref> <right-dataset-ref>
./target/debug/tentgent dataset diff <dataset-ref> --path /path/to/work-dir
./target/debug/tentgent dataset rm <dataset-ref>
```

Dataset directories are marked tuning-ready when a root `train.jsonl` exists. `valid.jsonl`, `test.jsonl`, `eval_cases.jsonl`, and source `manifest.json` are detected when present.

Export a managed dataset to a working copy:

```bash
./target/debug/tentgent dataset export <dataset-ref> /path/to/work-dir
```

Edit the working copy, then import it again with `dataset add` to create a new dataset reference.

## Train Commands

Create a managed LoRA training plan without executing it:

```bash
./target/debug/tentgent train lora plan create \
  --model <model-ref> \
  --dataset <dataset-ref> \
  --interactive
```

Force a backend during plan creation:

```bash
./target/debug/tentgent train lora plan create \
  --model <model-ref> \
  --dataset <dataset-ref> \
  --backend mlx
```

Override selected defaults:

```bash
./target/debug/tentgent train lora plan create \
  --model <model-ref> \
  --dataset <dataset-ref> \
  --rank 8 \
  --learning-rate 0.00008 \
  --max-steps 320 \
  --review
```

Inspect stored plans:

```bash
./target/debug/tentgent train lora plan ls
./target/debug/tentgent train lora plan inspect <plan-ref>
./target/debug/tentgent train lora plan rm <plan-ref>
```

Run the current orchestration scaffold:

```bash
./target/debug/tentgent train lora run <plan-ref>
```

## Chat Commands

Run Python chat directly:

```bash
python/tentgent-daemon/.venv/bin/tentgent-chat-once \
  --model-ref <model-ref> \
  --home "$PWD/.tentgent-test" \
  --message "user:Hello there"
```

Run Rust-wrapped chat:

```bash
./target/debug/tentgent chat <model-ref> \
  --home "$PWD/.tentgent-test" \
  --message "user:Hello there"
```

Stream generated text:

```bash
./target/debug/tentgent chat <model-ref> \
  --home "$PWD/.tentgent-test" \
  --message "system:You are terse." \
  --message "user:Hello there" \
  --stream
```

Message inputs accept `role:content` for ordered context. Supported roles are `system`, `user`, and `assistant`. If no role prefix is present, Tentgent treats the message as `user`.

## Server Commands

Run a foreground server:

```bash
./target/debug/tentgent server run <model-ref> \
  --home "$PWD/.tentgent-test" \
  --host 127.0.0.1 \
  --port 8780 \
  --lazy-load
```

Run a detached server:

```bash
./target/debug/tentgent server run <model-ref> \
  --home "$PWD/.tentgent-test" \
  --host 127.0.0.1 \
  --port 8780 \
  --lazy-load \
  --detach
```

Run a cloud provider server:

```bash
./target/debug/tentgent server run openai:gpt-4.1-mini \
  --home "$PWD/.tentgent-test" \
  --host 127.0.0.1 \
  --port 8780 \
  --detach
```

Inspect and manage servers:

```bash
./target/debug/tentgent server ls --home "$PWD/.tentgent-test"
./target/debug/tentgent server ps --home "$PWD/.tentgent-test"
./target/debug/tentgent server inspect <server-ref> --home "$PWD/.tentgent-test"
./target/debug/tentgent server start <server-ref> --home "$PWD/.tentgent-test"
./target/debug/tentgent server stop <server-ref> --home "$PWD/.tentgent-test"
./target/debug/tentgent server rm <server-ref> --home "$PWD/.tentgent-test"
```

Add `--details` to `server start`, `server stop`, or `server rm` when you want a full inspection table.

## Python Server Direct Entry

Exercise the Python server module directly:

```bash
PYTHONPATH=python/tentgent-daemon/src \
python/tentgent-daemon/.venv/bin/python -m tentgent_daemon.cli.server \
  --server-ref <server-ref> \
  --runtime-kind local \
  --model-ref <model-ref> \
  --host 127.0.0.1 \
  --port 8000
```

Run the Python server module directly for a cloud provider:

```bash
OPENAI_API_KEY=<key> \
PYTHONPATH=python/tentgent-daemon/src \
python/tentgent-daemon/.venv/bin/python -m tentgent_daemon.cli.server \
  --server-ref <server-ref> \
  --runtime-kind cloud \
  --provider openai \
  --provider-model gpt-4.1-mini \
  --host 127.0.0.1 \
  --port 8000
```

The server exposes:

- `GET /healthz`
- `POST /v1/chat`

HTTP `stream=true` is not implemented yet and returns `501`.

## Documentation Rules

- Keep the root README user-facing.
- Put developer command detail in this file.
- Put interface contracts under `docs/contracts/`.
- Put unfinished staged plans under `docs/plans/`.
- Move completed plans to `docs/plans/archive/`.
- Keep Markdown concise and split by concern.
