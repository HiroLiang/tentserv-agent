# M5 Rerank MVP

This is the focused execution plan for M5 in the
[capability-first release roadmap](./capability-first-release-roadmap.md).

Status: implemented.

## Implemented Slice

- Added `features/rerank` in the Rust kernel with domain types, model resolver,
  use case, and Python one-shot runtime client.
- Added daemon REST `POST /v1/rerank` with query/document input validation,
  model alias resolution, capability gates, `top_n`, and session-free
  execution.
- Added Python `tentgent-rerank-once` and direct server `POST /v1/rerank`.
- Added CLI one-shot `tentgent embed` and `tentgent rerank` commands that call
  the same kernel/runtime path without requiring a running daemon.
- Chose the first backend path as safetensors through the existing
  `transformers-peft` local-model profile using
  `AutoModelForSequenceClassification`.
- Enabled local rerank server specs with `--capability rerank` while preserving
  existing chat and embedding server identity behavior.
- Added rerank backend capability probing through the local-model dependency
  set.

## Goal

- Add a native rerank endpoint family without using chat sessions or chat
  transcript storage.
- Serve managed local rerank models through daemon REST and direct local server
  paths.
- Score one query against a non-empty list of candidate documents.
- Return ranked original document indexes and scores, with optional `top_n`
  truncation.
- Gate endpoint use by model capability and backend readiness.

## Non-Goals

- Do not add audio or multimodal support.
- Do not add broad OpenAI-compatible expansion beyond the native rerank shape.
- Do not use chat prompts, chat sessions, or transcript compaction for rerank.
- Do not silently coerce chat or embedding models into rerank models.
- Do not claim score calibration across different rerank model families.
- Do not implement cloud provider rerank servers in this slice.

## Starting Baseline

- M1/M2 provide `model_capabilities` metadata and correction paths.
- M3 adds server capability metadata and endpoint-family gates.
- M4 adds embedding endpoint/runtime patterns that M5 should mirror where useful.
- Rerank metadata could already be stored before this slice, but rerank runtime
  execution was rejected or reported as unknown/deferred.

## API Contract

### Daemon REST

Add native:

```text
POST /v1/rerank
```

Request body:

```json
{
  "model_ref": "model-ref-or-alias",
  "query": "refund policy",
  "documents": ["candidate one", "candidate two"],
  "top_n": 1
}
```

Response body:

```json
{
  "model_ref": "resolved-model-ref",
  "data": [
    {
      "index": 1,
      "score": 0.91
    }
  ]
}
```

MVP rules:

- Reject empty or blank `query` with `400 bad_request`.
- Reject empty arrays, blank documents, non-string documents, and unknown fields
  with `400 bad_request`.
- `top_n` is optional. When omitted, return all documents sorted by descending
  score.
- Reject `top_n` values less than 1 or greater than the number of documents
  with `400 bad_request`.
- Preserve original document indexes in the response.
- Sort by descending score and use original index as the deterministic tie
  breaker.
- Resolve `model_ref` the same way chat and embedding routes resolve full refs,
  unique prefixes, source repo aliases, and source repo basenames.
- Reject models that do not advertise `rerank` with `400 unsupported_target`.
- Return clear runtime/backend readiness errors before model execution when no
  rerank backend is available.

### Direct Local Server

Add `POST /v1/rerank` to the Python direct server process only for local server
specs whose capability is `rerank`.

- Chat and embedding servers must reject rerank requests.
- Rerank servers must reject chat and embedding requests.
- Direct local server handling must remain stateless and must not read or write
  Tentgent sessions.
- Cloud provider direct servers remain chat-only.

## Backend Selection

Use the existing local-model Python dependency set for the first backend path:

- support managed safetensors rerank models first
- load with `AutoTokenizer` and `AutoModelForSequenceClassification`
- score query/document pairs with the model's scalar logits
- return raw backend scores as JSON numbers; clients should not compare scores
  across unrelated model families

Do not add `sentence-transformers` as a new dependency unless the sequence
classification path proves insufficient during implementation.

## Execution Slices

### 1. Kernel Domain And Ports

- Add a `features/rerank` package matching the embedding feature shape.
- Define request, response, runtime target, backend result, and validation
  domain types.
