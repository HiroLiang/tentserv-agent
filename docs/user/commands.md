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
local-only through the CLI:

```bash
curl -sS http://127.0.0.1:8790/v1/auth \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/auth/openai \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
```

Daemon auth status reports local env/keychain presence only. It does not print
secrets and does not call provider validation endpoints.

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

Import a local model:

```bash
tentgent model add /path/to/local-model
tentgent model add ./models/bge-small --capability embedding
```

Pull models from Hugging Face:

```bash
tentgent model pull google/gemma-3-1b-it
tentgent model pull mlx-community/Llama-3.2-1B-Instruct-4bit
tentgent model pull DravenBlack/gemma-3-1b-it-Q4_K_M-GGUF
tentgent model pull BAAI/bge-reranker-base --capability rerank --revision main
```

`--capability` accepts `chat`, `embedding`, `rerank`,
`audio-transcription`, `audio-speech`, `vision-chat`, or
`image-generation`. Chat, embedding, and rerank endpoints enforce this metadata
before runtime dispatch. `audio-transcription` is available through the daemon
job API for local safetensors ASR models. The remaining media capability values
are metadata-only until their payload and runtime contracts are implemented.

List and inspect models:

```bash
tentgent model ls
tentgent model inspect <model-ref>
tentgent model set-capability <model-ref> embedding
```

When no explicit capability or confident Hugging Face metadata is available,
Tentgent keeps the backward-compatible `chat` default and prints a warning.
Use `set-capability` to correct stored metadata later without changing
`model_ref`.

For recommended small Hugging Face fixtures, gated-access reminders, and
copy-paste smoke commands, see [model-fixtures.md](./model-fixtures.md).

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

Run one-shot embedding inference without starting the daemon:

```bash
tentgent embed <embedding-model-ref> \
  --input "first text" \
  --input "second text" \
  --pretty
```

Run one-shot rerank inference without starting the daemon:

```bash
tentgent rerank <rerank-model-ref> \
  --query "refund policy" \
  --document "first candidate text" \
  --document "second candidate text" \
  --top-n 1 \
  --pretty
```

`tentgent embed` and `tentgent rerank` print JSON with the resolved `model_ref`
and a `data` array matching daemon `/v1/embeddings` and `/v1/rerank` responses.
They are useful for scripts and smoke tests. For repeated traffic, use daemon
REST or a direct local server so the model can stay warm.

## Audio Transcription

Audio transcription currently runs through daemon jobs. The daemon reads a
local audio file path, writes the transcript into a kernel job workspace, and
serves result bytes through the workflow result route.

Pull a small model and start the daemon:

```bash
tentgent runtime bootstrap --profile local-model
tentgent model pull openai/whisper-tiny.en --capability audio-transcription
tentgent daemon start --host 127.0.0.1 --port 8790
```

MP3 and other compressed audio files require `ffmpeg` on `PATH` because the
Transformers ASR pipeline uses it to decode file paths. On macOS:

```bash
brew install ffmpeg
```

`tentgent doctor` reports this as `media decoder ffmpeg`. Missing `ffmpeg`
does not block non-media commands, but local audio/video file jobs should treat
the warning as required setup. The doctor warning prints an install hint for
the current operating system.

Start a transcription job:

```bash
curl -sS http://127.0.0.1:8790/v1/audio/transcriptions/jobs \
  -H 'Content-Type: application/json' \
  -d '{
    "model_ref": "<audio-transcription-model-ref>",
    "path": "/absolute/path/audio.wav",
    "language": "en",
    "output_format": "text",
    "timestamps": false
  }'
```

Omit `language` for English-only Whisper checkpoints such as
`openai/whisper-tiny.en`. Use `language` with multilingual checkpoints such as
`openai/whisper-tiny`.

Inspect the job and read result bytes:

```bash
curl -sS http://127.0.0.1:8790/v1/jobs/<job-id>

curl -sS \
  'http://127.0.0.1:8790/v1/audio/transcriptions/jobs/<job-id>/result?cursor=0&max_chunks=32' \
  -o transcript.txt
```

Supported output formats are `text`, `json`, `vtt`, and `srt`. A foreground
`tentgent transcribe` CLI wrapper is planned for the next media slice.

## Server

Launch a long-lived local server:

```bash
tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load
```

Server launch defaults to `--capability chat` and requires a chat-capable model.
Use `--capability embedding` for a local safetensors embedding model and
`--capability rerank` for a local safetensors rerank model.

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

Launch and call a direct local embedding server:

```bash
tentgent server run <embedding-model-ref> \
  --capability embedding \
  --host 127.0.0.1 \
  --port 8781 \
  --lazy-load

curl -s http://127.0.0.1:8781/v1/embeddings \
  -H 'Content-Type: application/json' \
  -d '{"input":["first text","second text"]}'
```

