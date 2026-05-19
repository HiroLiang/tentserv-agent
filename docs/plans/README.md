# Plans

Use this directory for the current active implementation roadmap and any
still-open plans that are too large or too cross-cutting to execute safely in
one pass without a staged breakdown.

## Scope

- Record step-by-step execution plans before large runtime, server, backend, or
  release changes.
- Keep one active roadmap unless a future initiative needs its own focused plan.
- Prefer short, action-oriented documents over long design essays.

## Routing Rule

- Keep each plan focused on one execution track.
- If a plan grows large, split it into subfolders with a local `README.md`.
- Update the plan when the approved execution order changes materially.
- Prefer review-sized implementation slices over one large execution step.
- When a track is being developed interactively, document the next small slice explicitly.
- Move completed or superseded plans into `archive/` so the top-level plan
  directory stays focused on current work.

## Active Plan Index

- [capability-first-release-roadmap.md](./capability-first-release-roadmap.md)
  Active roadmap after `v0.3.5-alpha.0`: model capability classification,
  embedding and rerank endpoint work, deferred multimodal and streaming-boundary
  planning, and Apple Developer ID signing before beta or release candidate
  tags.
- [m2-model-capability-detection-and-correction.md](./m2-model-capability-detection-and-correction.md)
  Detailed M2 slice: Hugging Face capability detection, manual metadata
  correction, and clear default-chat fallback warnings.
- [m3-server-compatibility-gates.md](./m3-server-compatibility-gates.md)
  Implemented M3 slice: server capability metadata, daemon server DTOs, and
  endpoint-family compatibility gates before embedding and rerank runtime work.
- [m4-embedding-mvp.md](./m4-embedding-mvp.md)
  Implemented M4 slice: native embedding endpoint, embedding runtime port,
  first backend path, and endpoint-family isolation from chat sessions.
- [m5-rerank-mvp.md](./m5-rerank-mvp.md)
  Implemented M5 slice: native rerank endpoint, rerank runtime port, first local
  cross-encoder backend path, CLI one-shot embedding/rerank helpers, and
  endpoint-family isolation from chat sessions.

## Recommended Order

1. Define the M6 native multimodal contracts and opaque streaming proxy boundary
   before implementing media runtime support.
2. Run Apple Developer ID signing and notarization on prerelease artifacts
   before beta or release candidate tags.

## Deferred Plans

- No terminal UI redesign track is active. The product surface is CLI plus
  daemon REST.

## Archived Plans

- [archive/README.md](./archive/README.md)
  Router for completed or superseded plans that are kept only for historical
  context.
