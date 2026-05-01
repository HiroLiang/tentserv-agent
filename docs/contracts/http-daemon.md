# HTTP Daemon

This document defines the first stable HTTP daemon boundary for the Rust
`tentgent-http` entry point and `tentgent daemon run`.

## Scope

- Bind locally by default through `127.0.0.1`.
- Serve JSON responses for daemon-owned routes and errors.
- Pass through model-bound server chat response bodies and content types from
  `POST /v1/chat`, including Server-Sent Events.
- Expose daemon health, status, read-only store discovery, controlled server
  lifecycle mutations, chat proxying, and log diagnostics.
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

## Read-Only Store Discovery

Read-only store endpoints use the daemon runtime home passed to
`tentgent daemon run --home <PATH>` or `tentgent-http --home <PATH>`. Store
specific directory overrides still win when set:

- `TENTGENT_MODELS_DIR`
- `TENTGENT_ADAPTERS_DIR`
- `TENTGENT_DATASETS_DIR`

These endpoints do not mutate state, start servers, stop servers, or proxy chat.

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
