# Development

This document collects repository-local developer commands that are useful while Tentgent is still source-first.

For user-facing setup and usage, start with the root [README.md](../../README.md).

Run any command with `--help` or `-h` to see available short option aliases for that command.

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

Check release tag parsing and prerelease flag helpers:

```bash
bash scripts/test-release-metadata.sh
bash -n scripts/release-metadata.sh
bash -n scripts/test-release-metadata.sh
```

## CLI Help

Inspect layered help:

```bash
make run-cli ARGS='auth --help'
make run-cli ARGS='model --help'
make run-cli ARGS='adapter --help'
make run-cli ARGS='dataset eval --help'
make run-cli ARGS='server run --help'
```

## TUI Development

Run the terminal UI from source:

```bash
cargo run -- tui
cargo run -- tui --home "$TENTGENT_HOME"
```

The TUI is daemon-first for live status, doctor data, and read-only operator
navigation. It does not silently auto-start the daemon on launch; select
`Start daemon` and press Enter to start the daemon explicitly through the shared
detached-launch helper. The TUI derives the start host and port from the
resolved daemon URL, including `--daemon-url`, `TENTGENT_DAEMON_URL`, config,
metadata, or the default URL.

With the daemon running, use the operator menu to browse read-only models,
adapters, datasets, servers, sessions, train plans, and train runs. Navigator
tail views use bounded requests for logs, metrics, and session messages.

The `Models`, `Adapters`, and `Datasets` navigator screens have guarded action
menus opened with `a`. Slice 4 actions are allowlisted daemon HTTP mutations
only: model/adapter pull/import/remove, adapter bind, and dataset
import/validate/template/export/diff/synth/eval/remove. They must not shell out
to the CLI, call `/v1/auth`, read Keychain for confirmation, call server or
training lifecycle routes, or write store files directly. DELETE actions use an
empty body and require exact selected-ref confirmation. Provider-backed
synth/eval actions render bounded summaries and artifact/debug paths, not raw
provider output.

Long-running TUI actions use daemon-side background jobs. Model/adapter
pull/import, dataset import, and provider-backed dataset synth/eval start via
explicit `/jobs` routes, then surface progress in the footer, dashboard, and
Operator `Jobs` screen. The original synchronous daemon routes keep their old
response shapes for CLI and existing HTTP clients. Slice 4.1 records jobs under
runtime home and marks active jobs `interrupted` after daemon restart; it does
not expose cancellation.

Server and training lifecycle actions are TUI-only clients of existing daemon
HTTP routes. Server start uses `POST /v1/servers/{ref}/start` with bounded
readiness wait and is not a `/jobs` job. LoRA run start uses the existing
training run registry; the TUI polls active runs with bounded metrics/log tail
requests and does not mirror runs into the job registry. These flows must not
call `/v1/auth`, read Keychain, shell out to the CLI, or show fake cancel
controls.

The `Chat` menu entry is the first TUI mutation surface. It is limited to
existing daemon session/chat routes: `POST /v1/sessions` to create a session and
`POST /v1/chat` to send a turn. It streams by default, refreshes persisted
messages after completion, and never calls `PATCH /v1/sessions`, `/v1/auth`, or
server/model lifecycle routes. TUI chat defaults to `last 2` context, can cycle
`none` / `last 2` / `last 10` / `last 50` with `h` when the composer is not
focused, and keeps transcript display tail separate from the context sent to
`POST /v1/chat`.

The `Resources` menu entry is read-only and local: press `r` there to run a
bounded runtime-home disk/process scan. The normal dashboard refresh path should
reuse the last completed resource snapshot and must not deep-scan directories,
read Keychain, call `/v1/auth`, or issue mutation requests.

Use a token-enabled daemon when checking auth-required behavior:

```bash
export TENTGENT_DAEMON_TOKEN='<local-token>'
cargo run -- daemon start --host 127.0.0.1 --port 8790
cargo run -- tui --token "$TENTGENT_DAEMON_TOKEN"
```

`GET /healthz` remains public. `/v1/status` returns `401` without a valid bearer
token when daemon auth is enabled, and the TUI should show that as auth
required rather than daemon down.

Useful daemon lifecycle commands:

```bash
cargo run -- daemon start --host 127.0.0.1 --port 8790
cargo run -- daemon status
curl -sS http://127.0.0.1:8790/healthz
curl -sS http://127.0.0.1:8790/v1/status \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
cargo run -- daemon stop
```

`daemon status` resolves daemon paths in read-only mode before any stale-metadata
cleanup, so checking a deleted or moved runtime home should not recreate it. If
status reports `runtime_home_missing`, `runtime_dir_missing`, or stale metadata
warnings, confirm the listener or pid manually before terminating any process.