Launch and call a direct local rerank server:

```bash
tentgent server run <rerank-model-ref> \
  --capability rerank \
  --host 127.0.0.1 \
  --port 8782 \
  --lazy-load

curl -s http://127.0.0.1:8782/v1/rerank \
  -H 'Content-Type: application/json' \
  -d '{"query":"refund policy","documents":["first text","second text"],"top_n":1}'
```

Servers reject endpoint families that do not match their launch capability.

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

curl -sS http://127.0.0.1:8790/v1/daemon/logs \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS 'http://127.0.0.1:8790/v1/daemon/logs/stderr?tail_bytes=4096' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"

curl -sS http://127.0.0.1:8790/v1/models \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/adapters \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/datasets \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/jobs \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"

curl -sS http://127.0.0.1:8790/v1/chat \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "model_ref": "<model-ref>",
    "messages": [{"role": "user", "content": "Say hello in Traditional Chinese."}],
    "max_tokens": 64,
    "temperature": 0.0,
    "stream": false
  }'

curl -sS -N http://127.0.0.1:8790/v1/chat/completions \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "<model-ref>",
    "messages": [{"role": "user", "content": "Say hello in Traditional Chinese."}],
    "max_tokens": 64,
    "temperature": 0.0,
    "stream": true
  }'

curl -sS http://127.0.0.1:8790/v1/embeddings \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "model_ref": "<embedding-model-ref>",
    "input": ["first text", "second text"]
  }'

curl -sS http://127.0.0.1:8790/v1/rerank \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "model_ref": "<rerank-model-ref>",
    "query": "refund policy",
    "documents": ["first text", "second text"],
    "top_n": 1
  }'

curl -sS -N http://127.0.0.1:8790/v1/messages \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "<model-ref>",
    "max_tokens": 64,
    "messages": [{"role": "user", "content": "Say hello in Traditional Chinese."}],
    "stream": true
  }'

curl -sS -N 'http://127.0.0.1:8790/v1beta/models/<model-ref>:streamGenerateContent' \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "contents": [
      {"role": "user", "parts": [{"text": "Say hello in Traditional Chinese."}]}
    ]
  }'

curl -sS http://127.0.0.1:8790/v1/servers \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"runtime_ref":"openai:gpt-4.1-mini","host":"127.0.0.1","port":8780}'
curl -sS http://127.0.0.1:8790/v1/servers/<server-ref>/start \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"wait_ready":true,"timeout_seconds":30}'
curl -sS http://127.0.0.1:8790/v1/servers/<server-ref>/health \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/servers/<server-ref>/logs \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS 'http://127.0.0.1:8790/v1/servers/<server-ref>/logs/stderr?tail_bytes=4096' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/servers/<server-ref>/stop \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{}'
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
Rust HTTP health/status, store discovery and mutation, controlled server
lifecycle endpoints, background jobs, chat, sessions, and LoRA plan APIs.
Native `/v1/chat`, native `/v1/embeddings`, native `/v1/rerank`,
OpenAI-compatible
`/v1/chat/completions`, Claude-compatible `/v1/messages`, and Gemini-compatible
`/v1beta/models/{model}:generateContent` adapters are DTO/SSE translators over
kernel use cases. Chat routes currently reject tools, images, and audio before
calling the model runtime. Embedding and rerank requests do not create or mutate
sessions.
Log diagnostics endpoints expose fixed daemon/server stdout and stderr paths for
local debugging.
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
  -d '{"path":"/absolute/path/on/daemon-host/model","capability":"embedding"}'
curl -sS http://127.0.0.1:8790/v1/models/pull \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"repo_id":"owner/name","revision":null,"capability":"rerank"}'
curl -sS http://127.0.0.1:8790/v1/models/pull/jobs \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"repo_id":"owner/name","revision":null,"capability":"rerank"}'
curl -sS http://127.0.0.1:8790/v1/models/<model-ref> \
  -X PATCH \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"capability":"embedding"}'
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
curl -sS http://127.0.0.1:8790/v1/datasets/synth/jobs \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"provider":"openai","model":"gpt-4.1-mini","output_path":"/absolute/path/on/daemon-host/generated","brief":"Generate support examples in Traditional Chinese.","split":"train","count":20,"timeout_seconds":300,"retries":1}'
curl -sS http://127.0.0.1:8790/v1/datasets/eval/jobs \
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
with `truncated:true` when the underlying diff is larger. Dataset synth/eval job
endpoints create daemon-side background jobs. They can accept direct spec or
dataset content for tool integrations, but may send that selected content to
the configured provider. Failed provider runs return debug artifact paths, not
raw provider output.

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
