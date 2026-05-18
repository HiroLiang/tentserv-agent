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
tentgent auth gemini set
```

Inspect or remove one provider key:

```bash
tentgent auth openai
tentgent auth openai rm
tentgent auth gemini
tentgent auth gemini rm
```

The daemon exposes read-only auth status. Provider key set/remove stays
local-only through the CLI or guarded TUI Keychain setup flows:

```bash
curl -sS http://127.0.0.1:8790/v1/auth \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/auth/openai \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
```

Daemon auth status reports local env/keychain presence only. It does not print
secrets and does not call provider validation endpoints.

## TUI

Open the local terminal operator console:

```bash
tentgent tui
tentgent tui --home /path/to/tentgent-home
tentgent tui --daemon-url http://127.0.0.1:8790
```

The TUI uses daemon HTTP for live status, doctor data, and read-only operator
navigation. It uses shared local code only for bootstrap config, daemon
discovery, explicit daemon start, and guarded Keychain-backed provider setup.

When the daemon is down, select `Start daemon` and press Enter to start a local
daemon through the same detached-launch helper as `tentgent daemon start`. The
start host and port come from the configured daemon host/port, so
`--daemon-url http://127.0.0.1:8791` starts `127.0.0.1:8791` rather than the
default port.

When the daemon is running, the TUI can browse models, adapters, datasets,
servers, sessions, train plans, and train runs in read-only navigator screens.
Server logs, session message tails, train run metrics, and train run logs are
bounded read-only views.

In the `Models`, `Adapters`, and `Datasets` screens, press `a` to open guarded
store actions. These forms call existing daemon HTTP routes for model/adapter
pull/import/remove, adapter bind, and dataset import/validate/template/export/
diff/synth/eval/remove. Destructive remove actions require typing the selected
short ref or full ref. Provider-backed synth/eval actions show an explicit
network/provider-credit confirmation and do not read Keychain before submit.
The TUI never shells out to `tentgent`, edits store files directly, or stores
raw provider output in the UI.

Long-running TUI store actions use daemon-side background jobs. Model/adapter
pull, imports, and provider-backed dataset synth/eval can be backgrounded while
you keep browsing. The Operator menu includes `Jobs`, and the footer/dashboard
show active job progress. Job records are read-only and use the same daemon
auth as other `/v1/*` routes; Slice 4.1 does not expose cancellation.

The `Servers` and `Training` screens expose guarded runtime actions through
the same daemon HTTP lifecycle routes documented below. Press `a` in `Servers`
to create/start/stop/remove server specs. Press `a` in `Training` Plans to
preview/create/remove LoRA plans or start a run from a selected plan. Press `A`
from `Models` to prefill server creation from the selected model, or from
`Datasets` to prefill LoRA plan creation from the selected dataset. Server
start uses a bounded readiness wait and is not a background job; LoRA runs stay
in the training run registry and are monitored with bounded metrics/log tails.
The TUI does not expose fake cancellation.

The Operator menu includes `Chat`, a session workspace over existing daemon
session and chat routes. It can choose a running server, create or resume a
session, stream assistant output through `POST /v1/chat`, and refresh the
persisted session transcript. Chat defaults to sending only the last 2 persisted
session messages as context; focus outside the composer and press `h` to cycle
`none`, `last 2`, `last 10`, and `last 50`. The transcript display still shows a
bounded tail separately. The TUI does not start servers, pull models, delete
sessions, compact manually, or store transcript files itself.

The Operator menu also includes `Resources`, a read-only local monitor for
runtime-home storage, daemon/server/train process pressure, disk-free state, and
large/stale resource warnings. Open `Resources` and press `r` to scan; normal
dashboard refresh uses only the last completed resource snapshot and does not
deep-scan the runtime home.

Daemon URL discovery order is `--daemon-url`,
`TENTGENT_DAEMON_URL`, `<TENTGENT_HOME>/config.toml` `[daemon].url`,
daemon metadata, then `http://127.0.0.1:8790`. Token discovery is `--token`,
`TENTGENT_DAEMON_TOKEN`, then no token. The TUI never stores daemon tokens or
provider secrets in config.

## Runtime

Inspect the managed Python runtime:

```bash
tentgent runtime status
tentgent runtime status --profile full
tentgent runtime status --project /path/to/python-project --env /path/to/python-env
```

Prepare the managed Python runtime after package-manager installs such as
Homebrew:

```bash
tentgent runtime bootstrap
tentgent doctor
```

