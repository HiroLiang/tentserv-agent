# TUI Session MVP

This plan defines the future terminal UI track: make common Tentgent workflows selectable and give chat a coarse session/context manager.

## Priority

Run this after the cloud dataset track has its first usable slices. Prefer implementing the daemon/API boundary first if the TUI would otherwise duplicate server and session state logic.

## Scope

- Add a terminal UI entry point for local interactive use.
- Let users select models, adapters, datasets, and servers from terminal menus.
- Add coarse chat session context management.
- Reuse existing stores and runtime commands instead of creating a parallel state model.

## Non-Goals

- Do not build a GUI.
- Do not replace the existing CLI command surface.
- Do not add multi-user collaboration.
- Do not implement advanced memory, summarization, or semantic search in the first pass.
- Do not manage cloud-backed model sessions in this track.

## Command Surface

Planned command:

```text
tentgent tui [--home <PATH>]
```

Possible later CLI helpers:

```text
tentgent session ls
tentgent session inspect <SESSION_REF>
tentgent session export <SESSION_REF> <PATH>
tentgent session rm <SESSION_REF>
```

## Session Shape

Store session metadata under Tentgent-managed runtime state, for example:

```text
TENTGENT_HOME/
└── sessions/
    └── <session_ref>/
        ├── session.toml
        └── transcript.jsonl
```

First-pass fields:

- `session_ref`
- `short_ref`
- `model_ref`
- optional `adapter_ref`
- `created_at`
- `updated_at`
- transcript messages in canonical role/content form

## Execution Order

### Slice 1: TUI Architecture Decision

Choose the Rust TUI stack and command boundary.

Goals:

- decide whether to use `ratatui` plus `crossterm`
- define the TUI state adapter over existing Tentgent managers
- keep rendering code separate from store/runtime logic

Review target:

- one minimal screen can show status without launching model runtime work

### Slice 2: Read-Only Navigator

Build read-only browsing first.

Goals:

- show status and backend capability state
- list models, adapters, datasets, and servers
- inspect one selected item
- avoid mutations while the navigation model settles

Review target:

- users can orient themselves without memorizing CLI commands

### Slice 3: Chat Session Draft

Add a chat workflow with saved transcript state.

Goals:

- create a new session from a selected model
- optionally attach a compatible adapter
- append user and assistant messages to `transcript.jsonl`
- reuse current one-shot or server chat paths for generation

Review target:

- users can leave and resume a coarse terminal chat session

### Slice 4: Session Management

Add basic lifecycle actions.

Goals:

- list sessions
- resume a session
- export a transcript
- delete a stopped or inactive session

Review target:

- the session store is useful from both TUI and possible CLI helpers

### Slice 5: Dataset Workflow Hooks

Expose the cloud dataset workflow after it exists.

Goals:

- run `dataset validate` from the TUI
- open `dataset template` output for copy/paste
- start `dataset synth` only after explicit provider/key confirmation

Review target:

- dataset generation remains contract-safe inside the TUI

## Open Questions

- Should session commands be public CLI commands before the TUI ships?
- Should TUI chat talk directly to Python runtime, to `tentgent server`, or to the future daemon API?
- Should session transcripts use `tentgent.chat.v1` records or a thinner session-specific schema?
