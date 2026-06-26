# HTTP Daemon

This document defines the HTTP daemon boundary for the Rust `tentgent-daemon`
app host and `tentgent daemon` lifecycle commands. The route-by-route stability
tier is tracked in [api-surface-stability.md](./api-surface-stability.md);
some public routes remain experimental because their cleanup, support-proof
recovery, runtime, or release-readiness semantics are documented but still
allowed to tighten outside the `v1.0.0` stable surface.

## Scope

- Bind locally by default through `127.0.0.1`.
- Serve JSON responses for daemon-owned routes and errors.
- Expose native model chat at `POST /v1/chat`, including Server-Sent Events.
- Expose native local embeddings at `POST /v1/embeddings`.
- Expose native local rerank at `POST /v1/rerank`.
- Expose native media workflows through workflow-specific routes, including
  audio transcription, audio speech, vision chat, image generation/editing, and
  video understanding.
- Expose limited OpenAI, Claude, and Gemini compatible chat routes that translate
  DTO and SSE shapes only.
- Keep user-facing provider compatibility details in
  [provider-compatibility.md](../user/provider-compatibility.md) so callers can
  distinguish supported, partial, planned, and unsupported provider-shaped
  endpoint families.
- Expose daemon health, status, read-only store discovery, controlled server
  lifecycle mutations, store import/pull mutations, deterministic dataset
  tooling, cloud dataset tooling, LoRA train-plan management, auth status,
  doctor diagnostics, daemon shutdown control, chat, log diagnostics,
  session discovery and explicit session mutation.
- Keep loopback-local daemon development usable without auth, while requiring a
  token or explicit unsafe flag for non-loopback and wildcard binds.

## Version Source

HTTP daemon responses should use the active Rust daemon package version from
the workspace package version:

```text
env!("CARGO_PKG_VERSION")
```

Do not scatter independent package-version constants across HTTP handlers.

## Response Shape

Successful responses are endpoint-specific JSON objects in the MVP. They are not
wrapped in a shared `{ "data": ..., "error": null }` envelope yet.

Error responses use this shape:

```json
{
  "error": "not_found",
  "message": "route `/missing` was not found"
}
```

Rules:

- daemon-owned success and error responses must use
  `Content-Type: application/json; charset=utf-8`
- chat responses are daemon-owned JSON or Server-Sent Events
- unknown routes return a JSON `404`
- unsupported methods return a JSON `405`
- invalid requests return a JSON `400`
- ambiguous store references return a JSON `409`
- already-running, not-running, and provider-auth lifecycle conflicts return a
  JSON `409`
- unauthorized daemon requests return a JSON `401` with
  `WWW-Authenticate: Bearer`
- manager parse, IO, and unexpected read errors return a JSON `500` without
  secret values

## Local Auth And Bind Safety

`TENTGENT_DAEMON_TOKEN` enables a local bearer-token guard for the daemon. Unset,
empty, and whitespace-only values disable auth. Non-empty values are trimmed
before use.

When the token is enabled:

- `GET /healthz` remains public for readiness probes
- every `/v1/*` route requires `Authorization: Bearer <token>`, including
  unknown `/v1/*` routes before they return `404`
- missing, malformed, or wrong tokens return:

```json
{
  "error": "unauthorized",
  "message": "missing or invalid daemon bearer token"
}
```

The daemon never writes the token value to runtime state, logs, server specs, or
status responses. Detached daemon children inherit daemon configuration
environment variables, including `TENTGENT_DAEMON_TOKEN`, so token-enabled start
keeps auth enabled. Model-bound server child processes do not inherit
`TENTGENT_DAEMON_TOKEN`; provider auth environment variables keep their existing
behavior.

Bind safety is checked before the daemon listens or records process metadata.
Loopback hosts include parsed loopback IPs and literal `localhost`. Wildcard
hosts include `0.0.0.0`, `::`, and parsed unspecified IPs. Other IPs and
unrecognized hostnames are treated as unsafe non-loopback hosts without DNS
resolution.

```text
host class             token enabled  allowed
loopback               no             yes
loopback               yes            yes
wildcard/non-loopback  no             no, unless --allow-unsafe-bind
wildcard/non-loopback  yes            yes, with warning
```

`--allow-unsafe-bind` is available on daemon CLI launch paths and the
`tentgent-daemon` binary. It is intended only for explicit local-network
experiments; this MVP is not a public-service security model and does not add
TLS, CORS, multi-user auth, keychain token storage, or per-endpoint permissions.

## Lifecycle Commands And Discovery

`tentgent daemon start` launches the daemon in background mode and waits for
readiness. `tentgent daemon run --detach` uses the same detached-launch
implementation. Plain `tentgent daemon run` remains foreground mode for
debugging.

Detached launch readiness is based on public `GET /healthz`. If
`TENTGENT_DAEMON_TOKEN` is set, the parent may also probe `GET /v1/status` with
the bearer token, but a `401` status is reported as an auth warning rather than
a failed start after `/healthz` succeeds.

Client daemon URL discovery should use this order:

1. explicit `--daemon-url`
2. `TENTGENT_DAEMON_URL`
3. `<TENTGENT_HOME>/config.toml` `[daemon].url`
4. daemon process metadata `host` and `port`
5. `http://127.0.0.1:8790`

Token discovery should use explicit `--token`, then `TENTGENT_DAEMON_TOKEN`,
then no token. No token file or Keychain-backed daemon token is defined in this
contract.

## Health And Status

`GET /healthz` returns lightweight process health:

```json
{
  "status": "ok",
  "service": "tentgent-daemon",
  "version": "0.3.1"
}
```

`GET /v1/status` returns runtime-home, daemon process metadata, and non-secret
auth state:

```json
{
  "service": "tentgent-daemon",
  "version": "0.3.1",
  "status": "running",
  "auth": {
    "token_enabled": true
  },
  "host": "127.0.0.1",
  "port": 8790,
  "pid": 1234,
  "started_at": "2026-05-01T00:00:00Z",
  "runtime_home": "/path/to/tentgent-home",
  "runtime_dir": "/path/to/tentgent-home/runtime",
  "log_dir": "/path/to/tentgent-home/logs",
  "process_path": "/path/to/tentgent-home/runtime/daemon/process.toml",
  "pid_path": "/path/to/tentgent-home/runtime/daemon/daemon.pid",
  "warnings": []
}
```

`warnings` contains stable daemon diagnostic records with `code`, `message`, and
optional `path`. Missing runtime-home states use warning codes such as
`runtime_home_missing`, `runtime_dir_missing`, `process_path_missing`,
`pid_path_stale`, or `process_metadata_stale`.

## Auth Status, Doctor, And Shutdown

`GET /v1/auth` returns local provider credential status without provider network
validation and without secret values:

```json
{
  "providers": [
    {
      "provider": "openai",
      "display_name": "OpenAI",
      "env_present": true,
      "keychain_present": false,
      "effective_source": "env",
      "validation": {
        "state": "not_checked",
        "summary": "not checked",
        "detail": null
      }
    }
  ]
}
```

`GET /v1/auth/{provider}` accepts only exact lowercase provider ids:

```text
hf
openai
anthropic
gemini
```

