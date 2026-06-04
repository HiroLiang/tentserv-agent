# Provider Compatibility Audit

Status: initial implementation audit for `v0.6.0` compatibility-contract work.

This audit records the current behavior of Tentgent provider-shaped HTTP
routes. It is engineering reference material for the user-facing compatibility
matrix, unsupported-field semantics, and conformance tests.

## Documents

- [daemon-routes.md](./daemon-routes.md)
  Daemon provider-compatible routes, required fields, optional fields, defaults,
  rejected fields, ignored fields, and JSON examples.
- [direct-cloud-routes.md](./direct-cloud-routes.md)
  Direct cloud provider server routes and how they differ from daemon
  compatibility adapters.
- [local-model-boundary.md](./local-model-boundary.md)
  Native local model-bound server boundary, fallback role, and provider-shaped
  local ingress adapter rules.
- [field-behavior.md](./field-behavior.md)
  Shared field behavior for `model`, `model_ref`, `stream`, `tools`,
  `response_format`, `dimensions`, `size`, and model-specific parameters.

## Scope

Audited surfaces:

- daemon provider-compatible routes from `src/tentgent-daemon/src/transport/rest/router.rs`
- direct cloud provider server routes from `src/tentgent-daemon/src/cloud_server.rs`
- local provider-shaped ingress adapters from `src/tentgent-daemon/src/local_server.rs`
- provider request mapping in `src/tentgent-kernel/src/features/cloud/infra.rs`

Native-only local routes such as `/v1/rerank`, `/v1/vision/chat`, audio jobs,
video jobs, and local image job routes are fallback context only. They are not
provider-compatible routes.

## Cross-Cutting Findings

- Daemon chat compatibility routes can target either a local model or a cloud
  provider. The route first tries to resolve `model` as a local model ref or
  alias, then falls back to the route's provider when no local model resolves
  and no `adapter_ref` is present.
- Daemon chat compatibility routes are text-first. OpenAI image parts, Claude
  image blocks, and Gemini non-text parts are rejected before chat dispatch.
- Daemon `/v1/embeddings` is both a native local embedding endpoint and a
  provider-shaped cloud embedding endpoint. `model` without `provider` implies
  OpenAI; `provider` can select OpenAI or Gemini.
- Direct cloud provider servers are bound to one provider model at launch. Their
  provider-shaped routes generally ignore caller-supplied `model` fields and
  use the bound model instead.
- Direct cloud provider servers accept broader image content for chat than the
  daemon compatibility routes, but streaming currently uses generic Tentgent
  `delta` and `done` SSE events.
- Local model-bound servers launched with `tentgent server run <model-ref>` use
  native Tentgent request bodies at the Python runtime boundary. Implemented
  provider-shaped local ingress routes, such as OpenAI `/v1/chat/completions`
  and OpenAI `/v1/embeddings`, translate at the Rust proxy edge and can be
  counted as local provider compatibility.
- Local model-bound servers mount implemented provider-shaped ingress adapters
  for this slice. Provider paths that collide with native local paths need
  request-shape disambiguation before they can be mounted safely.
- Unknown-field behavior is inconsistent. Daemon embedding rejects unknown
  fields manually, while most provider-shaped request structs ignore unknown
  fields because they do not use `#[serde(deny_unknown_fields)]`.

## User Matrix Feedback

The current user-facing compatibility matrix handles the first accuracy slice:
daemon cloud embeddings are represented, unsupported-field wording is
conservative, `/v1/chat/stream` is not promoted as a stable provider route, and
daemon compatibility wording is no longer chat-only.

Remaining refinements for later docs or fixtures:

- Mark direct cloud provider streaming as partial because it uses generic
  Tentgent `delta`/`done` SSE events rather than provider-native chunk shapes.
- Clarify where embedding responses are provider-shaped. OpenAI-compatible
  embedding requests now return OpenAI-style lists through daemon OpenAI cloud
  routing, direct OpenAI cloud servers, and local OpenAI embedding ingress,
  while native local and Gemini cloud embedding responses remain Tentgent-shaped.
- Keep native local model-bound routes in the fallback column, but list
  implemented provider-shaped local ingress routes in the compatibility
  surface.

## Follow-Up Issue Mapping

- `[api] Define stable unsupported provider API error semantics`
  Decide when unknown provider fields should reject, when fields may be ignored,
  and which error codes and JSON shapes are stable. Use the unknown-field notes
  in [field-behavior.md](./field-behavior.md) as the starting inventory.
- `[test] Add OpenAI chat completions compatibility fixtures`
  Cover daemon OpenAI-shaped response, local model-bound OpenAI ingress,
  streaming chunk behavior, and direct cloud streaming differences. Do not
  count native local `/v1/chat` itself as OpenAI compatibility.
- `[test] Add OpenAI embeddings compatibility fixtures`
  Cover daemon cloud embeddings, local OpenAI embedding ingress, direct cloud
  embedding validation gaps, unknown-field rejection, invalid input,
  capability-gate errors, and OpenAI-shaped embedding responses.
- `[test] Add OpenAI image generation compatibility fixtures`
  Cover daemon, direct cloud, and local model-bound OpenAI image request
  mapping, rejected image fields, local capability gates, and `gpt-image-*`
  `response_format` behavior.
- `[test] Add Claude messages compatibility fixtures`
  Cover daemon Claude-shaped non-streaming and streaming behavior plus rejected
  tool/image blocks, then decide direct cloud ignored-field behavior.
- `[test] Add Gemini compatible endpoint fixtures`
  Cover daemon Gemini non-streaming, streaming, text-only constraints, and
  rejected tools/non-text parts, then decide direct cloud streaming and tool
  behavior.
- `[docs] Add provider-compatible curl and SDK examples`
  Use the final matrix and this audit to avoid implying full provider parity.

## Recommended Next Step

Use this audit as the handoff for the next implementation slice:
unsupported-field semantics first, then focused conformance fixtures for each
provider-shaped route family. Keep native local model-bound server examples in
native Tentgent docs unless the route is explicitly wrapped by a provider
compatibility adapter.
