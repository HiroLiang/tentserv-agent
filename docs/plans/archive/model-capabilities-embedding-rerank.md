# Model Capabilities: Embedding And Rerank

Deprecated as an active plan. This standalone track was superseded on
2026-05-19 by [Capability-First Release Roadmap](./capability-first-release-roadmap.md).
Keep this file only for historical model-capability context.

This plan separates model storage format from model serving capability so
Tentgent can support non-chat models such as embedding and rerank models without
forcing them through chat-only server flows.

## Current State

- Model metadata records source, primary format, detected formats, file count,
  and size.
- Kernel model domain has a `model_capabilities` metadata field with `chat`,
  `embedding`, and `rerank` values. Old metadata defaults to `chat`, and new
  imports still default to `chat` until import/pull overrides are wired.
- Kernel model domain records `model_capability_source` so callers can
  distinguish default chat fallback, explicit user input, future Hugging Face
  metadata detection, and manual metadata updates.
- Format detection is layout-based: `safetensors`, `gguf`, or `mlx`.
- Server specs distinguish local vs cloud runtimes, but local server behavior is
  chat-oriented.
- Direct server chat and daemon chat contracts are built around `messages`,
  `max_tokens`, `temperature`, and optional `adapter_ref`.
- There are no first-class embedding or rerank endpoints, CLI/HTTP capability
  overrides, or server compatibility checks.
- The kernel has machine-local capability state primitives, but embedding and
  rerank workflows are not yet gated through them.

## Goals

- Represent model capability separately from file format.
- Support at least these capabilities:
  - `chat`
  - `embedding`
  - `rerank`
- Prevent accidental misuse, such as starting a chat server from an embedding
  model or sending chat requests to a rerank runtime.
- Add native daemon/server endpoints for embeddings and rerank before adding
  broad OpenAI-compatible surface area.
- Keep existing chat behavior and session semantics unchanged.

## Non-Goals

- Do not change existing model refs or manifest hash identity just to add
  capability metadata.
- Do not make adapters compatible with embedding/rerank models in the first
  pass.
- Do not add embedding/rerank training execution in this track.
- Do not claim every Hugging Face embedding or rerank architecture is supported
  immediately.
- Do not use embedding/rerank work to invent a second machine capability model;
  local backend readiness should come from kernel capability state in
  [tentgent-kernel-migration.md](./tentgent-kernel-migration.md).

## Proposed Concepts

- `model_capabilities`: stored metadata describing what the model can serve.
- `server_capability`: server spec field that declares the endpoint family a
  server supports.
- `chat` remains the default capability for existing imported models until a
  better detector or explicit override says otherwise.
- Explicit user override should be available for early support because model
  names and file layouts alone are not reliable enough.

Candidate capability values:

```text
chat
embedding
rerank
```

Candidate endpoint families:

```text
POST /v1/chat
POST /v1/embeddings
POST /v1/rerank
```

## Slices

### M1: Capability Metadata And CLI Surface

- Add model capability metadata without changing canonical model identity.
- Add explicit model import/pull override such as `--capability embedding` or
  `--capability rerank`.
- Display capability in model list/inspect output.
- Default existing models to `chat` when capability is absent.
- Update model-store contract docs.

Review target:

- Users and tests can distinguish chat, embedding, and rerank models in the
  store without changing existing model refs.

### M2: Server Compatibility Gates

- Add server capability to local server specs.
- Reject incompatible starts and requests with clear errors:
  - chat endpoint with embedding/rerank server
  - embedding endpoint with chat/rerank server
  - rerank endpoint with chat/embedding server
- Keep direct model-server chat stateless and keep daemon session chat unchanged.
- Document error codes and compatibility rules.

Review target:

- Non-chat models cannot be accidentally served through chat-only code paths.

### M3: Embedding MVP

- Add native `POST /v1/embeddings` through daemon proxy and direct server.
- Support request shape with model/server selection and string or string-array
  input.
- Return vectors with stable JSON shape and input index ordering.
- Implement one local backend path first, likely Python `sentence-transformers`
  or a transformers feature path, chosen after dependency review.
- Gate local backend startup through kernel capability state once runtime
  adapters and backend-gated workflow bundles are in place.
- Add cloud provider support only if the provider mapping is straightforward and
  does not complicate the local MVP.

Review target:

- A managed embedding model can return deterministic embedding arrays through
  the daemon without touching session transcripts.

### M4: Rerank MVP

- Add native `POST /v1/rerank` through daemon proxy and direct server.
- Support request shape with `query`, `documents`, and optional `top_n`.
- Return document indexes and scores, preserving enough data for callers to map
  results back to original inputs.
- Implement one local backend path first, likely a cross-encoder rerank model
  path.
- Gate local backend startup through kernel capability state once runtime
  adapters and backend-gated workflow bundles are in place.
- Do not add session storage or transcript behavior for rerank requests.

Review target:

- A managed rerank model can score candidate documents and return ordered
  results through the daemon.

### M5: OpenAI-Compatible And CLI/Daemon Follow-Up

- Add OpenAI-compatible `/v1/embeddings` only after native embeddings are stable.
- Decide whether rerank needs an OpenAI-compatible route or remains Tentgent
  native.
- Add CLI and daemon REST visibility for model/server capability and prevent
  invalid actions.
- Add docs and command examples for embedding/rerank workflows.

Review target:

- The user-facing surface makes non-chat capabilities visible without bloating
  the chat-first workflow.

## Risks And Notes

- File format is not enough to infer capability. Safetensors can represent chat,
  embedding, rerank, classification, or other model families.
- Capability detection may need Hugging Face metadata such as architecture,
  pipeline tags, or explicit user overrides.
- Embedding and rerank dependencies may increase runtime footprint. Keep
  dependency additions deliberate and documented.
- Local embedding/rerank backend work should depend on kernel manifest-backed
  runtime profile readiness instead of re-probing platform, Python, CPU, or GPU
  state in each endpoint implementation.
- Session context, rolling summaries, and chat streaming should not be reused
  for embedding/rerank requests.

## Verification Themes

- Store tests for default capability, explicit override, and backward
  compatibility with old metadata.
- Server tests for incompatible endpoint/model combinations.
- HTTP tests for embeddings/rerank request validation and response ordering.
- Python runtime tests with small or mocked models before adding heavyweight
  model downloads.
- CLI and daemon REST response tests so users can see model capability before starting a
  server.
