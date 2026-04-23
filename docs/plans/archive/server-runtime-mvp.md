# Server Runtime MVP

This plan defines the next execution track after one-shot chat: a long-lived `tentgent server` command that keeps one model available behind a stable HTTP interface.

## Status

- This plan is now completed and archived.
- The next active runtime track now lives in [../lora-server-mvp.md](../lora-server-mvp.md).
- Packaging and install remain a separate future track in [../packaging-install-mvp.md](../packaging-install-mvp.md).

## Decision Summary

- Make `tentgent server <MODEL_REF>` the next milestone before LoRA work.
- Reuse the Python runtime layer that already powers `tentgent-chat-once`.
- Start with one process serving one model reference.
- Support long-lived model residency, but keep lifecycle policy simple in the first pass.
- Defer dynamic multi-model scheduling and advanced adapter orchestration until the server contract is stable.
- Implement the server path in small interactive slices so each step is easy to review before the next one begins.

## Goals

- Add a user-facing `tentgent server <MODEL_REF>` command.
- Keep a loaded model available across requests.
- Expose one HTTP chat surface that can stream responses.
- Reuse the same backend routing rules already proven by one-shot chat.
- Create the right lifecycle boundary for later LoRA work.

## Non-Goals

- Multi-model scheduling in one process
- Full production deployment features
- Provider-compatible surface for every external API on day one
- Dynamic LoRA hot-swap in the first server cut
- TUI management screens

## Why Server Before LoRA

- LoRA needs a stable runtime lifecycle:
  - where adapters load
  - when they unload
  - how they interact with cached base models
- A one-shot process can prove backend correctness, but it cannot prove long-lived adapter behavior.
- The server phase should establish:
  - model session ownership
  - load and release policy
  - request streaming contract
  - error and shutdown behavior

## Review Strategy

Use small review-sized slices instead of one large server branch.

Rules:

- start with `server run`
- keep each implementation step focused on one boundary
- review and discuss after each slice before moving on
- avoid mixing process control, registry, HTTP shape, and LoRA policy in the same step

The first server cut should be built interactively, not as one end-to-end patch.

## Command Surface

Define three runtime concepts first:

- `server spec`
  - persisted server configuration under Tentgent-managed storage
- `server process`
  - the live process currently binding a host and port
- `model session`
  - the in-memory loaded model instance owned by one process

Start with this command family:

```text
tentgent server run <MODEL_REF>
tentgent server ls
tentgent server ps
tentgent server inspect <SERVER_REF>
tentgent server stop <SERVER_REF>
tentgent server start <SERVER_REF>
tentgent server rm <SERVER_REF>
```

Command intent:

- `run`
  - create a server spec and launch a process
- `ls`
  - list registered server specs, including stopped ones
- `ps`
  - list only live server processes
- `inspect`
  - show the merged view of spec, process state, and runtime policy
- `stop`
  - stop a live process without deleting its spec
- `start`
  - launch a stored server spec again
- `rm`
  - remove a stored server spec and require the process to be stopped first, unless a later `--force` policy is added

Planned first-pass `run` options:

- `--home <PATH>`
- `--host <HOST>`
- `--port <PORT>`
- `--lazy-load`
- `--idle-seconds <N>`

Keep the first command surface intentionally small. Add more flags only when the runtime policy is proven.

## HTTP Surface

Start with one chat endpoint and one health endpoint.

- `GET /healthz`
- `POST /v1/chat`

`POST /v1/chat` should accept:

- `messages`
- optional generation settings
- optional `stream`

Do not commit to a full OpenAI-compatible API in the first pass. Favor a Tentgent-native request shape first.

## Runtime Shape

Use one long-lived process with one active model session.

Core pieces:

- Rust command layer:
  - parse flags
  - prepare environment
  - launch the Python server entry
- Python server layer:
  - own the HTTP loop
  - own the in-memory model session
  - reuse backend routing and generation code
- Shared runtime session layer:
  - load model
  - generate or stream
  - release model

