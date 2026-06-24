# Plans

Use this directory for current roadmap and maintenance plans that are too large
or too cross-cutting to track only in GitHub issues.

## Scope

- Record step-by-step execution plans before large runtime, server, backend, or
  release changes.
- Keep the active plan surface small.
- Prefer short, action-oriented documents over long design essays.
- Archive completed or superseded tracks so the active plan surface stays
  focused.

## Routing Rule

- Start with the active plans below.
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

- [v1.x-roadmap.md](./v1.x-roadmap.md)
  Long-term post-`v1.0.0` product roadmap for cluster/serving targets,
  compatibility proof foundations, tuple-aware model and LoRA gates, resource
  coordination, and later 1.x capabilities.
  - [cluster-serving-target-roadmap.md](./cluster-serving-target-roadmap.md)
    Focused sub-roadmap for deciding the cluster/serving target candidate
    groups before individual `v1.1.0` issues are selected.
- [bugfix-maintenance-plan.md](./bugfix-maintenance-plan.md)
  Active maintenance queue for post-`v1.0.0` bug fixes, diagnostics polish,
  release follow-up, documentation cleanup, and repository hygiene.

## Deferred Plans

- No terminal UI redesign track is active. The product surface is CLI plus
  daemon REST.

## Archived Plans

- [archive/README.md](./archive/README.md)
  Router for completed or superseded plans, including the completed
  capability-first M2-M7 release roadmap and archived `v0.9.0` / `v1.0.0`
  release records.
- [archive/v1.0.0-stable-compatibility-plan.md](./archive/v1.0.0-stable-compatibility-plan.md)
  Archived `v1.0.0` stable compatibility release train and post-merge
  release/tap checklist.
- [archive/post-m7-platform-compatibility-roadmap.md](./archive/post-m7-platform-compatibility-roadmap.md)
  Archived post-M7 platform compatibility roadmap. Its active follow-up items
  are now split between the `v1.x` roadmap and the bugfix maintenance plan.
- [archive/post-1.0-serving-targets-and-multimodal-context-pipeline.md](./archive/post-1.0-serving-targets-and-multimodal-context-pipeline.md)
  Archived post-1.0 serving-target and multimodal-context planning note. Its
  current direction is summarized in the `v1.x` roadmap.
- [archive/provider-api-compatibility-and-model-support-roadmap.md](./archive/provider-api-compatibility-and-model-support-roadmap.md)
  Archived provider compatibility, model support, runtime profile, and 1.0
  readiness roadmap.
- [archive/v0.9.0-hardening-plan.md](./archive/v0.9.0-hardening-plan.md)
  Completed `v0.9.0` hardening execution plan and release PR/tag checklist.
- [archive/v0.9.0-api-surface-audit-findings.md](./archive/v0.9.0-api-surface-audit-findings.md)
  Archived `v0.9.0` API surface audit findings and follow-up routing record.