Auth status endpoints may reveal whether credentials are configured, but never
credential values. Environment-variable credentials bypass Keychain reads. The
daemon HTTP endpoint reports cached Keychain presence metadata and does not
trigger the platform Keychain prompt.

`GET /v1/auth` and `GET /v1/auth/{provider}` are diagnostic-only and must not
accept provider secret values. Future HTTP workflows that perform provider work
may accept a per-request provider secret in a header or request body field, but
must not accept it in query strings, persist it, return it, or store it as
daemon-global mutable state.

`GET /v1/doctor` returns observational local diagnostics for the daemon runtime
home. It may inspect files, commands, Python runtime assets, platform
capability, and local paths, but it must not create, write, install, download,
repair, or delete anything:

```json
{
  "status": "warn",
  "summary": {
    "pass": 12,
    "warn": 2,
    "fail": 0,
    "skipped": 1
  },
  "checks": [
    {
      "name": "python binary",
      "status": "pass",
      "detail": "present: /path/to/python"
    }
  ]
}
```

Doctor status is `fail` if any check fails, otherwise `warn` if any check
warns, otherwise `pass`.

`POST /v1/daemon/shutdown` accepts an empty body or `{}` and returns
`202 Accepted` before stopping the daemon accept loop:

```json
{
  "shutdown": {
    "accepted": true,
    "pid": 12345,
    "message": "daemon shutdown requested"
  }
}
```

Shutdown stops only the daemon process. It does not stop running model-bound
servers. Before the daemon accept loop stops, shutdown aborts in-flight daemon
job handles, marks queued or running daemon jobs as terminal `interrupted`, and
runs one retention-aware workspace sweep. The sweep is best-effort and must not
delete just-interrupted or just-completed workspaces immediately. Unlike most
loopback-local daemon routes, shutdown requires `TENTGENT_DAEMON_TOKEN` to be
enabled and a valid bearer token; otherwise it returns
`409 daemon_token_required` or `401 unauthorized`.

## Read-Only Store Discovery

Read-only store endpoints use the daemon runtime home passed to
`tentgent daemon start --home <PATH>`, `tentgent daemon run --home <PATH>`, or
`tentgent-daemon --home <PATH>`. Store specific directory overrides still win
when set:

- `TENTGENT_MODELS_DIR`
- `TENTGENT_ADAPTERS_DIR`
- `TENTGENT_DATASETS_DIR`

These endpoints do not mutate state, start servers, stop servers, or proxy chat.
All dynamic refs accept full refs or unique prefixes. Ref path segments are
managed refs only, never filesystem paths.

`GET /v1/models` returns:

```json
{
  "models": [
    {
      "model_ref": "8fac...",
      "short_ref": "8fac906c66b9",
      "store_path": "/path/to/models/store/8fac...",
      "file_count": 8,
      "total_bytes": 2488939597,
      "imported_at": "2026-04-28T00:00:00Z",
      "format": "mlx",
      "detected_formats": ["mlx"],
      "mlx_runtime_family": "mlx-lm",
      "model_capabilities": ["chat"],
      "model_capability_source": "default-chat",
      "source_kind": "huggingface",
      "source_repo": "mlx-community/Llama-3.2-1B-Instruct-MLXTuned",
      "source_revision": "7247...",
      "source_path": null
    }
  ]
}
```

`GET /v1/models/{model_ref}` returns one model with the same stable fields plus
inspect-only paths:

```json
{
  "model": {
    "model_ref": "8fac...",
    "short_ref": "8fac906c66b9",
    "store_path": "/path/to/models/store/8fac...",
    "file_count": 8,
    "total_bytes": 2488939597,
    "imported_at": "2026-04-28T00:00:00Z",
    "format": "mlx",
    "detected_formats": ["mlx"],
    "mlx_runtime_family": "mlx-lm",
    "model_capabilities": ["chat"],
    "model_capability_source": "default-chat",
    "source_kind": "huggingface",
    "source_repo": "mlx-community/Llama-3.2-1B-Instruct-MLXTuned",
    "source_revision": "7247...",
    "source_path": null,
    "manifest_path": "/path/to/models/store/8fac.../manifest.json",
    "variant_source_path": "/path/to/models/store/8fac.../variants/mlx/source"
  }
}
```

`mlx_runtime_family` is present only for stored MLX models whose capability
metadata maps to a single runtime family such as `mlx-lm`, `mlx-vlm`,
`mlx-audio`, or `mlx-diffusion`. Missing family remains valid for legacy MLX
chat metadata.

`GET /v1/adapters` returns:

```json
{
  "adapters": [
    {
      "adapter_ref": "4012...",
      "short_ref": "4012b081478d",
      "store_path": "/path/to/adapters/store/4012...",
      "file_count": 6,
      "total_bytes": 56422213,
      "imported_at": "2026-04-28T00:00:00Z",
      "format": "mlx",
      "type": "lora",
      "base_model_ref": "8fac...",
      "base_model_source_repo": null,
      "base_model_source_revision": null,
      "model_family": null,
      "backend_support": [],
      "source_kind": "train-run",
      "source_repo": null,
      "source_revision": null,
      "source_path": null,
      "training_dataset_ref": "dataset-ref",
      "training_run_ref": "run-ref",
      "training_config_ref": "config-ref"
    }
  ]
}
```

`GET /v1/adapters/{adapter_ref}` returns one adapter with the same stable fields
plus `manifest_path` and `managed_source_path`.

`GET /v1/datasets` returns:

```json
{
  "datasets": [
    {
      "dataset_ref": "abcd...",
      "short_ref": "abcd1234abcd",
      "store_path": "/path/to/datasets/store/abcd...",
      "file_count": 3,
      "total_bytes": 12345,
      "imported_at": "2026-04-28T00:00:00Z",
      "format": "directory",
      "source_kind": "generated",
      "source_path": null,
      "source_repo": null,
      "source_revision": null,
      "tuning_ready": true,
      "splits": {
        "train": "train.jsonl",
        "validation": "valid.jsonl",
        "test": "test.jsonl",
        "eval_cases": null,
        "source_manifest": "manifest.json"
      },
      "warnings": []
    }
  ]
}
```

`GET /v1/datasets/{dataset_ref}` returns one dataset with the same stable fields
plus `manifest_path` and `managed_source_path`.

`GET /v1/sessions` returns local session metadata from
`<TENTGENT_HOME>/sessions` sorted by `updated_at` descending:

```json
{
  "sessions": [
    {
      "session_ref": "abcdefabcdef000000000000",
      "short_ref": "abcdefabcdef",
      "title": "Planning session",
      "created_at": "2026-05-01T00:00:00Z",
      "updated_at": "2026-05-01T00:10:00Z",
      "message_count": 2,
      "default_server_ref": null,
      "adapter_ref": null,
      "tags": [],
      "store_path": "/path/to/tentgent-home/sessions/abcdefabcdef000000000000"
    }
  ]
}
```

`GET /v1/sessions/{session_ref}` accepts a full session ref or unique prefix and
returns:

