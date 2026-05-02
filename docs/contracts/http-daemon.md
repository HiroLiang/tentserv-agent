# HTTP Daemon

This document defines the first stable HTTP daemon boundary for the Rust
`tentgent-http` entry point and `tentgent daemon run`.

## Scope

- Bind locally by default through `127.0.0.1`.
- Serve JSON responses for daemon-owned routes and errors.
- Pass through model-bound server chat response bodies and content types from
  `POST /v1/chat`, including Server-Sent Events.
- Expose a limited OpenAI-style chat-completions compatibility route at
  `POST /v1/chat/completions`.
- Expose daemon health, status, read-only store discovery, controlled server
  lifecycle mutations, store import/pull mutations, deterministic dataset
  tooling, cloud dataset tooling, LoRA train-plan management, auth status,
  doctor diagnostics, daemon shutdown control, chat proxying, log diagnostics,
  session discovery, and explicit session mutation.
- Keep loopback-local daemon development usable without auth, while requiring a
  token or explicit unsafe flag for non-loopback and wildcard binds.

## Version Source

HTTP daemon responses should use the shared Rust version source:

```text
tentgent_core::VERSION
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
- chat proxy responses preserve the selected model-bound server status, body,
  and content type
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
status responses. Model-bound server child processes do not inherit
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

`--allow-unsafe-bind` is available on both `tentgent daemon run` and the
low-level `tentgent-http` binary. It is intended only for explicit local-network
experiments; this MVP is not a public-service security model and does not add
TLS, CORS, multi-user auth, keychain token storage, or per-endpoint permissions.

## Health And Status

`GET /healthz` returns lightweight process health:

```json
{
  "status": "ok",
  "service": "tentgent-daemon",
  "version": "0.1.4"
}
```

`GET /v1/status` returns runtime-home, daemon process metadata, and non-secret
auth state:

```json
{
  "service": "tentgent-daemon",
  "version": "0.1.4",
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
  "pid_path": "/path/to/tentgent-home/runtime/daemon/daemon.pid"
}
```

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
```

Auth status endpoints may reveal whether credentials are configured, but never
credential values. Environment-variable credentials bypass Keychain reads.
When no env override is present, checking Keychain presence may trigger the
platform Keychain prompt.

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
servers. Unlike most loopback-local daemon routes, shutdown requires
`TENTGENT_DAEMON_TOKEN` to be enabled and a valid bearer token; otherwise it
returns `409 daemon_token_required` or `401 unauthorized`.

## Read-Only Store Discovery