Install heavier optional runtime profiles only when needed:

```bash
tentgent runtime bootstrap --profile local-model
tentgent runtime bootstrap --profile training
tentgent runtime bootstrap --profile full
```

Inspect the paths that would be used without syncing:

```bash
tentgent runtime bootstrap --print-plan
tentgent runtime bootstrap --profile local-model --dry-run
```

Direct release installers run this bootstrap by default. Use this command when
you install from a package manager, intentionally skipped installer bootstrap,
or need to resync Python dependencies after an upgrade.

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

Direct model-server chat is stateless. Do not send `session_ref` or
`max_session_messages` to a server port such as `8780`; those daemon-only fields
belong on daemon `POST /v1/chat` requests, usually port `8790`.

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

Start the local daemon process in background mode:

```bash
tentgent daemon start --host 127.0.0.1 --port 8790
tentgent daemon status
```

`tentgent daemon start` and `tentgent daemon run --detach` use the same detached
launch path. Loopback daemon binds can run without auth for local development.
To protect daemon `/v1/*` routes, set a local bearer token before starting the
daemon:

```bash
export TENTGENT_DAEMON_TOKEN='<local-token>'
tentgent daemon start --host 127.0.0.1 --port 8790
```

When the token is enabled, add
`-H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"` to every daemon `/v1/*`
request. `GET /healthz` stays public.

Inspect, call, or stop the daemon:

```bash
tentgent daemon status
curl -sS http://127.0.0.1:8790/healthz
curl -sS http://127.0.0.1:8790/v1/status \
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
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{}'
tentgent daemon stop
```

For foreground debugging, use:

```bash
tentgent daemon run --host 127.0.0.1 --port 8790
```

The daemon records process metadata under `TENTGENT_HOME/runtime` and exposes
Rust HTTP health/status, read-only store discovery, and controlled server
lifecycle endpoints. `POST /v1/chat` proxies to an already-running model-bound
server and preserves both JSON and streaming Server-Sent Event responses.
`POST /v1/chat/completions` offers a limited OpenAI-style success response for
basic chat-completion clients; its `model` field selects a Tentgent server ref
or unique prefix, not a provider model name.
Both daemon chat endpoints can optionally take `session_ref`; request messages
are treated as the new turn, recent session messages are prepended as context,
and successful assistant replies are appended to the transcript. Session
transcripts are bounded to 50 persisted messages; older messages may be
destructively summarized before chat continues.
The daemon-only `server_ref` selector belongs on daemon `POST /v1/chat` requests;
do not send it when calling the model-bound server port directly. Direct
model-server ports also reject `session_ref` and `max_session_messages`; use the
daemon URL for session-aware chat. Log diagnostics endpoints expose fixed
daemon/server stdout and stderr paths for local debugging.
Non-loopback or wildcard daemon binds require `TENTGENT_DAEMON_TOKEN` or the
explicit `--allow-unsafe-bind` flag.
Detached daemon children inherit daemon configuration environment variables,
including `TENTGENT_DAEMON_TOKEN`; model-bound server children still remove that
token before launch.
`POST /v1/daemon/shutdown` requires `TENTGENT_DAEMON_TOKEN` even on loopback
and stops only the daemon process; it does not stop running model-bound servers.

Inspect and remove managed store entries through the daemon:

```bash
curl -sS http://127.0.0.1:8790/v1/models/import \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"path":"/absolute/path/on/daemon-host/model"}'
curl -sS http://127.0.0.1:8790/v1/models/pull \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"repo_id":"owner/name","revision":null}'
curl -sS http://127.0.0.1:8790/v1/models/pull/jobs \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"repo_id":"owner/name","revision":null}'
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
curl -sS http://127.0.0.1:8790/v1/datasets/validate \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"path":"/absolute/path/on/daemon-host/dataset"}'
curl -sS http://127.0.0.1:8790/v1/datasets/template \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"task":"support","language":"zh-TW"}'
curl -sS http://127.0.0.1:8790/v1/datasets/synth \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"print_prompt":true,"brief":"Generate support examples in Traditional Chinese.","split":"train","count":20}'
curl -sS http://127.0.0.1:8790/v1/datasets/synth \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"provider":"openai","model":"gpt-4.1-mini","output_path":"/absolute/path/on/daemon-host/generated","brief":"Generate support examples in Traditional Chinese.","split":"train","count":20,"timeout_seconds":300,"retries":1}'
curl -sS http://127.0.0.1:8790/v1/datasets/eval \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"dataset_ref":"<dataset-ref>","provider":"openai","model":"gpt-4.1-mini","output_path":"/absolute/path/on/daemon-host/eval-report","max_records":20}'
curl -sS http://127.0.0.1:8790/v1/datasets/<dataset-ref>/export \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"output_path":"/absolute/path/on/daemon-host/work-dir"}'
curl -sS http://127.0.0.1:8790/v1/datasets/<dataset-ref>/diff \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"right_path":"/absolute/path/on/daemon-host/work-dir"}'
curl -sS http://127.0.0.1:8790/v1/models/<model-ref> \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS -X DELETE http://127.0.0.1:8790/v1/models/<model-ref> \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/adapters/<adapter-ref> \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS -X DELETE http://127.0.0.1:8790/v1/adapters/<adapter-ref> \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/datasets/<dataset-ref> \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS -X DELETE http://127.0.0.1:8790/v1/datasets/<dataset-ref> \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS -X DELETE http://127.0.0.1:8790/v1/servers/<server-ref> \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/jobs \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/jobs/<job-id> \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
```

