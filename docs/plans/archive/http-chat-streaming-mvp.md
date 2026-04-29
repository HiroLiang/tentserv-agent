# HTTP Chat Streaming MVP

Status: archived. This plan completed Server-Sent Events streaming for local
base-model chat, request-time local adapter chat, and OpenAI/Anthropic cloud
provider chat through the existing `POST /v1/chat` endpoint.

Add a streaming response path for `POST /v1/chat` so local servers can return tokens incrementally when clients send `stream: true`.

## Priority

Do this before the broader HTTP daemon and TUI tracks. Streaming is a narrow server contract that improves adapter smoke tests now and gives the future daemon and TUI a stable response protocol to reuse.

Recommended plan order:

1. HTTP Chat Streaming MVP
2. Cloud Provider Server MVP polish and docs
3. HTTP Daemon MVP
4. TUI Session MVP

## Scope

- Support `stream: true` on the existing `/v1/chat` endpoint.
- Use Server-Sent Events for the first public streaming protocol.
- Keep the current non-streaming JSON response unchanged.
- Support local model runtimes first, including request-time `adapter_ref`.
- Preserve the current `stream_not_implemented` error for runtimes that cannot stream yet.

## Non-Goals

- Do not add WebSocket, gRPC, or OpenAI-compatible route aliases in this slice.
- Do not add durable chat session storage.
- Do not implement TUI chat playback.
- Do not require cloud provider streaming before local streaming is stable.

## HTTP Contract

Request shape stays the same:

```json
{
  "messages": [
    {"role": "user", "content": "幫我列三個今天下午安排工作的建議。"}
  ],
  "adapter_ref": "4012b081478d",
  "max_tokens": 160,
  "temperature": 0.2,
  "stream": true
}
```

Streaming responses use:

```text
Content-Type: text/event-stream; charset=utf-8
Cache-Control: no-cache
```

Events:

```text
event: delta
data: {"delta":"..."}

event: done
data: {"finish_reason":"stop"}
```

Errors after the stream starts are sent as:

```text
event: error
data: {"error":"runtime_error","message":"..."}
```

Preflight validation errors should still return normal JSON errors before the stream starts.

## Execution Order

### Slice 1: SSE Response Contract

Status: implemented. The SSE event serializer and HTTP header writer are in
place; local base-model runtime token streaming is now wired by Slice 2.

Define the HTTP writer boundary and tests.

Goals:

- parse `stream: true` without changing non-streaming requests
- write SSE headers and event formatting helpers
- keep malformed requests and unsupported runtimes on normal JSON errors
- add tests for delta, done, and error event serialization

Review target:

- `/v1/chat` has a stable streaming wire format even before runtime token streaming is wired in

### Slice 2: Local Runtime Stream Boundary

Status: implemented. Local requests can now stream backend deltas through the
existing `/v1/chat` route with `stream: true`; Slice 3 extends the same route to
compatible adapters.

Expose an iterator or callback from the Python runtime session.

Goals:

- add a backend-neutral streaming method beside the existing blocking chat method
- stream local MLX output where the backend supports incremental generation
- keep PEFT and llama-cpp behavior explicit if they remain non-streaming in this slice
- ensure lazy-load and idle-release state updates still run

Review target:

- local non-adapter chat can stream deltas through `curl -N`

### Slice 3: Adapter Streaming Path

Status: implemented. Streaming requests with `adapter_ref` now reuse the same
adapter lookup, compatibility check, backend support check, and request-time
selection path as non-streaming chat before SSE headers are sent.

Make request-time adapters work through the same streaming route.

Goals:

- validate `adapter_ref` before starting the SSE response
- load or switch adapters before first token emission
- keep adapter mismatch errors as normal JSON preflight failures
- add a smoke test for an adapter request with `stream: true`

Review target:

- the existing adapter test flow can compare base vs adapter output through streaming

### Slice 4: Cloud Runtime Streaming Follow-Up

Status: implemented. OpenAI and Anthropic provider stream events are normalized
inside the provider client layer and exposed through the same SSE `delta`,
`done`, and `error` contract as local runtimes.

Add provider streaming only after the local streaming contract is stable.

Goals:

- normalize OpenAI and Anthropic provider deltas into the same SSE events
- hide provider-specific event shapes from clients
- keep provider auth and model errors as preflight JSON when possible

Review target:

- cloud servers preserve the same client protocol as local servers

### Slice 5: User Docs And Smoke Commands

Status: implemented. User and development docs now include non-streaming JSON,
local SSE, adapter SSE, and cloud SSE smoke commands, plus the current
local-only adapter limit for cloud provider servers.

Document the user-facing command and curl shape.

Goals:

- update command docs with `curl -N` examples
- mention that non-streaming JSON remains the default
- document provider/runtime limits for streaming

Review target:

- users can test streaming locally without reading implementation files
