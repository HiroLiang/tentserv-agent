# TUI Session MVP

This plan defines the next terminal UI track after the HTTP daemon parity work.
The TUI should be a local operator console over the existing daemon, stores, and
session APIs, not a second implementation of runtime state.

## Current Baseline

The daemon and CLI now own the main workflow surfaces:

- managed models, adapters, datasets, servers, sessions, train plans, and train runs
- dataset validate/template/export/diff/synth/eval
- session mutation, session-aware chat, and bounded session compaction
- auth status, doctor diagnostics, daemon shutdown, and local HTTP contracts

The TUI should reuse those boundaries. It should not create another session
format, training registry, server registry, or provider runtime path.

## Scope

- Add a terminal UI entry point for local interactive use.
- Give users a browsable control plane for status, auth, doctor, stores, servers,
  sessions, dataset tools, and training plans/runs.
- Provide a session-aware chat surface that uses existing session compaction
  semantics.
- Prefer daemon HTTP APIs for live workflows and shared state.
- Use core managers only for bootstrap reads or explicit offline screens where
  HTTP is not yet available.

## Non-Goals

- Do not build a GUI.
- Do not replace the existing CLI command surface.
- Do not add multi-user collaboration.
- Do not add long-term memory, semantic retrieval, or a dataset-grade transcript
  archive.
- Do not add a separate TUI state model for stores, sessions, or runs.
- Do not add provider key mutation through daemon HTTP, config files, logs, or a
  second secret store. A guarded local TUI setup flow may reuse the existing
  `AuthManager` and system Keychain path.

## Command Surface

Planned command:

```text
tentgent tui [--home <PATH>] [--daemon-url <URL>] [--token <TOKEN>]
```

Recommended supporting daemon UX before or alongside the TUI:

```text
tentgent daemon start [--home <PATH>] [--host 127.0.0.1] [--port 8790]
tentgent daemon run --detach
```

Slice 0 fixes both commands as the supported detached daemon UX. `daemon start`
is the primary user-facing command, and `daemon run --detach` must remain a thin
entry over the same detached-launch implementation. The TUI should be able to
detect a missing daemon and show the command needed to start one.

Daemon URL discovery order for the TUI is:

1. `--daemon-url <URL>`
2. `TENTGENT_DAEMON_URL`
3. `<TENTGENT_HOME>/config.toml` `[daemon].url`
4. daemon metadata `host` and `port`
5. `http://127.0.0.1:8790`

Token discovery order is `--token <TOKEN>`, then `TENTGENT_DAEMON_TOKEN`, then
no token. No daemon token file is part of this MVP.

`config.toml` stores only non-secret preferences. It must tolerate unknown
fields, save atomically through a temp file and rename, validate `daemon.url` as
an absolute `http` or `https` URL, and never persist provider secrets or
`TENTGENT_DAEMON_TOKEN`.

## Product Shape

The first screen should be useful immediately after install:

```text
┌ Tentgent ───────────────────────────────────────────────────────────────┐
│ Home: ~/Library/Application Support/com.tentserv.tentgent   Daemon: OK  │
├───────────────┬─────────────────────────────────────────────────────────┤
│ Status        │ Doctor: ready with warnings                             │
│ Models        │ Auth: OpenAI env, HF missing, Anthropic missing          │
│ Adapters      │ Models: 2  Adapters: 1  Datasets: 3  Sessions: 4         │
│ Datasets      │ Running servers: 1  Running train runs: 0                │
│ Servers       │                                                         │
│ Sessions      │ Actions: Enter inspect  n new  / search  ? help          │
│ Training      │                                                         │
│ Settings      │                                                         │
└───────────────┴─────────────────────────────────────────────────────────┘
```

The UI should feel like a compact local operations panel, not a marketing page:
dense, keyboard-first, and explicit about which local paths, daemon URL, and
auth state are active.

## Navigation Model

Use a two-pane layout by default after the daemon is reachable:

- Left: stable navigation sections.
- Right: list, detail, chat, logs, or action form for the selected section.
- Footer: contextual key hints and destructive-action confirmation state.

Primary sections:

- Status: daemon status, doctor summary, auth status, runtime paths.
- Models: list, inspect, import/pull shortcuts later.
- Adapters: list, inspect, bind/import shortcuts later.
- Datasets: list, inspect, validate/template/export/diff/synth/eval actions.
- Servers: list, inspect, start/stop/logs/chat entry.
- Sessions: list, inspect, messages, compact, chat resume, delete.
- Training: LoRA plan list/inspect/create and run list/inspect/logs/metrics.
- Settings: home path, daemon URL, token source, Python/runtime paths.

The TUI has two top-level interaction modes:

- Bootstrap mode: daemon is down or auth is insufficient for live daemon
  workflows. Show a focused setup screen with `Start daemon`, daemon URL/token
  config, explicit per-provider auth setup, settings, and quit. Do not show fake
  live workflow panels.
- Operator mode: daemon is reachable. Show a menu plus monitoring dashboard
  summary. Live workflow data should come through daemon HTTP unless a setting
  is explicitly local-only.

Bootstrap mode rules:

- `Start daemon` is an explicit action. The TUI must not silently auto-start on
  launch.
- Starting should update app state to `starting`, keep the current terminal UI
  stable, then replace the whole screen with the new daemon state after the
  detached launch and readiness check finish. Do not clear the terminal, scroll
  content, or expose launch logs as in-band UI output.
- When the daemon is down, only local setup and auth setup actions are enabled.
  Store, server, session, dataset, training, and dashboard workflow sections
  remain disabled until daemon HTTP is reachable.
- Provider auth status should not imply secret reads. The default auth summary
  may show env presence and `keychain: not checked`; it should read Keychain
  only after the user enters an explicit provider setup/check/set/remove flow.
- Auto refresh must not read Keychain and must not call daemon routes that read
  Keychain. Manual auth checks may prompt.

Operator mode rules:

- The landing view is a function menu plus compact monitoring dashboard table:
  servers, sessions, stores, doctor, daemon status, and auth summary.
- Dashboard boxes are acceptable, but they are supporting UI, not blocking
  modal-like interactions.
- All live workflow reads should go through daemon HTTP in this mode.
- Settings remain available while the daemon is running. TUI/client preferences
  such as daemon URL and UI section can update immediately. Daemon bind settings
  such as host/port and process token state apply to the next daemon start and
  should be marked as requiring restart.
- Provider Keychain set/remove should take effect for future daemon workflows
  that resolve auth after the change. Already launched runtime child processes
  do not retroactively receive updated environment secrets.
- Chat uses a separate workspace only after the user selects a running server
  or starts a chat flow. The operator dashboard is not itself the chat UI.

Interaction controls should preserve the visual style in the design draft:

- Use dynamic tables for inventories, status summaries, and dashboard metrics.
- Use radio/choice controls such as `●` selected and `○` unselected for option
  sets.
- Use explicit selection state and keyboard hints for `↑`/`↓`, Enter, Escape,
  refresh, and destructive confirmations.
- Resize events should recompute layout. Wide terminals may show menu plus
  dashboard table; compact terminals should collapse detail panels and hide
  nonessential columns before wrapping text.

## Design Artifacts

The current visual draft lives in
[docs/plans/tui/design/](./tui/design/README.md). Treat it as a layout and
interaction reference, not as a protocol contract.

If the visual draft diverges from HTTP/session/runtime contracts, the contracts
win. Update the design notes rather than copying stale mock data into code.

## Execution Order

### Slice 0: Daemon UX Prerequisite

Make the daemon easy to run before the TUI depends on it.

Goals:

- add a documented detached daemon start path
- define how the TUI discovers daemon URL and token
- show a friendly startup hint when no daemon is running
- keep shutdown protected by existing daemon-control auth rules
- keep `daemon start` and `daemon run --detach` on the same detached-launch
  implementation
- treat public `GET /healthz` readiness as authoritative; report `/v1/status`
  `401` as an auth warning after health succeeds
- keep idempotent start scoped to the resolved runtime home
- let detached daemon children inherit daemon configuration environment while
  preserving daemon-token sanitization for model-bound server children

Review target:

- a user can start the daemon, run `tentgent tui`, and see status without
  opening a second foreground terminal.

### Slice 1: TUI Architecture Skeleton + Local Setup Foundation

Choose the Rust TUI stack and command boundary.

Goals:

- use `ratatui` plus `crossterm` unless a concrete blocker appears
- add `tentgent tui`
- implement terminal lifecycle, app state, navigation, key handling, status, and
  settings screens
- keep rendering code separate from daemon API/client code
- use daemon HTTP first for live status, auth, and doctor data
- use core code only for bootstrap config, daemon discovery, explicit daemon
  start, and guarded local Keychain auth setup
- allow daemon start only after an explicit `Start daemon` selection; do not
  silently auto-start the daemon on launch
- derive explicit daemon start host/port from the resolved daemon URL, so
  `--daemon-url`, `TENTGENT_DAEMON_URL`, config, metadata, and default discovery
  all feed the same start target
- treat `/healthz` success plus `/v1/status` `401` as `AuthRequired`, not down
- persist only non-secret daemon URL/UI preferences to `config.toml`
- allow provider key set/remove only through local `AuthManager` and Keychain;
  never through daemon HTTP, config, logs, or UI output

Review target:

