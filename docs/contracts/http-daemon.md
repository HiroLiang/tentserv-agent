# HTTP Daemon

This document defines the first stable HTTP daemon boundary for the Rust
`tentgent-http` entry point and `tentgent daemon run`.

## Scope

- Bind locally by default through `127.0.0.1`.
- Serve JSON responses for every route, including errors.
- Expose daemon health, status, read-only store discovery, and controlled server
  lifecycle mutations.
- Keep the first daemon unauthenticated only for loopback-local MVP usage.

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

- every response must use `Content-Type: application/json; charset=utf-8`
- unknown routes return a JSON `404`
- unsupported methods return a JSON `405`
- invalid requests return a JSON `400`
- ambiguous store references return a JSON `409`
- already-running, not-running, and provider-auth lifecycle conflicts return a
  JSON `409`
- manager parse, IO, and unexpected read errors return a JSON `500` without
  secret values

## Health And Status

`GET /healthz` returns lightweight process health:

```json
{
  "status": "ok",
  "service": "tentgent-daemon",
  "version": "0.1.4"
}
```

`GET /v1/status` returns runtime-home and daemon process metadata.

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

## Request Logging

The daemon logs one stderr request event per handled request:

```text
peer_addr method path status elapsed_ms
```