Persist server specs separately from live process state, for example under:

- `TENTGENT_HOME/servers/<server_ref>/server.toml`

The exact file names can still change, but the first implementation should distinguish persisted server identity from current runtime state.

## Lifecycle Policy

Define these policies in the first server cut:

- Startup mode:
  - eager load or lazy load
- Idle behavior:
  - keep loaded forever, or
  - auto release after `idle_seconds`
- Shutdown behavior:
  - stop accepting requests
  - allow in-flight request cleanup
  - release model resources cleanly

Do not add cross-process coordination yet.

## Concurrency Policy

The first server cut should be conservative:

- one process owns one model session
- one process binds one port
- one model session serves one active generation at a time
- additional requests should queue instead of trying to run concurrent generation immediately

Do not assume two ports can share one loaded model unless they are served by the same process. In practice:

- same process
  - can reuse one in-memory model session
- different processes
  - should be treated as separate model loads, even when they point at the same `MODEL_REF`

## Future Packaging Target

The current Python locator is a development-time strategy:

- Rust resolves the Python subproject from the repository layout
- Rust launches the daemon through the local `.venv`

This is acceptable for iterative development, but it is not the final release shape.

Future release candidates should choose between these packaging tracks:

- Track A:
  - ship a controlled Python runtime inside the product bundle
- Track B:
  - expose the Python daemon as a packaged executable boundary

No packaging decision is locked in yet. The current server work should keep the Rust-to-Python boundary clean enough that either track can be adopted later.

## Execution Order

The server initiative should begin with `run`, then expand outward.

### Phase 1: Shared runtime session

- Extract a reusable runtime session from the current one-shot chat path.
- Keep one interface for:
  - `load()`
  - `generate()`
  - `stream()`
  - `release()`

### Phase 2: Python server entry

- Add a Python server entry point under the daemon subproject.
- Start with one process and one loaded model reference.
- Reuse the existing backend router instead of duplicating backend logic.

### Phase 3: Rust wrapper

- Add the `tentgent server` command family in Rust.
- Have Rust launch the Python server entry with a clean command surface.
- Keep stderr and startup output user-friendly.

### Phase 4: Lifecycle controls

- Add `--lazy-load`
- Add `--idle-seconds`
- Verify release and reload behavior per backend

### Phase 5: LoRA-ready contract

- Add adapter slots to the server-side request and session design.
- Let request-time chat optionally specify `adapter_ref`.
- Keep adapter registry and allowlist policy outside the core chat request shape.
- Keep first-pass implementation disabled or backend-limited if necessary.
- Do not block server MVP on dynamic LoRA execution.

## Review-Sized Implementation Slices

Build the first server milestone in this order:

### Slice 1: `server run` command shape

- add Rust CLI parsing for `tentgent server run <MODEL_REF>`
- define first-pass flags:
  - `--home`
  - `--host`
  - `--port`
  - `--lazy-load`
  - `--idle-seconds`
- do not launch a real server yet
- goal:
  - lock the command surface and help text

### Slice 2: server spec and `SERVER_REF`

- define persisted server spec format
- define where specs live under `TENTGENT_HOME/servers/`
- define how `SERVER_REF` is generated
- write the first `server.toml`
- goal:
  - make server identity stable before runtime control exists

### Slice 3: Python server entry skeleton

- add a Python server entry point
- accept spec-derived runtime inputs
- expose `GET /healthz`
- do not require real model generation yet
- goal:
  - prove the long-lived process boundary

### Slice 4: Rust `server run` launches Python

- have Rust launch the Python server entry in foreground mode
- keep startup and shutdown output clean
- support `Ctrl+C` shutdown
- goal:
  - make `run` usable as a foreground command

### Slice 5: single-model chat endpoint

- add `POST /v1/chat`
- reuse the existing runtime session and backend router
- support one active request at a time
- goal:
  - complete the first real server-backed chat flow

### Slice 6: lifecycle policy