- Add a rerank model resolver that requires `ModelCapability::Rerank`.
- Add rerank runtime client ports separate from chat and embedding clients.
- Keep model alias resolution shared or factored without coupling rerank to chat
  or embedding request types.

### 2. Backend Readiness And Selection

- Change `BackendKind::Rerank` probing from unknown/deferred to the selected
  local-model dependency set once the runtime path is implemented.
- Add a rerank backend enum that maps safetensors models to the transformers
  sequence-classification path.
- Keep GGUF, MLX, cloud provider, and embedding backends out of the rerank MVP.

### 3. Python Runtime

- Add a Python one-shot rerank entrypoint for daemon calls.
- Add direct server request parsing and response serialization for `/v1/rerank`.
- Add a `RerankBackend` contract and transformers sequence-classification
  implementation.
- Keep normalized rerank logic outside HTTP handlers so it can be tested with
  mocked backend objects.
- Return scores as JSON numbers without stringifying numeric values.

### 4. Daemon REST

- Add `POST /v1/rerank` under daemon REST.
- Parse daemon request DTOs into kernel rerank use cases.
- Map invalid input to `400 bad_request`, missing/ambiguous model selectors to
  existing lookup semantics, incompatible models to `400 unsupported_target`,
  and runtime readiness failures to a clear backend/runtime error.
- Do not add session fields or transcript writes.

### 5. Server Lifecycle

- Enable rerank-capable local server specs only when the selected model
  advertises `rerank`.
- Remove the Python server CLI guard that currently rejects
  `--capability rerank` after the rerank route and backend path exist.
- Preserve existing chat and embedding server identity behavior.
- Ensure `resolve_for_start` gates rerank specs by model capability and backend
  implementation status.

### 6. CLI One-Shot Inference

- Add `tentgent embed <model-ref> --input <text>` for direct local embedding
  smoke tests and scripts.
- Add `tentgent rerank <model-ref> --query <text> --document <text>` for direct
  local rerank smoke tests and scripts.
- Return JSON with the resolved `model_ref` and the same `data` array shape as
  daemon REST.
- Keep these commands session-free and daemon-free. They may reload the model on
  each invocation; long-lived or repeated traffic should use daemon REST or a
  direct server process.
- Preserve endpoint-family gates by using the existing embedding and rerank
  kernel use cases.

### 7. Tests

- Kernel tests:
  - rerank model resolver accepts rerank safetensors models
  - rerank resolver rejects chat and embedding models
  - input validation rejects blank query, empty documents, blank documents, and
    invalid `top_n`
  - ranking output sorts by score and preserves original indexes
- Daemon REST tests:
  - valid request returns ranked scores from a mocked runtime path
  - `top_n` truncates ranked output
  - invalid input is rejected
  - chat and embedding models are rejected before runtime dispatch
  - no session files are created
- Python tests:
  - request parser validates `query`, `documents`, and `top_n`
  - mocked backend response serializes numeric scores
  - direct server rejects endpoint-family mismatches
- Server tests:
  - rerank server spec exposes `capability: "rerank"`
  - chat and embedding endpoints with rerank server are rejected
  - rerank endpoint with chat or embedding server is rejected

## Documentation

- Update `docs/contracts/http-daemon.md` with `/v1/rerank` once the daemon
  request/response shape is stable.
- Add `docs/contracts/server-rerank.md` or a focused sibling contract if direct
  Python server rerank behavior needs its own boundary.
- Update `docs/user/commands.md` only after the endpoint is implemented and the
  first backend path is known.
- Update `docs/user/runtime.md` and `docs/user/version.md` when backend
  readiness behavior changes.

## Verification

Run focused Rust checks:

```bash
cargo test -p tentgent-kernel rerank
cargo test -p tentgent-daemon rerank
cargo test -p tentgent-cli embed
cargo test -p tentgent-cli rerank
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

- A managed rerank model can score candidate documents through daemon REST.
- Direct local rerank server requests work through `/v1/rerank`.
- CLI one-shot embedding and rerank commands can call managed local models
  without a running daemon.
- Chat and embedding models cannot be used through the rerank endpoint family.
- Rerank requests do not create or mutate sessions/transcripts.
- Docs no longer say rerank endpoints are missing once the endpoint is
  implemented.