```json
{
  "session": {
    "session_ref": "abcdefabcdef000000000000",
    "short_ref": "abcdefabcdef",
    "title": "Planning session",
    "created_at": "2026-05-01T00:00:00Z",
    "updated_at": "2026-05-01T00:10:00Z",
    "message_count": 2,
    "default_server_ref": null,
    "adapter_ref": null,
    "tags": [],
    "store_path": "/path/to/tentgent-home/sessions/abcdefabcdef000000000000",
    "messages_path": "/path/to/tentgent-home/sessions/abcdefabcdef000000000000/messages.jsonl",
    "warnings": []
  }
}
```

`GET /v1/sessions/{session_ref}/messages?tail=100` returns the last N transcript
messages in chronological order:

```json
{
  "session": {
    "session_ref": "abcdefabcdef000000000000",
    "short_ref": "abcdefabcdef"
  },
  "messages": [
    {
      "index": 0,
      "role": "user",
      "content": "Hello",
      "created_at": "2026-05-01T00:00:00Z",
      "server_ref": null,
      "adapter_ref": null,
      "metadata": {}
    }
  ],
  "tail": 100,
  "total_messages": 1,
  "truncated": false,
  "warnings": []
}
```

`tail` defaults to `200`, minimum `1`, and maximum `1000`. Repeated,
non-integer, zero, negative, or above-max values return JSON `400`. Unknown
query parameters are ignored.

Missing session refs return JSON `404`, ambiguous prefixes return JSON `409`,
and malformed session metadata or messages return JSON `500` with
`session_read_failed`. Message parse errors include the line number but do not
echo transcript content. Missing `messages.jsonl` returns `200` with an empty
message list and a structured `messages_missing` warning.

Session path fields are local diagnostics and may expose filesystem layout.
They are intended for loopback-local daemon usage.

`POST /v1/sessions` creates a session and returns `201`:

```json
{
  "session": {
    "session_ref": "abcdefabcdef000000000000",
    "short_ref": "abcdefabcdef",
    "title": "Planning session",
    "created_at": "2026-05-01T00:00:00Z",
    "updated_at": "2026-05-01T00:00:00Z",
    "message_count": 0,
    "default_server_ref": null,
    "adapter_ref": null,
    "tags": [],
    "store_path": "/path/to/tentgent-home/sessions/abcdefabcdef000000000000",
    "messages_path": "/path/to/tentgent-home/sessions/abcdefabcdef000000000000/messages.jsonl",
    "warnings": []
  },
  "created": true
}
```

The create body may include `title`, `default_server_ref`, `adapter_ref`,
`tags`, and initial `messages`. `PATCH /v1/sessions/{session_ref}` updates
metadata; `null` clears optional string fields, blank strings are invalid, and
`tags` replaces the full tag list. Empty patch objects return `400`.

`POST /v1/sessions/{session_ref}/messages` appends one or more messages.
Messages are bounded by the session's 50-message working-context cap. If append
would exceed the cap, the body may include `compaction_server_ref` to compact
older messages first; otherwise the daemon returns `409
session_compaction_required`. Tentgent assigns `created_at` and returns appended
indexes:

```json
{
  "session": {
    "session_ref": "abcdefabcdef000000000000",
    "short_ref": "abcdefabcdef",
    "message_count": 2,
    "updated_at": "2026-05-01T00:05:00Z"
  },
  "appended": [
    { "index": 1, "role": "user", "created_at": "2026-05-01T00:05:00Z" }
  ]
}
```

Message content must be non-empty and no larger than 1 MiB. `metadata` is
optional, defaults to `{}`, and must be a JSON object when present.

`POST /v1/sessions/{session_ref}/compact` destructively compacts older
transcript messages into one generated summary message:

```json
{
  "server_ref": "optional-running-server-ref",
  "keep_recent_messages": 49,
  "instructions": null
}
```

Manual compact keeps `1 summary + keep_recent_messages`, defaults to 49 recent
messages, and returns `compacted:false` when there are not enough messages to
compact. The selected server is request `server_ref`, then the session
`default_server_ref`. Summary calls do not use `session_ref` and are not recorded
as transcript turns.

`DELETE /v1/sessions/{session_ref}` permanently removes the session directory.
There is no trash or recycle bin. DELETE bodies must be empty. All session
mutation endpoints follow daemon auth rules.

Session writes use lock files to coordinate CLI and HTTP writers. `409
session_busy` means another writer held the lock for the acquisition timeout.
Session deletion is permanent. Session compaction is also destructive: older raw
messages may be replaced by a generated summary because sessions are bounded
working context, not audit logs.

`GET /v1/servers` returns stored server specs and their current process state:

```json
{
  "servers": [
    {
      "server_ref": "25ee...",
      "short_ref": "25ee5888595d",
      "runtime_kind": "cloud",
      "capability": "chat",
      "model_ref": null,
      "provider": "openai",
      "provider_model": "gpt-4.1-mini",
      "host": "127.0.0.1",
      "port": 8780,
      "requested_port": 8780,
      "port_auto": true,
      "bound_port": null,
      "lazy_load": false,
      "idle_seconds": null,
      "created_at": "2026-04-28T00:00:00Z",
      "running": false,
      "process": null
    }
  ]
}
```

`GET /v1/servers/{server_ref}` accepts a full server ref or unique short prefix
and returns:

```json
{
  "server": {
    "server_ref": "25ee...",
    "short_ref": "25ee5888595d",
    "runtime_kind": "cloud",
    "capability": "chat",
    "model_ref": null,
    "provider": "openai",
    "provider_model": "gpt-4.1-mini",
    "host": "127.0.0.1",
    "port": 8780,
    "requested_port": 8780,
    "port_auto": true,
    "bound_port": null,
    "lazy_load": false,
    "idle_seconds": null,
    "created_at": "2026-04-28T00:00:00Z",
    "running": false,
    "process": null,
    "home_dir": "/path/to/tentgent-home",
    "server_dir": "/path/to/tentgent-home/servers/25ee...",
    "spec_path": "/path/to/tentgent-home/servers/25ee.../server.toml",
    "process_path": "/path/to/tentgent-home/servers/25ee.../process.toml",
    "stdout_log": "/path/to/tentgent-home/servers/25ee.../stdout.log",
    "stderr_log": "/path/to/tentgent-home/servers/25ee.../stderr.log"
  }
}
```

`capability` is the endpoint family the stored server spec is meant to serve.
When omitted for a local model, server lifecycle routes choose a capability
from the model's stored capabilities using this priority:
`video-understanding`, `vision-chat`, `image-generation`, `audio-transcription`,
`audio-speech`, `rerank`, `embedding`, then `chat`. Cloud server specs default
to `chat` and accept explicit `chat`, `vision-chat`, `embedding`, or
`image-generation` capabilities when the selected provider supports that
endpoint family. Server lifecycle routes also accept explicit local
model-runtime endpoint families such as `embedding`, `rerank`,
`audio-transcription`, `audio-speech`, `vision-chat`, `video-understanding`,
and `image-generation`. Older `server.toml` files without `capability` are read
as `chat`.

`port` is the effective port clients should call. For stopped auto-port specs,
it equals `requested_port`. When `port` is omitted at create time, the stored
spec sets `requested_port` to `8780`, `port_auto` to `true`, and each launch
scans upward from `8780` until a free port is found. The running process records
that actual port as `bound_port`, and read/health responses report it as
top-level `port`. Explicit `port` values set `port_auto` to `false` and fail at
start time if unavailable.