- one minimal screen shows daemon status, runtime home, auth summary, and doctor
  summary without launching model runtime work, and settings can edit local
  non-secret setup safely.

### Slice 1.1: TUI Interaction Reset

Fix the Slice 1 skeleton into the intended operator-console interaction model
before adding read-only navigator workflows.

Goals:

- split app state into Bootstrap and Operator modes
- make daemon-down landing a focused setup/action screen, not a live dashboard
- make daemon-running landing a function menu plus monitoring dashboard summary
- keep the design draft's dense terminal UI style, including dynamic tables,
  selectable rows, radio-style choices, and keyboard hints
- remove auto Keychain reads from startup and periodic refresh
- avoid calling daemon `/v1/auth` automatically from the landing refresh path if
  it would trigger Keychain prompts
- keep provider auth setup explicit and provider-scoped
- make `Start daemon` render a stable `starting` state and then replace the
  full screen with the resulting state
- ensure start/refresh interactions never clear the terminal, scroll the UI, or
  leave partial frames visible
- make settings editable while the daemon is running, distinguishing immediate
  TUI/client settings from daemon process settings that require restart
- handle terminal resize by recalculating layout instead of relying on fixed
  panel dimensions

Review target:

- a user can launch `tentgent tui`, start a missing daemon, inspect the operator
  menu/dashboard, and enter explicit auth setup without repeated Keychain
  prompts or terminal redraw artifacts.

### Slice 2: Read-Only Navigator

Build read-only browsing first.

Goals:

- list and inspect models, adapters, datasets, servers, sessions, train plans,
  and train runs
- keep Training as one menu entry with read-only Plans and Runs sub-tabs
- show bounded server logs, train run metrics/logs, and session message tails
- use local filters only; do not add pagination/query routes in this slice
- preserve previous dashboard counts when one read endpoint fails
- treat inspect/tail `404` as a stale selected item, not an auth or daemon-down
  transition
- preserve existing CLI/HTTP behavior for errors and auth

Review target:

- users can orient themselves without memorizing CLI commands.

### Slice 2.5: Local Resource Monitor

Add a read-only local resource dashboard before mutation-heavy workflows.

Goals:

- show runtime home disk usage by category: models, adapters, datasets,
  sessions, logs, train plans, and train runs
- show daemon and managed server process resource summaries where available:
  pid, running state, RSS/memory, CPU sample, port, and log size
- show local disk free space for the resolved runtime home
- show large files, stale logs, stale pid/process metadata, and long-running or
  stale train runs as warnings
- keep this slice read-only; cleanup, delete, stop, compact, and archive actions
  remain deferred to later mutation slices
- avoid adding a second runtime state model; derive resource rows from existing
  runtime paths, daemon status, server specs/process metadata, train run
  metadata, and OS process probes
- degrade gracefully when platform resource probes are unavailable

Review target:

- users can understand local disk and process pressure from the TUI without
  leaving the operator console or running separate `du`, `ps`, `btop`, or
  `htop` commands.

### Slice 3: Session Chat Workspace

Add a session-aware chat view.

Goals:

- create or resume a session
- choose a running server and optional adapter
- send native daemon chat with `session_ref`
- display bounded session history and compaction status
- make failures explicit: server stopped, session busy, compaction failed, or
  context unsupported

Review target:

- users can leave and resume a compact terminal chat session without learning
  the raw session endpoints.

### Slice 4: Store And Dataset Actions

Expose safe store and dataset workflows.

Goals:

- import/pull models and adapters through existing HTTP endpoints
- import/validate/template/export/diff datasets
- run synth/eval only after explicit provider/auth confirmation
- show generated artifact paths without dumping raw provider output

Review target:

- common dataset and store workflows are discoverable from terminal menus while
  preserving the existing HTTP contracts.

### Slice 5: Server And Training Actions

Expose runtime and training controls.

Goals:

- start, inspect, stop, and remove server specs
- preview/create/remove LoRA plans
- start LoRA runs and poll run status, metrics, and logs
- make destructive actions confirmable

Review target:

- the TUI can operate the local runtime loop from model selection through a
  small training run without adding new backend semantics.

## Open Questions

- Should `tentgent tui` auto-start a daemon when token and bind settings are
  safe, or should it only show the exact command to run?
- Should the first chat view support streaming immediately, or ship non-stream
  first and add streaming after layout settles?

Resolved in Slice 1:

- `tentgent tui` does not silently auto-start the daemon. It may start the
  daemon after an explicit `s` action through the shared detached-launch helper,
  using the resolved daemon URL host/port as the start target.
- Provider key setup may be exposed in TUI only as a guarded local
  `AuthManager`/Keychain flow. No daemon HTTP secret mutation route is added.