Import paths are read from the daemon host filesystem, must be absolute, and
may expose local source/store paths in responses. Pull endpoints are synchronous
compatibility calls and may outlive short client timeouts on large downloads.
Use the `/jobs` variants for daemon-side background progress without changing
the synchronous response shape for existing clients.
Dataset validation failures return HTTP `200` with `valid:false`; HTTP `400`
is reserved for malformed daemon requests. Dataset template returns the prompt
body in JSON and does not write a file. Dataset export writes only to a missing
or empty daemon-host directory. Dataset diff returns at most 500 file entries
with `truncated:true` when the underlying diff is larger. Dataset synth/eval
HTTP calls are synchronous provider workflows; use long client timeouts. They
can accept direct spec or dataset content for tool integrations, but may send
that selected content to the configured provider. Failed provider runs return
debug artifact paths, not raw provider output.

Server delete removes a stopped server spec only. Stop a running server before
deleting it. Model and adapter delete may return `409 in_use` when server specs
still reference them.

Create and inspect local sessions from the CLI:

```bash
tentgent session create --title "Planning" --tag draft
tentgent session ls
tentgent session inspect <session-ref>
tentgent session append <session-ref> --role user --content "Hello"
tentgent session append <session-ref> --role user --content "Hello" --compaction-server <server-ref>
tentgent session compact <session-ref> --server <server-ref>
tentgent session messages <session-ref> --tail 100
tentgent session update <session-ref> --title "Planning v2"
tentgent chat <model-ref> --session <session-ref> --message "user:Continue."
tentgent session rm <session-ref>
```

Read and mutate local sessions through the daemon:

```bash
curl -sS http://127.0.0.1:8790/v1/sessions \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/sessions/<session-ref> \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS "http://127.0.0.1:8790/v1/sessions/<session-ref>/messages?tail=100" \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/sessions \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"title":"Planning","tags":["draft"]}'
curl -sS http://127.0.0.1:8790/v1/sessions/<session-ref>/messages \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"messages":[{"role":"user","content":"Hello"}]}'
curl -sS http://127.0.0.1:8790/v1/sessions/<session-ref>/compact \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"server_ref":"<server-ref>","keep_recent_messages":49}'
```

Session deletion is permanent. Chat remains stateless unless `--session` or
`session_ref` is provided. Session-aware chat serializes turns for a session
while the model response is running so transcript order stays stable. Sessions
are bounded working context: when they would exceed 50 messages, older messages
may be destructively summarized into one `system` summary message.

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

The daemon exposes the same plan-management step without starting training:

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

If `TENTGENT_DAEMON_TOKEN` is enabled, add
`-H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"`. HTTP deletion only removes
plans with zero run records.

Start and monitor a run through the daemon:

```bash
curl -sS -X POST http://127.0.0.1:8790/v1/train/lora/plans/<plan-ref>/runs
curl -sS http://127.0.0.1:8790/v1/train/lora/plans/<plan-ref>/runs
curl -sS http://127.0.0.1:8790/v1/train/lora/runs/<run-ref>
curl -sS http://127.0.0.1:8790/v1/train/lora/runs/<run-ref>/metrics
curl -sS http://127.0.0.1:8790/v1/train/lora/runs/<run-ref>/logs/raw
```

Run start returns after a detached worker process starts. Only one live LoRA run
is allowed at a time in the MVP. Training raw logs may include local paths or
dataset content and are not redacted.