## Store Import And Pull Mutations

The daemon can populate managed stores with strict, synchronous JSON mutation
endpoints:

```text
POST /v1/models/import
POST /v1/models/pull
POST /v1/adapters/import
POST /v1/adapters/pull
POST /v1/adapters/{adapter_ref}/bind
POST /v1/datasets/import
```

Import paths are absolute paths on the daemon host filesystem, not the HTTP
client machine. They are canonicalized before core import. These endpoints may
return local source and store paths; this is intended for loopback-local daemon
usage.

```json
{ "path": "/absolute/path/on/daemon-host" }
{ "path": "/absolute/path/on/daemon-host", "capability": "embedding" }
{ "path": "/absolute/path/on/daemon-host", "base_model_ref": "optional" }
{ "repo_id": "owner/name", "revision": null }
{ "repo_id": "owner/name", "revision": "main", "capability": "rerank" }
{ "repo_id": "owner/name", "revision": "main", "base_model_ref": "optional" }
{ "base_model_ref": "model-ref-or-prefix" }
```

Request bodies reject unknown fields. `repo_id` must be a Hugging Face repo id
such as `owner/name`, not a URL or `/tree/...` path. Omitted or `null`
`revision` uses the core default; blank `revision` returns JSON `400`.
Model import and pull may include one optional `capability` value: `chat`,
`embedding`, `rerank`, `audio-transcription`, `audio-speech`, `vision-chat`,
`video-understanding`, or `image-generation`. Invalid capability values return
JSON `400 bad_request`.
The capability field updates model metadata and is enforced by implemented
endpoint-family gates; it does not change model identity. Implemented media
runtime routes currently include audio transcription, audio speech, native
vision chat, video understanding, and image generation/editing.

Omitted, `null`, or blank `base_model_ref` means no base binding for adapter
import or pull.

Successful responses return the stable inspect shape plus a mutation summary:

```json
{
  "model": {
    "...": "same shape as GET /v1/models/{model_ref}"
  },
  "mutation": {
    "kind": "import",
    "deduplicated": false,
    "store_path": "/path/to/models/store/8fac...",
    "source_index_path": "/path/to/models/by-source/local/..."
  },
  "warnings": [
    "capability defaulted to chat; provide capability to classify another endpoint family"
  ]
}
```

`warnings` is omitted when empty. Model import and pull include this warning
when capability metadata remains `default-chat`.

`mutation.kind` is `import`, `pull`, or `bind`. Adapter import and pull include
`base_index_path` only when core writes one. Adapter bind returns the updated
adapter inspect shape and `mutation.base_model_ref` with the resolved full model
ref.

Local missing paths return `404 path_not_found`; unsupported local layouts
return `400 unsupported_layout`; ambiguous refs return `409 ambiguous_ref`;
adapter base mismatches return `409 base_model_mismatch`; provider auth failures
return `409 provider_auth_failed`; Hugging Face helper invocation/output
failures return `502 pull_failed`; unexpected store mutation failures return
`500 store_mutation_failed`.

These endpoints remain synchronous compatibility surfaces. Large local imports
or Hugging Face pulls may exceed short client timeouts; clients that want
background progress should use the explicit job routes below.

## Background Action Jobs

Long-running store and dataset actions can opt into background jobs without
changing the synchronous route response shape. Durable job records and future
job workspaces are kernel-owned concepts exposed through daemon REST; the
daemon remains responsible for in-flight worker handles, execution, shutdown,
and HTTP adaptation. The daemon process itself is not a job, and model-bound
servers remain server lifecycle resources rather than job records.

```text
POST /v1/models/import/jobs
POST /v1/models/pull/jobs
POST /v1/adapters/import/jobs
POST /v1/adapters/pull/jobs
POST /v1/datasets/import/jobs
POST /v1/datasets/synth/jobs
POST /v1/datasets/eval/jobs
GET /v1/jobs
GET /v1/jobs/{job_id}
```

The async `POST .../jobs` request bodies are the same JSON DTOs accepted by the
corresponding synchronous route. They reject unknown fields in the same way and
return `202` with:

```json
{
  "job": {
    "job_id": "job-...",
    "kind": "model_pull",
    "label": "owner/name",
    "target_section": "models",
    "target_ref": null,
    "status": "queued",
    "stage": "queued",
    "cancellable": true,
    "refresh_targets": ["models"],
    "bytes_done": null,
    "bytes_total": null,
    "files_done": null,
    "files_total": null,
    "percent": null,
    "speed_bytes_per_sec": null,
    "eta_seconds": null,
    "started_at": "2026-05-04T00:00:00Z",
    "updated_at": "2026-05-04T00:00:00Z",
    "finished_at": null,
    "artifact_ref": null,
    "artifact_path": null,
    "warning_summary": null,
    "result_summary": null,
    "error_summary": null
  }
}
```

Job `status` values are `queued`, `running`, `succeeded`, `failed`,
`interrupted`, and `canceled`. `cancellable` is `true` only while a daemon job is
active and the daemon can accept a cancellation request for its durable job
state. Cancellation guarantees a terminal `canceled` record for non-terminal
jobs; aborting already-started blocking worker work is best-effort and must not
be treated as a hard process-kill guarantee. `DELETE /v1/jobs/{job_id}` accepts
terminal jobs only and removes both the durable job record and its workspace if
present. Job records are persisted under the resolved runtime home. Daemon
restart marks previously queued or running jobs as terminal `interrupted`
instead of resuming them.

Job records must not contain daemon tokens, provider secrets, raw provider
output, or unbounded logs. `/v1/jobs*` follows the same daemon bearer-token auth
rules as other `/v1/*` routes.

## Dataset Deterministic Tools

The daemon exposes provider-free dataset tooling over HTTP:

```text
POST /v1/datasets/validate
POST /v1/datasets/template
POST /v1/datasets/{dataset_ref}/export
POST /v1/datasets/{dataset_ref}/diff
```

Provider-backed dataset synth and eval routes run as daemon jobs through the
Rust cloud provider client:
`POST /v1/datasets/synth/jobs` and `POST /v1/datasets/eval/jobs`.

All path fields are absolute paths on the daemon host filesystem, not the HTTP
client machine. Path fields may expose local filesystem layout and are intended
for loopback-local daemon usage.

`POST /v1/datasets/validate` accepts exactly one source:

```json
{ "path": "/absolute/path/on/daemon-host" }
{ "dataset_ref": "managed-ref-or-prefix" }
```

Dataset schema failures are successful tool results, not request errors. A
valid request for an invalid dataset returns `200` with `valid: false`:

```json
{
  "valid": false,
  "source": {
    "kind": "path",
    "path": "/absolute/path/on/daemon-host",
    "dataset_ref": null,
    "short_ref": null
  },
  "target": "directory",
  "tuning_ready": true,
  "records": 56,
  "errors_count": 2,
  "splits": [
    { "name": "train", "path": "/path/train.jsonl", "records": 40, "errors": 0 }
  ],
  "warnings": [],
  "errors": [
    { "path": "/path/train.jsonl", "line": 12, "message": "..." }
  ]
}
```

