# Common Commands

This document collects user-facing command examples. Short references are accepted anywhere a local `model_ref`, `adapter_ref`, `dataset_ref`, or `server_ref` is requested, as long as the prefix is unique.

Most common options have short aliases, such as `-m` for model/message-like inputs, `-o` for output, `-p` for provider/path/port depending on the subcommand, and `-H` for runtime home. Run `tentgent <command> --help`; every help screen also supports `-h`.

## Auth

Check all provider keys:

```bash
tentgent auth status
```

Set provider keys:

```bash
tentgent auth hf set
tentgent auth openai set
tentgent auth anthropic set
```

Inspect or remove one provider key:

```bash
tentgent auth openai
tentgent auth openai rm
```

## Models

Pull models from Hugging Face:

```bash
tentgent model pull google/gemma-3-1b-it
tentgent model pull mlx-community/Llama-3.2-1B-Instruct-4bit
tentgent model pull DravenBlack/gemma-3-1b-it-Q4_K_M-GGUF
```

List and inspect models:

```bash
tentgent model ls
tentgent model inspect <model-ref>
```

## Chat

Run one-shot chat:

```bash
tentgent chat <model-ref> --message "user:Hello there"
```

Run one-shot chat with an adapter:

```bash
tentgent chat <model-ref> \
  --adapter-ref <adapter-ref> \
  --message "user:Think step by step: what is 12 * 7?" \
  --max-tokens 128
```

## Server

Launch a long-lived local server:

```bash
tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load
```

Call the server:

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

Stream a local base-model response with Server-Sent Events:

```bash
curl -N http://127.0.0.1:8780/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{
    "messages": [
      {"role": "user", "content": "幫我列三個今天下午安排工作的建議。"}
    ],
    "max_tokens": 160,
    "temperature": 0.2,
    "stream": true
  }'
```

Stream with a compatible local adapter:

```bash
curl -N http://127.0.0.1:8780/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{
    "messages": [
      {"role": "user", "content": "請用繁體中文簡短介紹你自己。"}
    ],
    "adapter_ref": "<adapter-ref>",
    "max_tokens": 128,
    "temperature": 0.0,
    "stream": true
  }'
```

Use background server mode:

```bash
tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load --detach
tentgent server ls
tentgent server ps
tentgent server stop <server-ref>
```

Run a cloud provider server:

```bash
tentgent auth openai set
tentgent server run openai:gpt-4.1-mini --host 127.0.0.1 --port 8780
tentgent server ls
```

Cloud provider servers run as local HTTP proxies. Provider keys are resolved at launch and are not written to `server.toml`.
Cloud provider servers support the same `stream=true` SSE response shape as local servers.
Cloud provider servers do not support `adapter_ref`; adapters are local-runtime only.

Stream from a cloud provider server:

```bash
curl -N http://127.0.0.1:8780/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{
    "messages": [
      {"role": "system", "content": "Be concise."},
      {"role": "user", "content": "Say hello in Traditional Chinese."}
    ],
    "max_tokens": 64,
    "temperature": 0.0,
    "stream": true
  }'
```

## Daemon

Run the local daemon process in the foreground:

```bash
tentgent daemon run --host 127.0.0.1 --port 8790
```

Loopback daemon binds can run without auth for local development. To protect
daemon `/v1/*` routes, set a local bearer token before starting the daemon:

```bash
export TENTGENT_DAEMON_TOKEN='<local-token>'
tentgent daemon run --host 127.0.0.1 --port 8790
```

When the token is enabled, add
`-H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"` to every daemon `/v1/*`
request. `GET /healthz` stays public.

Inspect, call, or stop the daemon from another terminal:

```bash
tentgent daemon status
curl -sS http://127.0.0.1:8790/healthz
curl -sS http://127.0.0.1:8790/v1/status \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/daemon/logs
curl -sS 'http://127.0.0.1:8790/v1/daemon/logs/stderr?tail_bytes=4096'
curl -sS http://127.0.0.1:8790/v1/models
curl -sS http://127.0.0.1:8790/v1/adapters
curl -sS http://127.0.0.1:8790/v1/datasets
curl -sS http://127.0.0.1:8790/v1/servers
curl -sS http://127.0.0.1:8790/v1/servers \
  -X POST \
  -H 'Content-Type: application/json' \
  -d '{"runtime_ref":"openai:gpt-4.1-mini","host":"127.0.0.1","port":8780}'
curl -sS http://127.0.0.1:8790/v1/servers/<server-ref>/start \
  -X POST \
  -H 'Content-Type: application/json' \
  -d '{"wait_ready":true,"timeout_seconds":30}'
curl -sS http://127.0.0.1:8790/v1/servers/<server-ref>/health
curl -sS http://127.0.0.1:8790/v1/servers/<server-ref>/logs
curl -sS 'http://127.0.0.1:8790/v1/servers/<server-ref>/logs/stderr?tail_bytes=4096'
curl -sS http://127.0.0.1:8790/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{
    "server_ref": "<server-ref>",
    "messages": [
      {"role": "user", "content": "Say hello in Traditional Chinese."}
    ],
    "max_tokens": 64,
    "temperature": 0.0
  }'
curl -sS -N http://127.0.0.1:8790/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{
    "server_ref": "<server-ref>",
    "messages": [
      {"role": "user", "content": "Say hello in Traditional Chinese."}
    ],
    "max_tokens": 64,
    "temperature": 0.0,
    "stream": true
  }'
curl -sS http://127.0.0.1:8790/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{
    "model": "<server-ref>",
    "messages": [
      {"role": "user", "content": "Say hello in Traditional Chinese."}
    ],
    "max_tokens": 64,
    "temperature": 0.0
  }'
curl -sS -N http://127.0.0.1:8790/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{
    "model": "<server-ref>",
    "messages": [
      {"role": "user", "content": "Say hello in Traditional Chinese."}
    ],
    "max_tokens": 64,
    "temperature": 0.0,
    "stream": true
  }'
curl -sS http://127.0.0.1:8790/v1/servers/<server-ref>/stop -X POST
tentgent daemon stop
```

The daemon records process metadata under `TENTGENT_HOME/runtime` and exposes
Rust HTTP health/status, read-only store discovery, and controlled server
lifecycle endpoints. `POST /v1/chat` proxies to an already-running model-bound
server and preserves both JSON and streaming Server-Sent Event responses.
`POST /v1/chat/completions` offers a limited OpenAI-style success response for
basic chat-completion clients; its `model` field selects a Tentgent server ref
or unique prefix, not a provider model name.
The daemon-only `server_ref` selector belongs on daemon `POST /v1/chat` requests;
do not send it when calling the model-bound server port directly. Log diagnostics
endpoints expose fixed daemon/server stdout and stderr paths for local debugging.
Non-loopback or wildcard daemon binds require `TENTGENT_DAEMON_TOKEN` or the
explicit `--allow-unsafe-bind` flag.

## Adapters

Import or pull adapters:

```bash
tentgent adapter add /path/to/adapter --base-model-ref <model-ref>
tentgent adapter pull <hf-adapter-repo> --base-model-ref <model-ref>
tentgent adapter ls
```

Adapter requests should visibly change answer style when the adapter is compatible with the base model.

## Datasets

Import local datasets for training or evaluation:

```bash
tentgent dataset validate /path/to/dataset.jsonl
tentgent dataset validate /path/to/dataset-dir
tentgent dataset template -t chat -l zh-TW -o dataset-template.md
tentgent dataset synth \
  -p openai \
  -m gpt-4.1-mini \
  -o ./generated-dataset \
  --train-count 40 \
  --valid-count 8 \
  --test-count 8 \
  --timeout-seconds 300 \
  --retries 1 \
  -b "Generate concise support examples in Traditional Chinese."
tentgent dataset synth --print-prompt --train-count 20 -b "Generate concise support examples in Traditional Chinese."
tentgent dataset eval ./generated-dataset \
  -p openai \
  -m gpt-4.1-mini \
  -o ./generated-dataset-eval \
  -c "Check Traditional Chinese quality and whether final replies usually end with 咕嚕."
tentgent dataset add /path/to/dataset.jsonl
tentgent dataset add /path/to/dataset-dir
tentgent dataset ls
tentgent dataset inspect <dataset-ref>
tentgent dataset export <dataset-ref> /path/to/work-dir
tentgent dataset diff <left-dataset-ref> <right-dataset-ref>
tentgent dataset diff <dataset-ref> -p /path/to/work-dir
tentgent dataset rm <dataset-ref>
```

A training dataset directory is ready when it contains `train.jsonl`. Optional companions include `valid.jsonl`, `test.jsonl`, `eval_cases.jsonl`, and source `manifest.json`.

New chat and tool-use datasets should use the canonical `tentgent.chat.v1` schema in [docs/contracts/dataset-schema.md](../contracts/dataset-schema.md).

Use `dataset template` when you want a paste-ready prompt for OpenAI, Claude, or another agent to produce JSONL that should pass `dataset validate`.
Its `--task` and `--language` options are prompt hints only. For example, `--task support` asks the template to prefer support-style examples, and `--language zh-TW` asks for Traditional Chinese content; both still produce the same `tentgent.chat.v1` schema.

Use `dataset synth` to ask OpenAI or Claude to generate a local dataset directory. The output directory must be missing or empty. By default it writes one split, controlled by `--split` and optional `--count`. For a training-ready package with held-out files, use `--train-count`, `--valid-count`, `--test-count`, and optionally `--eval-count`; Tentgent writes each split file as soon as that provider call succeeds, so long multi-split runs leave visible file progress. It writes files only; run `dataset validate ./generated-dataset` and then `dataset add ./generated-dataset` when the result looks good. Add `--print-prompt` or `-P` to inspect the exact provider prompt without auth or network calls. `--retries` or `-r` defaults to `1` and retries each split independently after invalid provider JSON, schema mismatches, or transient provider errors; use `--retries 0` to disable retry. When provider output still fails local parsing or a later split times out, Tentgent writes `_debug/<split>/prompt.md`, `_debug/<split>/provider-output.raw.txt` when available, and `_debug/<split>/error.txt` under the output directory.

Use `dataset eval` to ask OpenAI or Claude to review generated or managed data before training. It does not mutate the dataset. The report directory contains `eval-report.json`, `eval-report.md`, `prompt.md`, and `provider-output.raw.txt`. Use `--criteria` or `-c` for project-specific checks such as style, language, or refusal behavior.

Most common long options have short aliases. Run `tentgent <command> --help` to see them; help always supports `-h`.

To edit a managed dataset, export it to a working directory, edit there, then run `dataset add` again to create a new content-derived reference.

## LoRA Training

Create, inspect, and run a managed LoRA training plan:

```bash
tentgent train lora plan create \
  --model <model-ref> \
  --dataset <dataset-ref> \
  --interactive
tentgent train lora plan ls
tentgent train lora plan inspect <plan-ref>
tentgent train lora plan rm <plan-ref>
tentgent train lora run <plan-ref>
```

Tentgent auto-selects the backend from the model format: `mlx` models use MLX, `safetensors` models use PEFT, and `gguf` models are blocked for LoRA training.

Common plan overrides: `--rank`, `--learning-rate`, `--batch-size`, `--grad-accum`, `--max-steps`, `--seed`, and `--max-seq-length`.

New LoRA plans mask prompt/context by default: the model still sees system, user, and tool context, but train loss only applies to the final assistant output. Use `--no-mask-prompt` only for plain continuation experiments where role labels and prompt framing should also be trained.