- add `--lazy-load`
- add `--idle-seconds`
- define release and reload behavior
- goal:
  - make memory residency policy explicit

### Slice 7: registry and process controls

- add `inspect`
- add `ls`
- add `ps`
- add `stop`
- add `start`
- add `rm`
- goal:
  - grow outward from a proven `run` path instead of inventing registry semantics first

## Verification Plan

- Start a server for one stored `safetensors` model and complete a chat request.
- Start a server for one stored `mlx` model and complete a chat request.
- Start a server for one stored `gguf` model and complete a chat request.
- Verify streaming output works over HTTP for each supported backend.
- Verify a lazy-loaded server can answer the first request and keep the model warm.
- Verify `idle_seconds` releases the model and reloads it on the next request.
- Verify server shutdown cleans up without leaving the model session in a broken state.

## Open Questions For Discussion

- Should the first HTTP streaming surface use SSE or chunked plain text?
- Should default startup be eager load or lazy load?
- Should default memory policy be keep-warm forever, or auto release after inactivity?
- Should the first server bind only one model reference, or allow future swap without restart?
- Should `ls` include all specs by default, or should a later `--all` filter be added only if we introduce more than one listing mode?
- How much of the future LoRA request shape should be reserved now?

## Current Recommendation

- Yes, make the `tentgent server` command family the next milestone.
- Yes, reuse the proven Python runtime path before adding more Rust-side intelligence.
- Yes, keep server scope to one process and one model in the first cut.
- No, do not start with LoRA before the server lifecycle exists.
- Yes, begin implementation with `server run` and proceed one review-sized slice at a time.

## Post-MVP Direction

- The server MVP is now in place as the runtime foundation.
- The next feature-focused runtime track should prefer single-server LoRA integration before multi-server orchestration.
- Multi-server coordination, cross-server state sharing, and distributed collaboration should be treated as a later systems track, not as the immediate next implementation step.
- Packaging and installation should be tracked in a separate future plan so runtime evolution and release engineering do not get mixed into the same execution track.

## Current Progress

- Slice 1 is now in place:
  - `tentgent server run <MODEL_REF>` exists in the Rust CLI
  - first-pass flags are parsed and rendered
  - the command surface and help text are locked in
- Slice 2 is now in place:
  - `tentgent server run` persists a stable server spec
  - `SERVER_REF` is derived from canonical server identity fields
  - the first `server.toml` is written under `TENTGENT_HOME/servers/<server_ref>/`
- Slice 3 is now in place:
  - the Python server entry exists under the daemon subproject
  - the long-lived process boundary is proven
  - `GET /healthz` returns a stable JSON health payload
- Slice 4 is now in place:
  - Rust `server run` launches the Python server entry in foreground mode
  - startup output stays inside the current terminal session
  - no real chat endpoint exists yet beyond `GET /healthz`
- Slice 5 is now in place:
  - `POST /v1/chat` exists
  - the existing runtime session and backend router are reused
  - one in-memory session serves one active request at a time
  - HTTP `stream=true` is still intentionally deferred and returns `501`
- Slice 6 is now in place:
  - eager startup and lazy startup are both explicit runtime policies
  - `idle_seconds` can release the loaded model after inactivity
  - the next request can reload the released model through the same session boundary
  - `/healthz` now reports lifecycle-facing state such as load mode, idle policy, and release metadata
- Slice 7 is now in place:
  - `tentgent server ls` lists persisted server specs
  - `tentgent server ps` lists running server processes
  - `tentgent server inspect <SERVER_REF>` merges spec and runtime state
  - `tentgent server stop <SERVER_REF>` stops a live process and clears stale process metadata
  - `tentgent server start <SERVER_REF>` launches a stored spec in background mode with server-local log files
  - `tentgent server rm <SERVER_REF>` removes a stopped server spec directory
  - foreground and background launch paths now share the same stable server spec identity
  - `tentgent server run --detach` can perform the initial launch in background mode
  - `server start` and `server stop` now default to concise summaries, with detailed inspection reserved for `--details`