`POST /v1/datasets/template` returns the deterministic prompt template without
writing a file:

```json
{ "task": "support", "language": "zh-TW" }
```

Response:

```json
{
  "template_version": "tentgent.dataset.synth.v1",
  "task": "support",
  "language": "zh-TW",
  "content": "..."
}
```

`POST /v1/datasets/{dataset_ref}/export` writes a managed dataset to a missing
or empty daemon-host directory:

```json
{ "output_path": "/absolute/path/on/daemon-host" }
```

Existing non-empty destinations return `409 output_exists`. Successful export
returns the dataset inspect shape plus output metadata:

```json
{
  "dataset": {
    "...": "same shape as GET /v1/datasets/{dataset_ref}"
  },
  "export": {
    "output_path": "/path/to/work-dir",
    "managed_source_path": "/path/to/tentgent-home/datasets/store/ref/source",
    "file_count": 3,
    "total_bytes": 1234
  }
}
```

`POST /v1/datasets/{dataset_ref}/diff` compares one managed left dataset with
exactly one right side:

```json
{ "right_dataset_ref": "managed-ref-or-prefix" }
{ "right_path": "/absolute/path/on/daemon-host" }
```

The response includes a bounded file list. `files` is capped at `500`; when the
underlying diff is larger, `truncated` is `true`.

```json
{
  "left": { "label": "8fac...", "short_ref": "8fac...", "path": null, "tuning_ready": true, "splits": "train,valid" },
  "right": { "label": "/path/to/work-dir", "short_ref": null, "path": "/path/to/work-dir", "tuning_ready": true, "splits": "train" },
  "diff": {
    "summary": {
      "added": 0,
      "removed": 0,
      "modified": 1,
      "unchanged": 2,
      "left_total_bytes": 100,
      "right_total_bytes": 120
    },
    "files": [
      { "status": "modified", "relative_path": "train.jsonl", "left_size_bytes": 100, "right_size_bytes": 120 }
    ],
    "file_limit": 500,
    "truncated": false
  }
}
```

Malformed JSON, unknown request fields, invalid one-of fields, and relative
paths return `400 bad_request`. Missing local paths return `404 path_not_found`;
missing dataset refs return `404 not_found`; ambiguous refs return
`409 ambiguous_ref`; unsupported dataset layouts return `400 unsupported_layout`;
unexpected local failures return `500 dataset_tool_failed`.

## Dataset Cloud Tools

Provider-backed dataset synth/eval are implemented in Rust through the shared
cloud provider client. OpenAI, Anthropic, and Gemini provider requests resolve
secrets through the daemon auth resolver; Python model-runtime processes are not
started for these workflows.

## LoRA Train Plans

The daemon exposes LoRA train-plan management without starting training runs:

```text
GET /v1/train/lora/plans
POST /v1/train/lora/plans/preview
POST /v1/train/lora/plans
GET /v1/train/lora/plans/{plan_ref}
DELETE /v1/train/lora/plans/{plan_ref}
```

`POST /v1/train/lora/plans/preview` validates and renders a plan but does not
write `plan.toml`. `POST /v1/train/lora/plans` writes or reuses the normalized
recipe. Requests are strict JSON:

```json
{
  "model_ref": "model-ref-or-prefix",
  "dataset_ref": "dataset-ref-or-prefix",
  "name": "optional display name",
  "backend": "auto",
  "overrides": {
    "max_seq_length": 1024,
    "mask_prompt": true,
    "rank": 8,
    "learning_rate": 0.0001,
    "batch_size": 1,
    "gradient_accumulation_steps": 4,
    "max_steps": 100,
    "seed": 42,
    "mlx_num_layers": 8,
    "mlx_grad_checkpoint": true,
    "peft_load_in_4bit": false,
    "peft_load_in_8bit": false
  }
}
```

`backend` defaults to `auto`. Numeric override values must be positive, and
`peft_load_in_4bit` cannot be combined with `peft_load_in_8bit`. `name` is
display metadata and does not participate in plan identity; repeated creates of
the same recipe return the existing plan and do not rename it.

Preview responses include `persisted:false`, `would_plan_dir`, and
`would_plan_path`. Create responses include `created`, `deduplicated`,
`run_count`, `plan_dir`, and `plan_path`. Blocked recipes return `200` with
`plan.status: "blocked"` and `blockers`; malformed requests return
`400 bad_request`.

`GET /v1/train/lora/plans` returns summaries sorted by `created_at` descending
and then `plan_ref` ascending:

```json
{
  "plans": [
    {
      "plan_ref": "plan-ref",
      "short_ref": "plan-short",
      "name": null,
      "status": "ready",
      "requested_backend": "auto",
      "backend": "mlx",
      "model_ref": "model-ref",
      "dataset_ref": "dataset-ref",
      "created_at": "2026-05-01T00:00:00Z",
      "run_count": 0,
      "plan_dir": "/path/to/train/lora/plans/plan-ref",
      "plan_path": "/path/to/train/lora/plans/plan-ref/plan.toml"
    }
  ]
}
```

`GET /v1/train/lora/plans/{plan_ref}` returns the full plan, run count, plan
path, and runs path. `DELETE` succeeds only for plans with zero runs and returns
pre-removal metadata. Plans with run records return `409 in_use`; callers should
use future run cleanup APIs before deleting those plans.

## LoRA Train Runs

The daemon can launch and observe saved LoRA training plans:

```text
POST /v1/train/lora/plans/{plan_ref}/runs
GET /v1/train/lora/plans/{plan_ref}/runs
GET /v1/train/lora/runs
GET /v1/train/lora/runs/{run_ref}
GET /v1/train/lora/runs/{run_ref}/metrics
GET /v1/train/lora/runs/{run_ref}/logs
GET /v1/train/lora/runs/{run_ref}/logs/raw
```

`POST /runs` accepts an empty body or `{}` only. It creates durable run
artifacts, launches a detached `tentgent train lora run-worker` process, and
returns `202 Accepted` after the worker starts. Run configuration always comes
from the saved plan; HTTP run start has no override fields.

Only one live LoRA run is allowed at a time in this MVP. Attempts to start a
second live run return `409 run_already_running`. Blocked plans return
`409 plan_blocked`.

Run inspect responses include persisted state and derived process state:

```json
{
  "run": {
    "run_ref": "run-ref",
    "short_ref": "run-short",
    "status": "running",
    "process_running": true,
    "stale": false,
    "phase": "train",
    "error": null,
    "plan_ref": "plan-ref",
    "model_ref": "model-ref",
    "dataset_ref": "dataset-ref",
    "backend": "peft",
    "pid": 12345,
    "exit_code": null,
    "adapter_ref": null,
    "created_at": "2026-05-01T00:00:00Z",
    "started_at": "2026-05-01T00:00:00Z",
    "ended_at": null,
    "run_dir": "/path/to/run",
    "run_path": "/path/to/run/run.toml",
    "metrics_path": "/path/to/run/metrics.jsonl",
    "raw_log_path": "/path/to/run/raw.log"
  }
}
```

`stale:true` is derived when `run.toml` records a live status but the recorded
pid is no longer running. `stale` is not written as a terminal status.

