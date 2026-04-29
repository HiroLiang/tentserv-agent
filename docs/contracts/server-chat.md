# Server Chat

This document defines the HTTP chat request contract for the Python server runtime.

## Endpoint

`POST /v1/chat`

Request body:

```json
{
  "messages": [
    {"role": "user", "content": "Hello"}
  ],
  "max_tokens": 128,
  "temperature": 0.0,
  "adapter_ref": "optional-adapter-ref",
  "stream": false
}
```

Non-streaming responses are JSON encoded as UTF-8. Non-ASCII text should remain
readable in the response body rather than being escaped as `\uXXXX` sequences.

## Streaming Contract

When `stream = true` is supported by the selected runtime, the response uses
Server-Sent Events:

```text
Content-Type: text/event-stream; charset=utf-8
Cache-Control: no-cache
```

Delta events carry incremental text:

```text
event: delta
data: {"delta":"..."}
```

Completion events end the stream:

```text
event: done
data: {"finish_reason":"stop"}
```

If a runtime error happens after the stream has started, the server emits an
SSE error event instead of switching back to a JSON response:

```text
event: error
data: {"error":"runtime_error","message":"..."}
```

Preflight validation errors, adapter lookup errors, provider setup errors, and
runtimes without streaming support must return normal JSON errors before SSE
headers are sent.

Current streaming support is intentionally staged:

- local base-model requests can stream through the selected backend
- compatible local adapter requests can stream after adapter validation and request-time adapter selection
- OpenAI and Anthropic cloud provider runtimes stream through the same SSE events after provider delta normalization

## Adapter Contract

`adapter_ref` is optional.

When `adapter_ref` is present, the server must validate the adapter before generation:

- the adapter exists in `TENTGENT_HOME/adapters`
- the adapter is compatible with the server model
- the adapter `backend_support` includes the server backend
- the runtime has implemented request-time adapter execution for that backend

Compatibility is considered proven when:

- `base_model_ref` matches the server model, or
- `base_model_source_repo` matches the server model source repo

If both sides provide source revisions, they must match.

## Backend Status

- `transformers-peft`
  Request-time PEFT adapter execution is implemented for managed adapters whose
  `backend_support` includes `transformers-peft`.
- `mlx`
  Request-time MLX adapter execution is implemented for managed adapters whose
  `backend_support` includes `mlx`.
- `llama-cpp`
  Contract-recognized as a backend support value, but external adapter execution is not implemented in this MVP.

For backends that do not yet implement adapter execution, a compatible adapter request should return a clear `501` instead of silently ignoring the adapter.

## Runtime Behavior

The first implemented paths are conservative:

- the server loads the base `safetensors` model through `transformers`
- a compatible adapter is loaded from the managed adapter `source/` directory with PEFT
- adapters are selected per request with `adapter_ref`
- requests without `adapter_ref` run with adapters disabled, even if a previous request used one
- loaded adapters may stay attached to the long-lived backend until the server releases the model
- the MLX backend uses `mlx_lm.load(..., adapter_path=...)` for compatible adapters
- changing the active MLX adapter reloads the MLX model because MLX adapter loading mutates model structure
- clearing an MLX adapter reloads the base model to avoid adapter state leaking into base-model requests

## Cloud Provider Client Boundary

Cloud provider runtimes should reuse the same normalized `messages`, `max_tokens`, and `temperature` request fields before provider-specific mapping.

- OpenAI maps normalized messages directly to chat-completion messages.
- Anthropic maps `system` messages to the top-level `system` field and sends only `user` and `assistant` messages in `messages`.
- Provider API keys must be supplied by launch-time environment and must not be stored in server specs or response errors.
- Cloud provider runtimes do not load local model records and report `cloud_proxy` in health snapshots.
- `adapter_ref` is not supported for cloud provider runtimes and should return a clear `501`.
- Cloud provider streaming must normalize provider-specific delta events into the same SSE `delta` events used by local runtimes.
- Provider request and response parsing should live outside HTTP handlers so it can be tested with mocked transports.

## Error Mapping

- `400 invalid_request`
  Request shape is invalid.
- `404 adapter_not_found`
  The requested adapter reference does not resolve.
- `409 adapter_ambiguous`
  The adapter prefix matches multiple adapters.
- `409 adapter_incompatible`
  The adapter exists but cannot be proven compatible with the server model.
- `501 adapter_backend_unsupported`
  The adapter exists but does not support the server backend.
- `501 adapter_execution_not_implemented`
  The adapter is recognized and compatible, but runtime execution is not implemented yet.
- `501 stream_not_implemented`
  HTTP streaming is requested for a runtime path that has not wired token streaming yet.
