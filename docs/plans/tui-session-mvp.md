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
- Do not add provider key mutation in the first pass.

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

The exact daemon detach command can be decided in the daemon UX slice. The TUI
should be able to detect a missing daemon and show the command needed to start
one.

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

Use a two-pane layout by default:

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

Review target:

- a user can start the daemon, run `tentgent tui`, and see status without
  opening a second foreground terminal.

### Slice 1: TUI Architecture Skeleton

Choose the Rust TUI stack and command boundary.

Goals:

- use `ratatui` plus `crossterm` unless a concrete blocker appears
- add `tentgent tui`
- implement app state, navigation, key handling, and a status screen
- keep rendering code separate from daemon API/client code

Review target:

- one minimal screen shows daemon status, runtime home, auth summary, and doctor
  summary without launching model runtime work.

### Slice 2: Read-Only Navigator

Build read-only browsing first.

Goals:

- list and inspect models, adapters, datasets, servers, sessions, train plans,
  and train runs
- show server logs, train run metrics, and session message tails
- preserve existing CLI/HTTP behavior for errors and auth

Review target:

- users can orient themselves without memorizing CLI commands.

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
- Should TUI tokens be read only from `TENTGENT_DAEMON_TOKEN`, or should a local
  daemon-token file be introduced later?
- Should provider key mutation remain CLI-only permanently, or get a separate
  guarded TUI flow after security review?
- Should the first chat view support streaming immediately, or ship non-stream
  first and add streaming after layout settles?