`GET /metrics?tail=N` returns the last metric events in chronological order.
The default tail is `200`, the maximum is `1000`, and malformed metric lines
produce structured warnings without echoing raw line content.

`GET /logs` returns raw log metadata. `GET /logs/raw?tail_bytes=N` uses the
same byte-tail rules as daemon log diagnostics: default `65536`, maximum
`262144`, UTF-8 lossy decoding, and `200 exists:false` for missing logs.
Training raw logs are local diagnostics and may contain dataset text or local
paths; no redaction is promised in this slice.

## Store Inspect And Remove Mutations

The daemon exposes safe metadata correction and remove parity for managed store
entries:

```text
POST /v1/models/{model_ref}/capabilities
GET /v1/models/{model_ref}/capabilities/proofs
DELETE /v1/models/{model_ref}/capabilities/proofs/{capability}
POST /v1/models/{model_ref}/capabilities/verify
DELETE /v1/models/{model_ref}
DELETE /v1/adapters/{adapter_ref}
DELETE /v1/datasets/{dataset_ref}
DELETE /v1/servers/{server_ref}
```

`POST /v1/models/{model_ref}/capabilities` rewrites only model capability
metadata. It sets `model_capability_source` to `manual-update`, recalculates
`mlx_runtime_family`, and does not change `model_ref`.

Replace the full capability set:

```json
{ "set": ["chat", "vision-chat"] }
```

Or apply an add/remove mutation:

```json
{ "add": ["vision-chat"], "remove": ["chat"] }
```

Successful model capability updates return:

```json
{
  "model": {
    "...": "same shape as GET /v1/models/{model_ref}"
  },
  "mutation": {
    "kind": "update_capabilities",
    "previous_capabilities": ["chat"],
    "capabilities": ["vision-chat"],
    "added": ["vision-chat"],
    "removed": ["chat"]
  }
}
```

Capability mutations canonicalize and de-duplicate values. Empty final
capability sets, invalid capability values, and bodies that mix `set` with
`add` or `remove` return `400 bad_request`; missing models return
`404 not_found`; ambiguous refs return `409 ambiguous_ref`.

`PATCH /v1/models/{model_ref}` remains a legacy compatibility alias that
accepts one `capability` string and replaces the model capability set with that
single value.

`GET /v1/models/{model_ref}/capabilities/proofs` returns latest local proof
records for the model:

The current response returns the legacy proof subset stored by
`ModelCapabilityProof`. The expanded proof and support hint schema is defined
in [model-support-proof-schema.md](./model-support-proof-schema.md); future
responses may add schema fields without changing the meaning of the existing
keys.

```json
{
  "model": {
    "...": "same shape as GET /v1/models/{model_ref}"
  },
  "proofs": [
    {
      "model_ref": "<model_ref>",
      "capability": "vision-chat",
      "status": "verified",
      "source": "server-start",
      "primary_format": "mlx",
      "backend": "mlx-vlm",
      "mlx_runtime_family": "mlx-vlm",
      "server_ref": "<server_ref>",
      "checked_at": "2026-05-26T00:00:00Z"
    }
  ]
}
```

`POST /v1/models/{model_ref}/capabilities/verify` accepts one capability:

```json
{ "capability": "<capability>" }
```

`<capability>` is one model capability value, such as `chat`, `embedding`,
`rerank`, `audio-transcription`, `audio-speech`, `vision-chat`,
`video-understanding`, or `image-generation`.

The manual verify route records a metadata-level `manual-probe` proof. It does
not run full endpoint inference in this slice. Local model-bound server starts
write `server-start` proofs after launch success or failure. Resolved local
runtime attempts may write `runtime-execution` proofs after execution succeeds
or fails. Future endpoint smoke tests can write `endpoint-smoke` proofs with
the same response shape.

`DELETE /v1/models/{model_ref}/capabilities/proofs/{capability}` clears all
local proof records for that model capability, including tuple-aware
backend/runtime-profile records and the legacy latest-proof file. It does not
remove model content or stored capability metadata. Successful proof clears
return:

```json
{
  "model": {
    "...": "same shape as GET /v1/models/{model_ref}"
  },
  "proof_clear": {
    "capability": "vision-chat",
    "removed_proof_count": 2
  }
}
```

`DELETE` requests must have an empty body. Non-empty bodies return JSON `400`
because this slice has no hidden `force`, dry-run, bulk, or cascade options.

Successful deletes return `200` with pre-removal metadata:

```json
{
  "removed": {
    "kind": "model",
    "model_ref": "8fac...",
    "short_ref": "8fac906c66b9",
    "store_path": "/path/to/models/store/8fac..."
  },
  "model": {
    "...": "same shape as GET /v1/models/{model_ref}"
  }
}
```

Adapters and datasets use typed `adapter_ref` and `dataset_ref` fields in the
`removed` object. Server spec removal returns:

```json
{
  "removed": {
    "kind": "server",
    "server_ref": "25ee...",
    "short_ref": "25ee5888595d",
    "server_dir": "/path/to/tentgent-home/servers/25ee..."
  },
  "server": {
    "...": "same shape as GET /v1/servers/{server_ref}"
  }
}
```

Model and adapter removal return JSON `409 in_use` when existing server specs
still reference them. Stop and remove those server specs first. Server removal
does not stop running processes; running servers return `409 already_running`,
so callers should use `POST /v1/servers/{server_ref}/stop` before `DELETE`.

Dataset removal only enforces protections currently tracked by core. Future
train-plan or train-run registries may make dataset deletion return
`409 in_use` when references are tracked.

`GET /v1/servers/{server_ref}/health` checks one stored server spec. Stopped
servers return `running: false` and `reachable: false` without opening a network
connection. Running servers probe the target model-bound server's `/healthz`
endpoint:

```json
{
  "server": {
    "server_ref": "25ee...",
    "short_ref": "25ee5888595d",
    "running": true
  },
  "running": true,
  "reachable": true,
  "target_url": "http://127.0.0.1:8780/healthz",
  "target_status": 200,
  "target_health": {
    "status": "ok",
    "chat_ready": true
  },
  "checked_at": "2026-04-28T00:00:00Z",
  "error": null
}
```

## Server Lifecycle

Lifecycle endpoints use the same daemon runtime home as the read-only discovery
routes. They mutate only stored server specs and server process state.

`POST /v1/servers` creates or reuses a stored server spec. It does not start the
server:

```json
{
  "runtime_ref": "openai:gpt-4.1-mini",
  "capability": "chat",
  "host": "127.0.0.1",
  "port": 8780,
  "lazy_load": false,
  "idle_seconds": null
}
```

Omit `port` to request automatic port selection. Auto-port specs keep
`requested_port = 8780` and rescan from that default on every launch; they do
not create a new server record just because a previous run had to bind a higher
port.

An abbreviated response is:

```json
{
  "server": {
    "server_ref": "25ee...",
    "short_ref": "25ee5888595d",
    "runtime_kind": "cloud",
    "capability": "chat",
    "model_ref": null,
    "provider": "openai",
    "provider_model": "gpt-4.1-mini",
    "host": "127.0.0.1",
    "port": 8780,
    "requested_port": 8780,
    "port_auto": false,
    "bound_port": null,
    "lazy_load": false,
    "idle_seconds": null,
    "created_at": "2026-04-28T00:00:00Z",
    "running": false,
    "process": null,
    "home_dir": "/path/to/tentgent-home",
    "server_dir": "/path/to/tentgent-home/servers/25ee...",
    "spec_path": "/path/to/tentgent-home/servers/25ee.../server.toml",
    "process_path": "/path/to/tentgent-home/servers/25ee.../process.toml",
    "stdout_log": "/path/to/tentgent-home/servers/25ee.../stdout.log",
    "stderr_log": "/path/to/tentgent-home/servers/25ee.../stderr.log"
  },
  "created": true
}
```

`POST /v1/servers/{server_ref}/start` starts one existing server spec in
background mode. `{server_ref}` accepts a full server ref or unique short prefix.
Cloud server starts validate launch-time provider auth from env/keychain and
never persist secrets in the server spec or response.
Local server starts verify that the selected model advertises the server
capability before launching a Rust local-server proxy on the requested port.
The proxy forwards matching `chat`, `embedding`, `rerank`, audio, vision,
video, and image-generation paths to the shared Python model runtime daemon
supervisor. The supervisor starts or reuses the capability/model-bound Python
runtime on demand and lets that Python runtime follow its normal idle shutdown
lifecycle. Requests to those model-bound server ports omit `model` and
`model_kind`; direct Python runtime callers may still provide those fields
explicitly. The server process remains a server lifecycle resource, not a job
record. `idle_seconds`, when set, is passed to the shared Python runtime
supervisor as the idle shutdown policy if the proxy is the process that starts
that capability/model runtime; an already-running shared runtime is reused with
its existing policy. The local proxy does not keep a separate permanent Python
process alive.

The body is optional. Omit it or send `{}` to preserve the original response
shape. Send `wait_ready` to ask the daemon to poll the target server's
`/healthz` after the process starts:

```json
{
  "wait_ready": true,
  "timeout_seconds": 30
}
```

`timeout_seconds` defaults to `30` and must be between `1` and `120`.

An abbreviated response is:

```json
{
  "server": {
    "server_ref": "25ee...",
    "short_ref": "25ee5888595d",
    "runtime_kind": "cloud",
    "capability": "chat",
    "provider": "openai",
    "provider_model": "gpt-4.1-mini",
    "host": "127.0.0.1",
    "port": 8780,
    "requested_port": 8780,
    "port_auto": false,
    "bound_port": 8780,
    "running": true,
    "process": {
      "pid": 12345,
      "launch_mode": "background",
      "started_at": "2026-04-28T00:00:00Z",
      "bound_port": 8780
    }
  }
}
```

With `wait_ready: true`, the response includes readiness. A readiness timeout
does not roll back or stop the launched process:

```json
{
  "server": {
    "server_ref": "25ee...",
    "short_ref": "25ee5888595d",
    "running": true
  },
  "readiness": {
    "ready": true,
    "reachable": true,
    "target_status": 200,
    "target_health": {
      "status": "ok",
      "chat_ready": true
    },
    "checked_at": "2026-04-28T00:00:00Z",
    "error": null
  }
}
```

`POST /v1/servers/{server_ref}/stop` stops one running server process without
removing its stored spec. The response is:

```json
{
  "server": {
    "server_ref": "25ee...",
    "short_ref": "25ee5888595d",
    "running": false,
    "process": null
  },
  "stopped_pid": 12345
}
```

## Native Chat

`POST /v1/chat` invokes kernel chat use cases directly. The daemon chooses the
use case from the request stream flag: omitted or `false` uses `complete_chat`;
`true` uses `stream_chat`. The REST layer does not merge these lower-level
use cases.

```json
{
  "model_ref": "d98392263ae1",
  "messages": [
    {
      "role": "user",
      "content": "Hello"
    }
  ],
  "adapter_ref": null,
  "max_tokens": 128,
  "temperature": 0.0,
  "stream": false
}
```

Selection rules:

- `model_ref` is required and may be a full ref, unique short prefix, source
  repo alias, or source repo basename alias.
- selected models must advertise `chat`; embedding-only and rerank-only models
  return `400 unsupported_target` before runtime dispatch.
- `adapter_ref` is optional and may be a full ref or unique short prefix.
- supported message roles are `system`, `user`, and `assistant`.
- messages are text-only; tools, images, audio, and tool-call transcript roles
  are not accepted in this slice.

Non-streaming success responses are daemon JSON:

```json
{
  "text": "Hello",
  "finish_reason": "stop",
  "model_ref": "d98392263ae1...",
  "adapter_ref": null
}
```

Streaming success responses are native Server-Sent Events:

```text
Content-Type: text/event-stream; charset=utf-8
Cache-Control: no-cache

event: delta
data: {"delta":"H"}

event: done
data: {"finish_reason":"stop"}
```

Daemon-owned native chat errors are JSON before streaming starts and SSE `error`
events after streaming starts:

- invalid JSON, unknown fields, empty prompts, empty content, or unsupported
  roles return `400 bad_request`
- non-chat models selected for chat routes return `400 unsupported_target`
- missing models return `404 not_found`
- ambiguous model aliases or refs return `409 ambiguous_ref`
- model, adapter, runtime, and Python execution failures map to daemon JSON or
  SSE error codes with no provider secrets or raw unbounded logs

## Native Embeddings

`POST /v1/embeddings` invokes the kernel embedding use case directly. It does
not read or write sessions and does not use chat prompts or transcript storage.

Request body:

```json
{
  "model_ref": "d98392263ae1",
  "input": ["first", "second"]
}
```

`input` accepts either one string or a non-empty string array. Empty arrays and
empty strings return `400 bad_request`.

Selection rules:

- `model_ref` is required and may be a full ref, unique short prefix, source
  repo alias, or source repo basename alias.
- selected models must advertise `embedding`; chat-only and rerank-only models
  return `400 unsupported_target` before runtime dispatch.
- implemented local backends include safetensors through the
  `transformers-peft` local-model Python profile and GGUF through
  `llama-cpp-python` embedding mode.
- MLX embedding is a recognized runtime kind, but the Apache-licensed runtime
  returns `501 not_implemented` unless a downstream fork or external runtime
  provides a concrete implementation.
- Cloud provider and rerank embedding paths are not part of this endpoint.

Success responses preserve request order and indexes:

```json
{
  "model_ref": "d98392263ae1...",
  "data": [
    {"index": 0, "embedding": [0.1, 0.2]},
    {"index": 1, "embedding": [0.3, 0.4]}
  ]
}
```

Daemon-owned embedding errors are JSON:

- invalid JSON, unknown fields, empty input, or unsupported input shape return
  `400 bad_request`
- non-embedding models return `400 unsupported_target`
- missing models return `404 not_found`
- ambiguous model aliases or refs return `409 ambiguous_ref`
- runtime dependency, executable, and Python execution failures return
  `500 embedding_runtime_unavailable` or `500 embedding_runtime_failed`

## Native Rerank

`POST /v1/rerank` invokes the kernel rerank use case directly. It does not read
or write sessions and does not use chat prompts or transcript storage.

Request body:

