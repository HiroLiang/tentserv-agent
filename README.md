# Tentgent

Tentgent is a Rust-first local operator CLI with a Python daemon layer for model runtimes, adapter management, and long-lived local servers.

The current MVP can manage provider keys, pull and deduplicate local models, import or pull LoRA adapters, run one-shot chat, and serve non-streaming local HTTP chat.

## Languages

- English source of truth: [README.md](./README.md)
- Traditional Chinese: [docs/i18n/zh-TW/README.md](./docs/i18n/zh-TW/README.md)
- Japanese: [docs/i18n/ja/README.md](./docs/i18n/ja/README.md)

## Install Status

Tentgent is currently source-first.

Available today:
- build and run from this repository

Planned later:
- Homebrew install
- packaged app or daemon distribution
- simpler bootstrap commands for non-developer users

Until packaged installers exist, run Tentgent from the checked-out repository.

## Quick Start

Build the Rust workspace:

```bash
cargo build --workspace
```

Use a repository-local runtime home while testing:

```bash
export TENTGENT_HOME="$PWD/.tentgent-test"
```

Pull a small model:

```bash
./target/debug/tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
```

List stored models:

```bash
./target/debug/tentgent model ls
```

Run one-shot chat:

```bash
./target/debug/tentgent chat <model-ref> --message "user:Hello there"
```

Launch a long-lived local server:

```bash
./target/debug/tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load
```

In another terminal, call the server:

```bash
curl -s http://127.0.0.1:8780/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{
    "messages": [
      {"role": "user", "content": "Hello there"}
    ],
    "max_tokens": 128,
    "temperature": 0.0
  }'
```

For background server mode, add `--detach` and manage the process with:

```bash
./target/debug/tentgent server ls
./target/debug/tentgent server ps
./target/debug/tentgent server stop <server-ref>
```

## Common Tasks

Authenticate provider keys:

```bash
./target/debug/tentgent auth hf set
./target/debug/tentgent auth openai set
./target/debug/tentgent auth anthropic set
```

Pull models from Hugging Face:

```bash
./target/debug/tentgent model pull google/gemma-3-1b-it
./target/debug/tentgent model pull mlx-community/Llama-3.2-1B-Instruct-4bit
./target/debug/tentgent model pull DravenBlack/gemma-3-1b-it-Q4_K_M-GGUF
```

Import or pull adapters:

```bash
./target/debug/tentgent adapter add /path/to/adapter --base-model-ref <model-ref>
./target/debug/tentgent adapter pull <hf-adapter-repo> --base-model-ref <model-ref>
./target/debug/tentgent adapter ls
```

Import local datasets for future training or evaluation:

```bash
./target/debug/tentgent dataset add /path/to/dataset.jsonl
./target/debug/tentgent dataset add /path/to/dataset-dir
./target/debug/tentgent dataset ls
./target/debug/tentgent dataset inspect <dataset-ref>
./target/debug/tentgent dataset export <dataset-ref> /path/to/work-dir
./target/debug/tentgent dataset diff <left-dataset-ref> <right-dataset-ref>
./target/debug/tentgent dataset diff <dataset-ref> --path /path/to/work-dir
./target/debug/tentgent dataset rm <dataset-ref>
```

For future tuning, a dataset directory is considered ready when it contains `train.jsonl`. `valid.jsonl`, `test.jsonl`, `eval_cases.jsonl`, and a source `manifest.json` are optional metadata or evaluation companions.
New chat and tool-use datasets should use the canonical `tentgent.chat.v1` schema in [docs/contracts/dataset-schema.md](./docs/contracts/dataset-schema.md).

To edit a managed dataset, export it to a working directory, make changes there, then run `dataset add` again to create a new content-derived dataset reference.
`dataset rm` removes only the managed store record and indexes; exported working copies are left alone.

Create, inspect, and run a managed LoRA training plan:

```bash
./target/debug/tentgent train lora plan create \
  --model <model-ref> \
  --dataset <dataset-ref> \
  --interactive
./target/debug/tentgent train lora plan ls
./target/debug/tentgent train lora plan inspect <plan-ref>
./target/debug/tentgent train lora plan rm <plan-ref>
./target/debug/tentgent train lora run <plan-ref>
```

Tentgent auto-selects the backend from the model format: `mlx` models use MLX, `safetensors` models use PEFT, and `gguf` models are blocked for LoRA training. A plan is a persistent recipe. `--review` previews the generated settings and asks before saving; `--interactive` lets you edit common settings before that review. `run` creates durable run records and imports successful MLX or PEFT outputs as managed adapters. `plan rm` removes only the stored plan and its run records.

Common plan overrides: `--rank` sets adapter capacity; `--learning-rate`, `--batch-size`, `--grad-accum`, `--max-steps`, and `--seed` control optimization; `--max-seq-length` caps token length.
Use `--mask-prompt` for chat-style datasets when you want the model to see system/user/tool context but train loss only on assistant output.
MLX-only overrides: `--num-layers` limits tuned layers and `--grad-checkpoint` trades speed for lower memory use.
PEFT-only overrides: `--load-in-4bit` and `--load-in-8bit` are planned quantized-loading flags; the minimal PEFT loop currently rejects them.

