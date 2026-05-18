# Plans

Use this directory for active or still-open implementation plans that are too large or too cross-cutting to execute safely in one pass without a staged breakdown.

## Scope

- Record step-by-step execution plans before large runtime, server, or backend changes.
- Keep one plan per major initiative.
- Prefer short, action-oriented documents over long design essays.

## Routing Rule

- Keep each plan focused on one execution track.
- If a plan grows large, split it into subfolders with a local `README.md`.
- Update the plan when the approved execution order changes materially.
- Prefer review-sized implementation slices over one large execution step.
- When a track is being developed interactively, document the next small slice explicitly.
- Move completed plans into `archive/` so the top-level plan directory stays focused on unfinished work.

## Active Plan Index

- [packaging-install-mvp.md](./packaging-install-mvp.md)
  Active release/install track. The current execution track is the 0.3.x
  project-owned Homebrew tap distribution path: stable release readiness, tag
  assets, tap formula, install/upgrade/uninstall smoke, docs, and tap update
  automation.
- [linux-release-support.md](./linux-release-support.md)
  Linux release/install track. The x86_64 prerelease path now has GitHub
  Release tarballs, `install.sh` support, base runtime bootstrap smoke, and
  user-facing preview docs. Optional expansion remains open for Linux arm64,
  distro packages, and heavier runtime profiles.
- [tentgent-kernel-migration.md](./tentgent-kernel-migration.md)
  Planned `tentgent-kernel` migration for unified runtime layout, application
  use cases, filesystem store boundaries, and the machine-local capability
  state. This should become the shared readiness source for
  Linux/Windows/macOS backend gates before optional local-model, training, GPU,
  or non-chat model capabilities are advertised.
- [tentgent-daemon-runtime.md](./tentgent-daemon-runtime.md)
  Planned `tentgent-daemon` runtime systems for one-shot background jobs,
  bounded progress/output visibility, session-aware daemon orchestration, and
  chat-backed session compaction.
- [model-capabilities-embedding-rerank.md](./model-capabilities-embedding-rerank.md)
  Planned model capability track for embedding and rerank models. Separates
  model storage format from serving capability before adding non-chat endpoints;
  local backend work should use kernel capability-state gates.
- [tui-v2-optimization.md](./tui-v2-optimization.md)
  Deferred TUI interaction redesign plan. The `v0.3.0-alpha.1` TUI is treated
  as an archived baseline, not a UX contract.
- [tui-session-mvp.md](./tui-session-mvp.md)
  Historical daemon-first terminal UI MVP plan for local status, store, server,
  session, dataset, and training workflows. It remains as the detailed alpha
  implementation record and routing document. Visual draft:
  [tui/design/README.md](./tui/design/README.md).

## Recommended Order

1. Start the `tentgent-kernel` migration with the crate shell, runtime layout,
   and app-context bundles.
2. Wire capability state into backend-gated workflow bundles before
   profile-specific backend readiness claims.
3. Use capability state to decide whether Linux optional expansion should
   continue now or wait for preview feedback.
4. Plan and implement model capabilities for embedding and rerank models.
5. Redesign the TUI shell using `v0.3.0-alpha.1` as an archived baseline.

## Deferred Plans

- macOS Developer ID signing and notarization remain deferred inside
  [packaging-install-mvp.md](./packaging-install-mvp.md) until the unsigned
  project-owned tap flow is stable.

## Archived Plans

- [archive/README.md](./archive/README.md)
  Router for completed plans that are kept only for historical context.
