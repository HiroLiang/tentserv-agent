# TUI V2 Optimization

This plan is for the next TUI interaction pass after the `v0.3.0-alpha.1`
preview.

The current TUI is archived as an alpha baseline. It proves the daemon routes,
DTOs, reducers, and many actions can work, but it is not a UX contract. Future
work may reuse working code and tests while replacing the shell, navigation, and
forms.

This is not the post-alpha bug bucket. Correctness bugs found during smoke tests
should be recorded first in [0.3-bugfix-rollup.md](./0.3-bugfix-rollup.md), then
fixed in batches. This plan starts after the bugfix rollup has clarified the
core session and daemon/server behavior.

## Baseline

- tag: `v0.3.0-alpha.1`
- release type: prerelease
- scope: daemon-first TUI preview with chat, sessions, jobs, resources, store
  actions, server/training actions, picker-based create flows, and session
  delete

Use the baseline for regression comparison only.

## Goals

- Make the TUI feel like a compact local operator console, not a collection of
  CLI-shaped forms.
- Keep daemon HTTP as the source of truth for live workflow state.
- Preserve the useful alpha implementation pieces:
  - daemon discovery and explicit start
  - chat/session routes
  - jobs/progress DTOs
  - read-only navigators
  - guarded action requests
  - picker/review tests where they help
- Replace confusing navigation, key hints, and dense forms.

## Proposed Shell

Primary modes should be fewer and clearer:

- Bootstrap: daemon down, auth required, or config error
- Dashboard: daemon health, jobs, resources, active servers/runs
- Chat: running server + session workspace
- Stores: models, adapters, datasets
- Runtime: servers and jobs
- Training: plans and runs
- Sessions: session list, messages, cleanup, compact later
- Settings: home, daemon URL, auth setup, local preferences

The left navigation may group sections instead of listing every resource type as
a permanent top-level row.

## Interaction Principles

- Footer hints come from the focused mode/screen, never from one global static
  string.
- Destructive actions require exact ref confirmation.
- Managed local resources use pickers first.
- Raw refs and cloud/manual values are advanced paths.
- Review pages stay open until explicit submit, cancel, or dismiss.
- Compact tables show short refs; full refs live in detail/review panes.
- Chat keys must not trigger global action keys.
- Background jobs remain visible without trapping the user in an action screen.

## Work Items

### V2.1 Shell And Navigation

- Replace the current primary menu with grouped operator modes.
- Define focus states and footer hint ownership per screen.
- Keep Bootstrap vs Operator mode explicit.
- Preserve existing daemon/client generation and stale-result protection.

### V2.2 Chat Surface

- Make server/session/context scope visible without overwhelming the transcript.
- Keep session context behavior aligned with the bugfix rollup.
- Make "new topic" and "new session" distinct.
- Make adapter selection explicit and avoid key collisions.

### V2.3 Store And Runtime Actions

- Rework action entry into a mode-aware command palette or action drawer.
- Keep model/dataset/server pickers primary.
- Keep manual refs as advanced.
- Make progress cards visually useful and backgroundable.

### V2.4 Training Flow

- Rebuild LoRA plan creation as a real wizard:
  - choose model
  - choose dataset
  - choose backend/profile
  - configure advanced settings only if requested
  - preview
  - review
  - create
- Allow every review row to jump back to its picker/form step.
- Keep training run start confirmation explicit about CPU/GPU/disk/time impact.

### V2.5 Visual And Compact Layout

- Revisit table density, borders, color, selected-row styling, and empty states.
- Avoid long full refs in dense layouts.
- Verify compact terminal rendering for every major screen.

## Dependencies

Do not begin broad TUI V2 implementation until the bugfix rollup fixes the
session context semantics and daemon/direct-server chat boundary:

- [0.3-bugfix-rollup.md](./0.3-bugfix-rollup.md)

## Non-Goals

- Do not add new daemon runtime state models.
- Do not add new backend routes just to satisfy layout desires.
- Do not add long-term memory or semantic retrieval.
- Do not replace CLI workflows.
- Do not make TUI auto-start servers, pull models, or run training without an
  explicit action and confirmation.