Read-only store endpoints use the daemon runtime home passed to
`tentgent daemon run --home <PATH>` or `tentgent-http --home <PATH>`. Store
specific directory overrides still win when set:

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
    "source_kind": "huggingface",
    "source_repo": "mlx-community/Llama-3.2-1B-Instruct-MLXTuned",
    "source_revision": "7247...",
    "source_path": null,
    "manifest_path": "/path/to/models/store/8fac.../manifest.json",
    "variant_source_path": "/path/to/models/store/8fac.../variants/mlx/source"
  }
}
```

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

`POST /v1/sessions/{session_ref}/messages` appends one to 100 messages.
Tentgent assigns `created_at` and returns appended indexes:

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

`DELETE /v1/sessions/{session_ref}` permanently removes the session directory.
There is no trash or recycle bin. DELETE bodies must be empty. All session
mutation endpoints follow daemon auth rules.

Session writes use lock files to coordinate CLI and HTTP writers. `409
session_busy` means another writer held the lock for the acquisition timeout.
Session-aware chat remains out of scope in this slice; `/v1/chat` and
`/v1/chat/completions` stay stateless.

`GET /v1/servers` returns stored server specs and their current process state:

```json
{
  "servers": [
    {
      "server_ref": "25ee...",
      "short_ref": "25ee5888595d",
      "runtime_kind": "cloud",
      "model_ref": null,
      "provider": "openai",
      "provider_model": "gpt-4.1-mini",
      "host": "127.0.0.1",
      "port": 8780,
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
    "model_ref": null,
    "provider": "openai",
    "provider_model": "gpt-4.1-mini",
    "host": "127.0.0.1",
    "port": 8780,
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
{ "path": "/absolute/path/on/daemon-host", "base_model_ref": "optional" }
{ "repo_id": "owner/name", "revision": null }
{ "repo_id": "owner/name", "revision": "main", "base_model_ref": "optional" }
{ "base_model_ref": "model-ref-or-prefix" }
```

Request bodies reject unknown fields. `repo_id` must be a Hugging Face repo id
such as `owner/name`, not a URL or `/tree/...` path. Omitted or `null`
`revision` uses the core default; blank `revision` returns JSON `400`.
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
  }
}
```

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

These endpoints are synchronous MVP calls. Large local imports or Hugging Face
pulls may exceed client timeouts until future job/progress APIs exist.

## Dataset Deterministic Tools

The daemon exposes provider-free dataset tooling over HTTP:

```text
POST /v1/datasets/validate
POST /v1/datasets/template
POST /v1/datasets/synth
POST /v1/datasets/eval
POST /v1/datasets/{dataset_ref}/export
POST /v1/datasets/{dataset_ref}/diff
```

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

The daemon exposes synchronous provider-backed dataset tooling:

```text
POST /v1/datasets/synth
POST /v1/datasets/eval
```

These endpoints follow daemon auth rules and may call OpenAI or Anthropic with
selected local content. All path fields are daemon-host paths. Direct content is
for dataset/spec-sized inputs only; multipart upload and background jobs are
future work.

`POST /v1/datasets/synth` accepts exactly one prompt source: `brief`,
`spec_content`, or absolute `spec_path`. Prompt-only mode does not require
provider auth:

```json
{
  "print_prompt": true,
  "brief": "Generate support examples in Traditional Chinese.",
  "split": "train",
  "count": 20
}
```

Provider mode requires `provider`, `model`, `output_path`, and explicit counts:

```json
{
  "provider": "openai",
  "model": "gpt-4.1-mini",
  "output_path": "/absolute/path/on/daemon-host/generated",
  "brief": "Generate support examples in Traditional Chinese.",
  "split": "train",
  "count": 20,
  "timeout_seconds": 300,
  "retries": 1
}
```

Single-split mode uses `split` plus `count`. Multi-split mode uses one or more
of `train_count`, `valid_count`, `test_count`, and `eval_count`; zero-valued
split counts are ignored, and at least one split count must be greater than
zero. `spec_content` is limited to 256 KiB. `output_path` must be missing or an
empty directory. Failed provider runs may leave partial output or `_debug`
artifacts and are not rolled back.

Prompt-only responses return:

```json
{
  "prompt": {
    "content": "...",
    "split": "train",
    "source_kind": "brief"
  }
}
```

Provider responses return the Python runtime outcome and capped progress events:

```json
{
  "synth": {
    "provider": "openai",
    "model": "gpt-4.1-mini",
    "output_dir": "/path/generated",
    "manifest_path": "/path/generated/manifest.json",
    "record_count": 20,
    "splits": []
  },
  "progress_events": [],
  "progress_truncated": false
}
```

`POST /v1/datasets/eval` accepts exactly one input source: managed
`dataset_ref`, direct `input_content`, or absolute `input_path`.

```json
{
  "dataset_ref": "managed-ref-or-prefix",
  "provider": "openai",
  "model": "gpt-4.1-mini",
  "output_path": "/absolute/path/on/daemon-host/eval-report",
  "max_records": 20
}
```

`dataset_ref` resolves to the managed dataset store path, not the original
source path. `input_content` defaults to `input_format: "jsonl"` and is limited
to 10 MiB; it is staged under the daemon runtime home before the provider
runtime runs. `output_path` must be missing or empty. Successful eval responses
wrap the Python report outcome:

```json
{
  "eval": {
    "provider": "openai",
    "model": "gpt-4.1-mini",
    "input_path": "/path/input",
    "output_dir": "/path/eval-report",
    "report_json_path": "/path/eval-report/eval-report.json",
    "report_md_path": "/path/eval-report/eval-report.md",
    "reviewed_records": 20,
    "overall_score": 0.82,
    "warnings": []
  }
}
```

Bad JSON, unknown fields, invalid modes, invalid counts, relative paths, or
oversized content return `400 bad_request`. Missing input/spec paths return
`404 path_not_found`; missing/ambiguous dataset refs return `404 not_found` or
`409 ambiguous_ref`; non-empty output directories return `409 output_exists`;
provider auth failures return `409 provider_auth_failed`. Provider/runtime
failures return `502 dataset_synth_failed` or `502 dataset_eval_failed` with
debug artifact paths when available, but never raw provider output or raw
request content.

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

The daemon exposes safe remove parity for managed store entries:

```text
DELETE /v1/models/{model_ref}
DELETE /v1/adapters/{adapter_ref}
DELETE /v1/datasets/{dataset_ref}
DELETE /v1/servers/{server_ref}
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
  "host": "127.0.0.1",
  "port": 8780,
  "lazy_load": false,
  "idle_seconds": null
}
```

An abbreviated response is:

```json
{
  "server": {
    "server_ref": "25ee...",
    "short_ref": "25ee5888595d",
    "runtime_kind": "cloud",
    "model_ref": null,
    "provider": "openai",
    "provider_model": "gpt-4.1-mini",
    "host": "127.0.0.1",
    "port": 8780,
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
    "provider": "openai",
    "provider_model": "gpt-4.1-mini",
    "running": true,
    "process": {
      "pid": 12345,
      "launch_mode": "background",
      "started_at": "2026-04-28T00:00:00Z"
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

## Chat Proxy

`POST /v1/chat` proxies a chat request to an already-running model-bound server.
The request body follows [server-chat.md](./server-chat.md) and adds one optional
daemon-only selector:

```json
{
  "server_ref": "25ee5888595d",
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

- when `server_ref` is present, it may be a full ref or unique short prefix and
  must resolve to a running server
- when `server_ref` is absent, exactly one server must be running
- the daemon removes `server_ref` before forwarding the request to the selected
  server's `POST /v1/chat`
- the daemon does not auto-start stopped servers

Non-streaming responses preserve the selected server status code, response body,
and `Content-Type`.

Streaming responses preserve Server-Sent Event bytes from the selected server and
return:

```text
Content-Type: text/event-stream; charset=utf-8
Cache-Control: no-cache
```

Daemon-owned chat selection and proxy errors are JSON:

- invalid JSON or invalid `server_ref` shape returns `400 bad_request`
- missing explicit `server_ref` returns `404 not_found`
- ambiguous explicit `server_ref` returns `409 ambiguous_ref`
- selected stopped server returns `409 server_not_running`
- no running server returns `409 no_running_server`
- multiple running servers without `server_ref` returns `409 ambiguous_server`
- target connection or transport failures return `502 server_proxy_failed`
  with a hint to inspect `GET /v1/servers/{server_ref}/health`

If the selected server returns its own chat error, the daemon passes through that
status, body, and content type unchanged.

## OpenAI-Style Chat Completions

`POST /v1/chat/completions` is a limited compatibility route for local clients
that already send the OpenAI Chat Completions wire shape. Success responses are
OpenAI-shaped. Daemon-owned errors keep the daemon JSON error shape.

Request body:

```json
{
  "model": "25ee5888595d",
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

In this daemon compatibility route, `model` selects a Tentgent server reference
or unique short prefix. It is not a provider model name. The selected server must
already be running; the daemon does not auto-start stopped servers.

MVP limits:

- `messages[].content` must be a string
- supported roles are `system`, `user`, and `assistant`
- unsupported OpenAI request fields are ignored
- multimodal content, tools/function calling, model-name routing, session
  persistence, and OpenAI-compatible error objects are out of scope

Non-streaming success responses map target `{ "text": "..." }` payloads into:

```json
{
  "id": "chatcmpl-...",
  "object": "chat.completion",
  "created": 1770000000,
  "model": "25ee-full-server-ref",
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

Streaming success responses transform Tentgent server SSE into OpenAI-style
chunks and end with `data: [DONE]`:

```text
Content-Type: text/event-stream; charset=utf-8
Cache-Control: no-cache

data: {"id":"chatcmpl-...","object":"chat.completion.chunk","created":1770000000,"model":"25ee-full-server-ref","choices":[{"index":0,"delta":{"content":"H"},"finish_reason":null}]}

data: {"id":"chatcmpl-...","object":"chat.completion.chunk","created":1770000000,"model":"25ee-full-server-ref","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]
```

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
