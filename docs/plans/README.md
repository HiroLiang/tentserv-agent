# Plans

Use this directory for the current active implementation roadmap and any
still-open plans that are too large or too cross-cutting to execute safely in
one pass without a staged breakdown.

## Scope

- Record step-by-step execution plans before large runtime, server, backend, or
  release changes.
- Keep one active roadmap unless a future initiative needs its own focused plan.
- Prefer short, action-oriented documents over long design essays.
- Archive completed or superseded tracks so the active plan surface stays
  focused.

## Routing Rule

- Start with the active roadmap below.
- Keep each plan focused on one execution track.
- If a plan grows large, split it into subfolders with a local `README.md`.
- Update the plan when the approved execution order changes materially.
- Prefer vertical feature slices over horizontal layer slices. A GitHub issue
  and branch should usually carry one user-visible or operator-visible
  capability through the needed contract, domain, persistence, entry point,
  documentation, and tests when that scope is manageable.
- Avoid opening separate issues only for implementation layers such as domain,
  store, CLI, daemon, docs, or tests unless the layer is a standalone design
  risk or the vertical feature branch would become too large to review safely.
- When a track is being developed interactively, document the next vertical
  capability slice explicitly.
- Move completed or superseded plans into `archive/` so this directory stays
  focused on current work.

## Active Plan Index

- [v1.0.0-stable-compatibility-plan.md](./v1.0.0-stable-compatibility-plan.md)
  Active `v1.0.0` release train for freezing stable API surfaces, proving
  provider-compatible and native workflow compatibility, validating install and
  diagnostics readiness, publishing the release, updating Homebrew, and closing
  the roadmap.
- [post-m7-platform-compatibility-roadmap.md](./post-m7-platform-compatibility-roadmap.md)
  Active post-M7 roadmap for platform trust, model and LoRA compatibility
  management, runtime proof storage, dynamic runtime routing, media serving
  wrappers, runtime stream proxy decisions, resource coordination, and
  conversion boundaries after the signed `v0.4.1` release.
- [provider-api-compatibility-and-model-support-roadmap.md](./provider-api-compatibility-and-model-support-roadmap.md)
  Focused roadmap for OpenAI, Claude/Anthropic, and Gemini-compatible API
  surfaces, model support records, runtime parameter profiles, verification
  gates, bounded dynamic routing, and the staged path to `1.0.0`.

## Deferred Plans

- [post-1.0-serving-targets-and-multimodal-context-pipeline.md](./post-1.0-serving-targets-and-multimodal-context-pipeline.md)
  Deferred post-1.0 direction for grouping multiple capability-specific models
  behind one serving target and pre-processing local media attachments into chat
  context.
- No terminal UI redesign track is active. The product surface is CLI plus
  daemon REST.

## Archived Plans

- [archive/README.md](./archive/README.md)
  Router for completed or superseded plans, including the completed
  capability-first M2-M7 release roadmap and archived `v0.9.0` hardening
  records.
- [archive/v0.9.0-hardening-plan.md](./archive/v0.9.0-hardening-plan.md)
  Completed `v0.9.0` hardening execution plan and release PR/tag checklist.
- [archive/v0.9.0-api-surface-audit-findings.md](./archive/v0.9.0-api-surface-audit-findings.md)
  Archived `v0.9.0` API surface audit findings and follow-up routing record.