Run one-shot chat with an adapter:

```bash
./target/debug/tentgent chat <model-ref> \
  --adapter-ref <adapter-ref> \
  --message "user:Think step by step: what is 12 * 7?" \
  --max-tokens 128
```

Short references are accepted anywhere a local `model_ref`, `adapter_ref`, `dataset_ref`, or `server_ref` is requested, as long as the prefix is unique.

## LoRA Server Smoke Test

Start a server for a managed model:

```bash
./target/debug/tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load
```

Base request:

```bash
curl -s http://127.0.0.1:8780/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{
    "messages": [
      {"role": "user", "content": "Think step by step: what is 12 * 7?"}
    ],
    "max_tokens": 128,
    "temperature": 0.0
  }'
```

Adapter request:

```bash
curl -s http://127.0.0.1:8780/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{
    "messages": [
      {"role": "user", "content": "Think step by step: what is 12 * 7?"}
    ],
    "adapter_ref": "<adapter-ref>",
    "max_tokens": 128,
    "temperature": 0.0
  }'
```

Expected signal: the adapter request should visibly change the answer style. In a local Gemma 3 1B IT smoke test, the base model answered tersely and became noisy, while the LoRA request produced a structured step-by-step calculation ending in `84`.

## Runtime Home

Tentgent stores runtime state outside source code by default. During development, prefer:

```bash
export TENTGENT_HOME="$PWD/.tentgent-test"
```

Runtime directories include:

- `models/`
- `adapters/`
- `datasets/`
- `train/`
- `servers/`
- `cache/`
- `runtime/`
- `logs/`

Supported path overrides:

- `TENTGENT_HOME`
- `TENTGENT_MODELS_DIR`
- `TENTGENT_ADAPTERS_DIR`
- `TENTGENT_DATASETS_DIR`
- `TENTGENT_TRAIN_DIR`
- `TENTGENT_CACHE_DIR`
- `TENTGENT_RUNTIME_DIR`
- `TENTGENT_LOG_DIR`

Environment variables are read when a process starts. Tentgent does not rewrite or persist shell environment settings.

## Backend Status

- `safetensors` models run through the `transformers-peft` backend.
- `mlx` models run through the MLX backend on Apple Silicon.
- `gguf` models run through `llama-cpp-python`.
- PEFT LoRA adapters can be selected per request with `adapter_ref`.
- MLX adapters can be selected per request; changing adapters reloads the MLX model for correctness.
- `llama-cpp` external adapter execution is not implemented in this MVP.
- HTTP `/v1/chat` is non-streaming; `stream=true` currently returns `501`.

## Keychain Prompts

On macOS, Tentgent may trigger a Keychain prompt when a command needs a stored provider secret and no environment override is present.

This is expected for commands such as:

- `tentgent auth hf`
- `tentgent auth openai`
- `tentgent auth anthropic`
- `tentgent model pull <HF_REPO>`
- `tentgent adapter pull <HF_REPO>`

If you trust your locally built `./target/debug/tentgent` binary, choosing `Always Allow` is reasonable during development. Rebuilding or relocating an unsigned development binary may cause macOS to ask again.

To skip Keychain reads for one command, pass a one-shot environment variable:

```bash
HF_TOKEN="your token" ./target/debug/tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
```

One-shot environment variables apply only to that command and do not need `unset`.

## Project Docs

- Start with [AGENTS.md](./AGENTS.md) for shared repository context and documentation routing.
- Continue with [CLAUDE.md](./CLAUDE.md) for agent workflows and role boundaries.
- Use [docs/development/README.md](./docs/development/README.md) for developer commands and repository-local testing.
- Use [docs/contracts/](./docs/contracts/README.md) for cross-language interfaces.
- Use [docs/plans/](./docs/plans/README.md) for active staged plans.
- Use [docs/plans/archive/](./docs/plans/archive/README.md) only for completed historical plans.

## License

This project is proprietary and all rights are reserved. See [LICENSE](./LICENSE).

## Repository Layout

- `src/tentgent-core/`
  Shared Rust core types, runtime contracts, and routing logic.
- `src/tentgent-cli/`
  Rust CLI entry crate. The `tentgent` binary lives here.
- `src/tentgent-http/`
  Rust HTTP entry crate.
- `python/tentgent-daemon/`
  Python subproject for daemon-side runtime work.
- `python/tentgent-daemon/src/tentgent_daemon/`
  Importable Python package for runtime contracts, backend adapters, CLI helpers, and internal tools.
- `docs/contracts/`
  Interface documents between Rust entry points, shared core logic, and the Python daemon.
- `Makefile`
  Root developer shortcuts for formatting, checking, building, and running the Rust workspace.

## Naming

- Product slug: `tentgent`
- Binary name: `tentgent`
- Service host: `agent.tentserv.com`
- App identifier: `com.tentserv.tentgent`
- Environment variable prefix: `TENTGENT_`