```json
{
  "model_ref": "d98392263ae1",
  "query": "refund policy",
  "documents": ["candidate one", "candidate two"],
  "top_n": 1
}
```

`query` must be a non-empty string. `documents` must be a non-empty string
array. `top_n` is optional and must be between `1` and the number of documents.

Selection rules:

- `model_ref` is required and may be a full ref, unique short prefix, source
  repo alias, or source repo basename alias.
- selected models must advertise `rerank`; chat-only and embedding-only models
  return `400 unsupported_target` before runtime dispatch.
- the implemented local backend is safetensors through the `transformers-peft`
  local-model Python profile using sequence classification.
- MLX rerank is a recognized runtime kind, but the Apache-licensed runtime
  returns `501 not_implemented` unless a downstream fork or external runtime
  provides a concrete implementation.
- GGUF, cloud provider, and embedding backend paths are not part of this
  endpoint.

Success responses are sorted by descending score and preserve original document
indexes:

```json
{
  "model_ref": "d98392263ae1...",
  "data": [
    {"index": 1, "score": 0.91},
    {"index": 0, "score": 0.22}
  ]
}
```

Daemon-owned rerank errors are JSON:

- invalid JSON, unknown fields, empty input, or unsupported input shape return
  `400 bad_request`
- non-rerank models return `400 unsupported_target`
- missing models return `404 not_found`
- ambiguous model aliases or refs return `409 ambiguous_ref`
- runtime dependency, executable, and Python execution failures return
  `500 rerank_runtime_unavailable` or `500 rerank_runtime_failed`

## Compatible Chat Adapters

`POST /v1/chat/completions` is a compatibility route for clients that already
send the OpenAI Chat Completions wire shape. If `model` resolves to a managed
local model or alias, the daemon uses the local chat use case. Otherwise the
route treats `model` as an OpenAI provider model and uses the Rust cloud client.
Success responses are OpenAI-shaped. Daemon-owned errors keep the daemon JSON
error shape.

Request body:

```json
{
  "model": "d98392263ae1",
  "messages": [
    {
      "role": "user",
      "content": "Hello"
    }
  ],
  "max_tokens": 128,
  "temperature": 0.0,
  "stream": false,
  "adapter_ref": "optional-tentgent-adapter-ref"
}
```

In this daemon compatibility route, local model refs and aliases take
precedence. Non-local model names are remote OpenAI provider model names.

MVP limits:

- OpenAI `messages[].content` may be a string or text-only content parts.
- OpenAI roles `developer` and `system` map to kernel `system`; `user` and
  `assistant` map directly.
- `adapter_ref` is a Tentgent extension field; SDK users may need an extra-body
  mechanism to pass it.
- OpenAI tools/function calling, tool-call messages, audio, response formats,
  and non-text modalities return `400 unsupported_provider_field`. OpenAI image
  content parts return `400 unsupported_provider_content`. Image content parts
  are accepted by direct cloud server workers; daemon local-model compatibility
  remains text-only.
- OpenAI-compatible error objects are out of scope; daemon-owned errors keep the
  daemon JSON shape.

Non-streaming success responses map kernel chat text into:

```json
{
  "id": "chatcmpl-...",
  "object": "chat.completion",
  "created": 1770000000,
  "model": "d98392263ae1",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Hello"
      },
      "finish_reason": "stop"
    }
  ]
}
```

Streaming success responses transform kernel stream events into OpenAI-style
chunks and end with `data: [DONE]`:

```text
Content-Type: text/event-stream; charset=utf-8
Cache-Control: no-cache

data: {"id":"chatcmpl-...","object":"chat.completion.chunk","created":1770000000,"model":"d98392263ae1","choices":[{"index":0,"delta":{"content":"H"},"finish_reason":null}]}

data: {"id":"chatcmpl-...","object":"chat.completion.chunk","created":1770000000,"model":"d98392263ae1","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]
```

Claude-compatible messages are exposed at:

```text
POST /v1/messages
```

The request accepts `model`, `system`, `messages`, `max_tokens`, `temperature`,
`stream`, `adapter_ref`, and text-only Claude content blocks. Claude tools and
non-text blocks return `400 unsupported_provider_field` or
`400 unsupported_provider_content`.
Streaming emits Anthropic-style `message_start`, `content_block_delta`, and
`message_stop` events.

Gemini-compatible content generation is exposed at:

```text
POST /v1beta/models/{model}:generateContent
POST /v1beta/models/{model}:streamGenerateContent
```

The request accepts `contents`, `systemInstruction`, `generationConfig`,
`adapter_ref`, and text-only parts. Gemini tools and non-text parts return
`400 unsupported_provider_field` or `400 unsupported_provider_content`.
Unsupported Gemini operations return `400 unsupported_provider_operation`.
Streaming uses Gemini-style SSE data frames.

## Log Diagnostics

Log diagnostics endpoints expose fixed daemon and model-bound server log paths
from Tentgent-managed state. They never accept arbitrary filesystem paths.

Metadata endpoints return stdout/stderr metadata and ignore query parameters:

```text
GET /v1/daemon/logs
GET /v1/servers/{server_ref}/logs
```

```json
{
  "logs": {
    "stdout": {
      "kind": "stdout",
      "path": "/path/to/stdout.log",
      "exists": true,
      "total_bytes": 1234,
      "modified_at": "2026-05-01T00:00:00Z"
    },
    "stderr": {
      "kind": "stderr",
      "path": "/path/to/stderr.log",
      "exists": false,
      "total_bytes": 0,
      "modified_at": null
    }
  }
}
```

Content endpoints return one shared shape for daemon and server logs:

```text
GET /v1/daemon/logs/stdout
GET /v1/daemon/logs/stderr
GET /v1/servers/{server_ref}/logs/stdout
GET /v1/servers/{server_ref}/logs/stderr
```

```json
{
  "log": {
    "owner": "daemon",
    "server_ref": null,
    "short_ref": null,
    "kind": "stdout",
    "path": "/path/to/stdout.log",
    "exists": true,
    "total_bytes": 4096,
    "modified_at": "2026-05-01T00:00:00Z",
    "tail_bytes": 65536,
    "truncated": false,
    "encoding": "utf-8-lossy",
    "content": "..."
  }
}
```

`tail_bytes` applies only to content endpoints. It defaults to `65536`, must be
between `1` and `262144`, and is rejected with `400 bad_request` when it is
zero, negative, non-integer, repeated, or above the maximum.

Log content is tailed by bytes, not lines. `truncated` is true when the log
exists and `total_bytes > tail_bytes`. Content is decoded as UTF-8 lossy so a
byte tail that cuts through a character can still be returned safely.

Missing log files return `200` with `exists: false`, `total_bytes: 0`,
`modified_at: null`, and empty content. Log metadata or read failures other
than missing files return `500 log_read_failed`.

`server_ref` accepts the same full refs and unique prefixes as other server
endpoints. Missing refs return `404 not_found`; ambiguous refs return
`409 ambiguous_ref`.

Log path fields are local diagnostics and may expose local filesystem layout.
They are only served by the loopback-local daemon MVP.

## Request Logging

The daemon logs one stderr request event per handled request:

```text
peer_addr method path status elapsed_ms
```

Request logs must not include bearer token values or authorization header
contents.
