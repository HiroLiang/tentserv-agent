# M4 Embedding MVP

This is the focused execution plan for M4 in the
[capability-first release roadmap](./capability-first-release-roadmap.md).

Status: implemented.

## Goal

- Add a native embedding endpoint family without using chat sessions or chat
  transcript storage.
- Serve managed local embedding models through daemon REST and direct local
  server paths.
- Return vectors for string and string-array input with stable output ordering.
- Gate endpoint use by model capability and backend readiness.

## Non-Goals

- Do not implement rerank; that remains M5.
- Do not add OpenAI-compatible expansion beyond the embedding request/response
  shape needed for this MVP.
- Do not add audio or multimodal support.
- Do not use chat prompts, chat sessions, or transcript compaction for
  embeddings.
- Do not silently coerce chat models into embedding models.

## Implemented Slice

- Added `features/embedding` in the Rust kernel with domain types, model
  resolver, use case, and Python one-shot runtime client.
- Added daemon REST `POST /v1/embeddings` with string and string-array input,
  model alias resolution, capability gates, and session-free execution.
- Added Python `tentgent-embed-once` and direct server `POST /v1/embeddings`.
- Chose the first backend path as safetensors through the existing
  `transformers-peft` local-model profile using `AutoModel` mean pooling.
- Enabled local embedding server specs with `--capability embedding` while
  preserving legacy chat server identity.
- Added embedding backend capability probing through the local-model dependency
  set; rerank remains unknown/deferred.

## Starting Baseline

- M1/M2 provide `model_capabilities` metadata and correction paths.
- M3 adds server capability metadata and endpoint-family gates.
- `docs/user/commands.md` previously stated that embedding runtime endpoints
  were not implemented.
- Backend capability probes previously reported embedding readiness as unknown.

## API Contract

### Daemon REST

Add native:

```text
POST /v1/embeddings
```

Request body:

```json
{
  "model_ref": "model-ref-or-alias",
  "input": "hello world"
}
```

Also accept string arrays:

```json
{
  "model_ref": "model-ref-or-alias",
  "input": ["first", "second"]
}
```

Response body:

```json
{
  "model_ref": "resolved-model-ref",
  "data": [
    {
      "index": 0,
      "embedding": [0.1, 0.2, 0.3]
    }
  ]
}
```

MVP rules:

- Reject empty strings and empty arrays with `400 bad_request`.
- Preserve input order and return matching indexes.
- Resolve `model_ref` the same way chat routes resolve full refs, unique
  prefixes, source repo aliases, and source repo basenames.
- Reject models that do not advertise `embedding` with
  `400 unsupported_target`.
- Return clear runtime/backend readiness errors before model execution when no
  embedding backend is available.

### Direct Local Server

Add `POST /v1/embeddings` to the Python direct server process only for local
server specs whose capability is `embedding`.

- Chat servers must reject embedding requests.
- Embedding servers must reject chat requests.
- Direct local server handling must remain stateless and must not read or write
  Tentgent sessions.

## Execution Slices

### 1. Kernel Domain And Ports

- Add an embedding feature package or focused module matching existing feature
  boundaries.
- Define request, response, runtime target, and backend result domain types.
- Add an embedding model resolver that requires `ModelCapability::Embedding`.
- Add an embedding runtime client port separate from chat runtime clients.
- Keep model alias resolution shared or factored without coupling embedding to
  chat request types.

### 2. Backend Selection

- Pick the first local backend path after a dependency check:
  - prefer `sentence-transformers` if it can fit the Python runtime packaging
    and Apple/Linux constraints, or
  - use a targeted `transformers` feature-extraction path if that is lighter
    and stable enough.
- Document the selected backend before implementation.
- Add capability-state checks so backend readiness is reported separately from
  stored model metadata.

### 3. Python Runtime

- Add a Python embedding runtime entrypoint for one-shot daemon calls.
- Add direct server request parsing and response serialization for
  `/v1/embeddings`.
- Keep normalized embedding logic outside HTTP handlers so it can be tested with
  mocked backend objects.
- Return vectors as JSON numbers without stringifying numeric values.

### 4. Daemon REST

- Add `POST /v1/embeddings` route under daemon REST.
- Parse daemon request DTOs into kernel embedding use cases.
- Map invalid input to `400 bad_request`, missing/ambiguous model selectors to
  existing lookup semantics, incompatible models to `400 unsupported_target`,
  and runtime readiness failures to a clear backend/runtime error.
- Do not add session fields or transcript writes.

### 5. Server Lifecycle

- Add a way to create or prepare embedding-capable local server specs only when
  the selected model advertises `embedding`.
- Keep existing chat server identity behavior stable. If server capability enters
  identity for non-chat specs, add tests proving old chat refs are unchanged.
- Ensure `resolve_for_start` gates embedding specs by model capability.

### 6. Tests

- Kernel tests:
  - embedding model resolver accepts embedding models
  - embedding resolver rejects chat and rerank models
  - response order matches input order
- Daemon REST tests:
  - string input returns one vector from a mocked runtime path
  - string-array input preserves indexes
  - empty input rejected
  - chat/rerank model rejected
  - no session files are created
- Python tests:
  - request parser validates string and string-array inputs
  - mocked backend response serializes numeric vectors
  - direct server rejects chat-only model/server mismatch
- Server tests:
  - embedding server spec exposes `capability: "embedding"`
  - chat endpoint with embedding server is rejected
  - embedding endpoint with chat server is rejected

## Documentation

- Update `docs/contracts/http-daemon.md` with `/v1/embeddings` once the daemon
  request/response shape is stable.
- Update `docs/contracts/server-chat.md` or add a sibling contract if direct
  Python server embedding behavior needs a separate boundary.
- Update `docs/user/commands.md` only after the endpoint is implemented and the
  first backend path is known.
- Update `docs/user/runtime.md` and `docs/user/version.md` when backend
  readiness behavior changes.

## Verification

Run focused Rust checks:

```bash
cargo test -p tentgent-kernel embedding
cargo test -p tentgent-daemon embedding
cargo test -p tentgent-kernel server
cargo test -p tentgent-daemon server
cargo check --workspace
```

Run focused Python checks from `python/tentgent-daemon/` after the Python
runtime slice lands:

```bash
uv run pytest
```

## Completion Criteria

- A managed embedding model can return vectors through daemon REST.
- Direct local embedding server requests work through `/v1/embeddings`.
- Chat models cannot be used through the embedding endpoint family.
- Embedding requests do not create or mutate sessions/transcripts.
- Docs no longer say embedding endpoints are missing once the endpoint is
  implemented.
