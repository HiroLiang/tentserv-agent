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

- [tui-session-mvp.md](./tui-session-mvp.md)
  Future terminal UI plan for selectable workflows and coarse chat session context management.

## Recommended Order

1. Build TUI session management after daemon/server behavior settles.

## Deferred Plans

- [packaging-install-mvp.md](./packaging-install-mvp.md)
  Mostly implemented release/install track; Homebrew tap and signing/notarization remain deferred.

## Archived Plans

- [archive/README.md](./archive/README.md)
  Router for completed plans that are kept only for historical context.