Detached daemon logs are written under the resolved runtime home:

```text
logs/daemon.stdout.log
logs/daemon.stderr.log
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
./target/debug/tentgent dataset validate /path/to/dataset.jsonl
./target/debug/tentgent dataset validate /path/to/dataset-dir
./target/debug/tentgent dataset template -t chat -l zh-TW -o /path/to/dataset-template.md
./target/debug/tentgent dataset synth \
  -p openai \
  -m gpt-4.1-mini \
  -o /path/to/generated-dataset \
  --train-count 40 \
  --valid-count 8 \
  --test-count 8 \
  --timeout-seconds 300 \
  --retries 1 \
  -b "Generate concise support examples in Traditional Chinese."
./target/debug/tentgent dataset synth --print-prompt --train-count 20 -b "Generate concise support examples in Traditional Chinese."
./target/debug/tentgent dataset eval /path/to/generated-dataset \
  -p openai \
  -m gpt-4.1-mini \
  -o /path/to/generated-dataset-eval \
  -c "Check language consistency and style drift."
./target/debug/tentgent dataset add /path/to/dataset.jsonl
./target/debug/tentgent dataset add /path/to/dataset-dir
```

Use `dataset template` to generate the manual prompt for OpenAI, Claude, or another agent. Its `--task` and `--language` options are prompt hints, not schema changes. Use `dataset synth` to call a provider directly and write a local package. Split-specific count options can generate train, validation, test, and eval files in one package. Use `--print-prompt` or `-P` to inspect the exact provider prompt without auth or network calls. `--retries` / `-r` retries each split independently after invalid provider output or transient provider failures. Failed provider parsing writes split-scoped `_debug/<split>` files under the output directory. Use `dataset eval` to write a report-only provider review before training. Use `dataset validate` before `dataset add` when working with generated JSONL.

The HTTP daemon exposes deterministic dataset tooling for local integrations:

```bash
curl -sS http://127.0.0.1:8790/v1/datasets/validate \
  -H 'Content-Type: application/json' \
  -d '{"path":"/absolute/path/on/daemon-host/dataset"}'
curl -sS http://127.0.0.1:8790/v1/datasets/template \
  -H 'Content-Type: application/json' \
  -d '{"task":"support","language":"zh-TW"}'
curl -sS http://127.0.0.1:8790/v1/datasets/synth \
  -H 'Content-Type: application/json' \
  -d '{"print_prompt":true,"brief":"Generate support examples in Traditional Chinese.","split":"train","count":20}'
curl -sS http://127.0.0.1:8790/v1/datasets/eval \
  -H 'Content-Type: application/json' \
  -d '{"input_content":"{\"schema\":\"tentgent.chat.v1\",\"messages\":[{\"role\":\"user\",\"content\":\"Hi\"},{\"role\":\"assistant\",\"content\":\"Hello\"}]}\n","provider":"openai","model":"gpt-4.1-mini","output_path":"/absolute/path/on/daemon-host/eval-report","max_records":1}'
curl -sS http://127.0.0.1:8790/v1/datasets/<dataset-ref>/export \
  -H 'Content-Type: application/json' \
  -d '{"output_path":"/absolute/path/on/daemon-host/work-dir"}'
curl -sS http://127.0.0.1:8790/v1/datasets/<dataset-ref>/diff \
  -H 'Content-Type: application/json' \
  -d '{"right_dataset_ref":"<other-dataset-ref>"}'
```

Validation failures are tool results: the daemon returns `200` with
`valid:false` when the request is well-formed but the dataset schema is invalid.
All paths are resolved on the daemon host filesystem. Cloud synth/eval HTTP
calls are synchronous provider workflows; prompt-only synth does not require
auth, while provider synth/eval may send selected path or content data to the
configured provider and return debug artifact paths on failure.

Inspect dataset state:

```bash
./target/debug/tentgent dataset ls
./target/debug/tentgent dataset inspect <dataset-ref>
./target/debug/tentgent dataset diff <left-dataset-ref> <right-dataset-ref>
./target/debug/tentgent dataset diff <dataset-ref> -p /path/to/work-dir
./target/debug/tentgent dataset rm <dataset-ref>
```

Dataset directories are marked tuning-ready when a root `train.jsonl` exists. `valid.jsonl`, `test.jsonl`, `eval_cases.jsonl`, and source `manifest.json` are detected when present.

Export a managed dataset to a working copy:

```bash
./target/debug/tentgent dataset export <dataset-ref> /path/to/work-dir
```

Edit the working copy, then import it again with `dataset add` to create a new dataset reference.

## Session Commands

Create and mutate local chat transcript sessions:

```bash
./target/debug/tentgent session create --title "Planning" --tag draft
./target/debug/tentgent session ls
./target/debug/tentgent session append <session-ref> --role user --content "Hello"
./target/debug/tentgent session append <session-ref> --role user --content "Hello" --compaction-server <server-ref>
./target/debug/tentgent session compact <session-ref> --server <server-ref>
./target/debug/tentgent session messages <session-ref> --tail 100
./target/debug/tentgent session update <session-ref> --title "Planning v2"
./target/debug/tentgent chat <model-ref> --session <session-ref> --message "user:Continue."
./target/debug/tentgent session rm <session-ref>
```

The HTTP daemon exposes the same session store mutations:

```bash
curl -sS http://127.0.0.1:8790/v1/sessions \
  -H 'Content-Type: application/json' \
  -d '{"title":"Planning","tags":["draft"]}'
curl -sS http://127.0.0.1:8790/v1/sessions/<session-ref>/messages \
  -H 'Content-Type: application/json' \
  -d '{"messages":[{"role":"user","content":"Hello"}]}'
curl -sS http://127.0.0.1:8790/v1/sessions/<session-ref>/compact \
  -H 'Content-Type: application/json' \
  -d '{"server_ref":"<server-ref>","keep_recent_messages":49}'
curl -sS http://127.0.0.1:8790/v1/sessions/<session-ref> \
  -X PATCH \
  -H 'Content-Type: application/json' \
  -d '{"title":"Planning v2"}'
curl -sS http://127.0.0.1:8790/v1/sessions/<session-ref> -X DELETE
```

Session deletion is permanent. Chat remains stateless unless `--session` or
`session_ref` is provided. Session-aware chat holds the session lock until the
assistant reply is recorded, so same-session turns are serialized. Sessions are
bounded to 50 persisted messages; compaction may rewrite older transcript
messages into a generated summary message.

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

Daemon train-plan parity:

```bash
curl -sS http://127.0.0.1:8790/v1/train/lora/plans/preview \
  -H 'Content-Type: application/json' \
  -d '{"model_ref":"<model-ref>","dataset_ref":"<dataset-ref>","backend":"auto","overrides":{"rank":8,"max_steps":100}}'

curl -sS http://127.0.0.1:8790/v1/train/lora/plans \
  -H 'Content-Type: application/json' \
  -d '{"model_ref":"<model-ref>","dataset_ref":"<dataset-ref>","backend":"auto"}'

curl -sS http://127.0.0.1:8790/v1/train/lora/plans
curl -sS http://127.0.0.1:8790/v1/train/lora/plans/<plan-ref>
curl -sS -X DELETE http://127.0.0.1:8790/v1/train/lora/plans/<plan-ref>
```

HTTP plan deletion refuses plans with run records. The daemon can also start
and observe runs:

```bash
curl -sS -X POST http://127.0.0.1:8790/v1/train/lora/plans/<plan-ref>/runs
curl -sS http://127.0.0.1:8790/v1/train/lora/runs
curl -sS http://127.0.0.1:8790/v1/train/lora/runs/<run-ref>/metrics
curl -sS http://127.0.0.1:8790/v1/train/lora/runs/<run-ref>/logs/raw
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

HTTP `stream=true` returns Server-Sent Events for local runtimes, compatible
local adapters, and OpenAI or Anthropic cloud provider runtimes.

Smoke-test streaming with:

```bash
curl -N http://127.0.0.1:8000/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{"messages":[{"role":"user","content":"Hello"}],"max_tokens":32,"stream":true}'
```

## Rust HTTP Daemon Entry

Start the daemon from the CLI:

```bash
cargo run -- daemon start --host 127.0.0.1 --port 8790
cargo run -- daemon status
```

Loopback daemon binds can run without auth for development. To exercise the
local bearer-token guard:

```bash
export TENTGENT_DAEMON_TOKEN='<local-token>'
cargo run -- daemon start --host 127.0.0.1 --port 8790
```

When the token is enabled, add
`-H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"` to every daemon `/v1/*`
request. `GET /healthz` stays public.

`daemon start` and `daemon run --detach` share one detached launch path. The
parent process waits up to five seconds for `GET /healthz`; if readiness times
out, the output includes the resolved daemon URL, runtime home, and daemon
stdout/stderr log paths. If `TENTGENT_DAEMON_TOKEN` is set, `/v1/status` is used
only as a stronger confirmation; a `401` does not override successful
`/healthz` readiness.

Check, call, or stop it:

```bash
cargo run -- daemon status
curl -sS http://127.0.0.1:8790/healthz
curl -sS http://127.0.0.1:8790/v1/status \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/auth \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/auth/openai \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/doctor \
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
curl -sS http://127.0.0.1:8790/v1/models/import \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"path":"/absolute/path/on/daemon-host/model"}'
curl -sS http://127.0.0.1:8790/v1/adapters/import \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"path":"/absolute/path/on/daemon-host/adapter","base_model_ref":"<model-ref>"}'
curl -sS http://127.0.0.1:8790/v1/adapters/<adapter-ref>/bind \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"base_model_ref":"<model-ref>"}'
curl -sS http://127.0.0.1:8790/v1/datasets/import \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"path":"/absolute/path/on/daemon-host/dataset"}'
curl -sS http://127.0.0.1:8790/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{
    "server_ref": "<server-ref>",
    "session_ref": "<session-ref>",
    "max_session_messages": 50,
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
    "session_ref": "<session-ref>",
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
curl -sS http://127.0.0.1:8790/v1/daemon/shutdown \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{}'
cargo run -- daemon stop
```

For foreground debugging, use:

```bash
cargo run -- daemon run --host 127.0.0.1 --port 8790
```

The low-level Rust HTTP binary has the same lifecycle metadata behavior:

```bash
cargo run -p tentgent-http --bin tentgent-http -- --host 127.0.0.1 --port 8790
```

All daemon CLI launch paths reject wildcard or non-loopback binds without
`TENTGENT_DAEMON_TOKEN` unless `--allow-unsafe-bind` is passed explicitly.
Detached daemon children inherit daemon configuration environment variables,
including `TENTGENT_DAEMON_TOKEN`; model-bound server children still remove that
token before launch.

At this stage the daemon records process metadata and serves `GET /healthz`,
`GET /v1/status`, and read-only discovery endpoints for models, adapters,
datasets, server specs, controlled server lifecycle mutations, and
`POST /v1/chat` proxying to already-running model-bound servers.
`POST /v1/chat/completions` adds a limited OpenAI-style success wrapper for
basic chat-completion clients; its `model` field selects a Tentgent server ref
or unique prefix, not a provider model name. Both chat routes can optionally use
`session_ref` for bounded context and transcript recording. Persisted session
transcripts are capped at 50 messages and may compact older messages into one
summary message. Use
`GET /v1/servers/<server-ref>/health` to distinguish process state from target
HTTP reachability before sending chat. Use the daemon and server log diagnostics
endpoints to inspect fixed stdout/stderr log paths without accepting arbitrary
filesystem paths. Store import, pull, inspect, and remove parity is available through
`POST /v1/models/import`, `POST /v1/models/pull`,
`POST /v1/adapters/import`, `POST /v1/adapters/pull`,
`POST /v1/adapters/<ref>/bind`, `POST /v1/datasets/import`,
`GET`/`DELETE /v1/models/<ref>`, `/v1/adapters/<ref>`, `/v1/datasets/<ref>`,
and `DELETE /v1/servers/<ref>`; server delete removes stopped specs only.
Import paths are absolute paths on the daemon host. Pull endpoints are
synchronous compatibility calls. Background variants use
`POST /v1/models/pull/jobs`, `POST /v1/models/import/jobs`, `POST
/v1/adapters/pull/jobs`, `POST /v1/adapters/import/jobs`, `POST
/v1/datasets/import/jobs`, `POST /v1/datasets/synth/jobs`, and `POST
/v1/datasets/eval/jobs`, then expose progress through `GET /v1/jobs` and `GET
/v1/jobs/<job-id>`. No cancel route exists in Slice 4.1.
Request logs are emitted to stderr with peer, method, path, status, and elapsed
time fields. Auth failures never log token or header values.
Auth status is read-only and reports local env/keychain presence without
provider network validation. HTTP doctor is observational only and does not run
`doctor --fix` behavior. Daemon shutdown requires an enabled bearer token even
on loopback and stops only the daemon process.

The `tentgent-http` crate is split by responsibility:

- `src/lib.rs` is the crate root and public export surface.
- `src/app.rs` owns daemon binding, accept-loop wiring, connection handling,
  shared state, and request logging.
- `src/http.rs` owns low-level HTTP parsing and response writing.
- `src/response.rs` owns JSON, raw proxy, and error response helpers.
- `src/dto.rs` owns daemon request and response DTOs.
- `src/routes/` owns endpoint dispatch by capability: `status`, `store`,
  `lifecycle`, `chat`, `diagnostics`, `openai`, and `session`.

Python model-bound server launch helpers are core-owned in
`src/tentgent-core/src/server_runtime.rs`. The CLI server commands and daemon
lifecycle routes both consume that core launcher; `tentgent-http` remains the
daemon HTTP entry point rather than the owner of runtime launching.

## Documentation Rules

- Keep the root README user-facing.
- Put developer command detail in this file.
- Put interface contracts under `docs/contracts/`.
- Put unfinished staged plans under `docs/plans/`.
- Move completed plans to `docs/plans/archive/`.
- Keep Markdown concise and split by concern.
