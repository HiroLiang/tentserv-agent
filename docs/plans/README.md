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

- [tui-v2-optimization.md](./tui-v2-optimization.md)
  Deferred TUI interaction redesign plan. The `v0.3.0-alpha.1` TUI is treated
  as an archived baseline, not a UX contract.
- [tui-session-mvp.md](./tui-session-mvp.md)
  Historical daemon-first terminal UI MVP plan for local status, store, server,
  session, dataset, and training workflows. It remains as the detailed alpha
  implementation record and routing document. Visual draft:
  [tui/design/README.md](./tui/design/README.md).

## Recommended Order

1. Use the archived bugfix rollup when reviewing the `v0.3.0-alpha.2`
   correctness fixes.
2. Redesign the TUI shell using `v0.3.0-alpha.1` as an archived baseline.

## Deferred Plans

- [packaging-install-mvp.md](./packaging-install-mvp.md)
  Mostly implemented release/install track; Homebrew tap and the macOS
  Developer ID signing/notarization slice remain deferred until the TUI and
  installer surfaces are ready to ship.

## Archived Plans

- [archive/README.md](./archive/README.md)
  Router for completed plans that are kept only for historical context.
