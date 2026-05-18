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
  embedding and rerank endpoint work, deferred audio contracts, and Apple
  Developer ID signing before beta or release candidate tags.

## Recommended Order

1. Add explicit model capability metadata and classification controls.
2. Use Hugging Face metadata as best-effort evidence, with user override as the
   authority.
3. Gate servers and endpoints by model capability before adding non-chat
   runtime paths.
4. Implement embedding MVP, then rerank MVP.
5. Define audio contracts before implementing audio runtime support.
6. Run Apple Developer ID signing and notarization on prerelease artifacts
   before beta or release candidate tags.

## Deferred Plans

- No terminal UI redesign track is active. The product surface is CLI plus
  daemon REST.

## Archived Plans

- [archive/README.md](./archive/README.md)
  Router for completed or superseded plans that are kept only for historical
  context.
