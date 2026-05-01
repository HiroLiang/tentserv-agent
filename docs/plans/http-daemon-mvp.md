# HTTP Daemon MVP

This plan defines the service-entry track: make Tentgent usable as a local HTTP subsystem that other projects can integrate without shelling out to every CLI command.

## Priority

Run this after the cloud provider server and first cloud dataset slices unless an external integration depends on it sooner. This track should land before a full TUI if the TUI needs stable API-backed state.

## Current State

- `tentgent server` already launches a model-bound local HTTP chat server.
- `POST /v1/chat` is defined in [../contracts/server-chat.md](../contracts/server-chat.md).
- `src/tentgent-http/` has a low-level daemon lifecycle entry point with `GET /healthz` and `GET /v1/status`.
- `tentgent daemon` has `run`, `status`, and `stop` lifecycle commands and starts the Rust HTTP daemon in foreground mode.

## Scope

- Promote the scaffold into a real local daemon/API entry point.
- Expose store and server lifecycle state over HTTP.
- Keep local-first defaults: bind to `127.0.0.1` unless explicitly changed.
- Reuse `tentgent-core` managers for models, adapters, datasets, train plans, and servers.
- Coordinate existing model-bound servers instead of replacing them in the first pass.

## Non-Goals

- Do not expose a public internet service by default.
- Do not add multi-user auth in the MVP.
- Do not define provider-specific cloud chat behavior in this track; consume server specs created by the cloud provider server track.
- Do not make the daemon a scheduler for multiple loaded models yet.
- Do not promise full OpenAI API compatibility.

## Command Surface

Planned commands:

```text
tentgent daemon run [--host 127.0.0.1] [--port 8790] [--home <PATH>]
tentgent daemon status [--home <PATH>]
tentgent daemon stop [--home <PATH>]
```

The standalone binary may remain available for packaging or embedding:

```text
tentgent-http --host 127.0.0.1 --port 8790 [--home <PATH>]
```

## HTTP Surface

Start read-only, then add mutations.

Implemented endpoints:

```text
GET /healthz
GET /v1/status
GET /v1/models
GET /v1/adapters
GET /v1/datasets
GET /v1/servers
GET /v1/servers/{server_ref}
GET /v1/servers/{server_ref}/health
GET /v1/daemon/logs
GET /v1/daemon/logs/stdout
GET /v1/daemon/logs/stderr
GET /v1/servers/{server_ref}/logs
GET /v1/servers/{server_ref}/logs/stdout
GET /v1/servers/{server_ref}/logs/stderr
POST /v1/servers
POST /v1/servers/{server_ref}/start
POST /v1/servers/{server_ref}/stop
POST /v1/chat
```

## Execution Order

### Slice 1: Daemon Command Shape

Replace the CLI scaffold with a real command shape.

Status: implemented in the active workspace.

Goals:

- add `daemon run`, `daemon status`, and `daemon stop`
- choose default host and port
- persist process metadata under runtime state
- keep help text clear about local-only defaults

Review target:

- command UX is stable before adding HTTP behavior

### Slice 2: Rust HTTP Skeleton

Make `tentgent-http` serve basic local HTTP.

Status: implemented in the active workspace.

Goals:

- implement `GET /healthz`
- load runtime home and platform status
- share response/error conventions with the existing Python server where practical
- keep dependencies minimal

Review target:

- `tentgent-http` can be packaged and smoke-tested without model files

### Slice 2.1: HTTP Skeleton Polish

Tighten the first HTTP contract before adding store APIs.

Status: implemented in the active workspace.

Goals:

- use a shared Tentgent version source in daemon HTTP responses
- keep every HTTP response JSON, including 404, 405, and request errors
- document the initial HTTP daemon response and error contract
- add minimal request logging with method, path, status, peer, and elapsed time
- defer a `{ data, error }` success wrapper until the read-only store API shape settles

Review target:

- the first HTTP surface has enough observability and response consistency for Slice 3

### Slice 3: Read-Only Store API

Expose current managed state.

Status: implemented in the active workspace.

Goals:

- list models, adapters, datasets, and server specs
- inspect one server
- avoid mutations while the API shape settles
- return stable JSON objects that are independent of terminal table formatting
- use the daemon runtime home for store discovery while preserving store
  specific env override precedence

Review target:

- another local project can discover Tentgent state over HTTP

### Slice 4: Server Lifecycle API

Add controlled mutations for stored model servers.

Status: implemented in the active workspace.

Goals:

- start an existing server spec
- stop a running server
- create a new server spec with explicit model, host, port, and lazy-load policy
- prevent unsafe deletion or mutation while a server is live
- keep chat proxying and server deletion out of this slice

Review target:

- external tools can control `tentgent server` without shelling out

### Slice 5: Chat Proxy

Add a daemon-level chat entry.

Status: implemented in the active workspace.

Goals:

- accept the existing `server-chat` request shape
- require either a server reference or an unambiguous default policy
- proxy to the model-bound server when possible
- pass through non-streaming JSON and streaming Server-Sent Events from the
  selected model-bound server
- return clear errors when no server is running or server selection is ambiguous

Review target:

- integrations can send chat through one stable daemon URL

### Slice 6: Daemon Readiness And Integration Polish

Clarify server readiness for HTTP integrations.

Status: implemented in the active workspace.

Goals:

- expose `GET /v1/servers/{server_ref}/health`
- allow `POST /v1/servers/{server_ref}/start` to wait for target `/healthz`
- keep `/v1/chat` no-auto-start behavior unchanged
- improve proxy transport failure messages with a health-check hint
- document the create, start, readiness, chat, and stop flow

Review target:

- external tools can distinguish stored specs, running processes, reachable
  HTTP targets, and chat-ready servers

### Slice 6.1: Split HTTP Library Modules

Reduce `tentgent-http` review risk by splitting the large library root.

Status: implemented in the active workspace.

Goals:

- keep `lib.rs` as the crate root and public export surface
- split daemon process wiring, HTTP parsing/writing, DTOs, response helpers,
  and route handlers into focused modules
- preserve all Slice 5 and Slice 6 endpoint behavior and response shapes
- leave `server_runtime.rs` in place for now, even though CLI use of it is a
  follow-up package-boundary cleanup

Implemented module split:

- `lib.rs` exposes only `server_runtime`, `DaemonHttpServer`, and
  `DaemonHttpState`
- `app.rs` owns daemon HTTP binding, accept-loop wiring, connection handling,
  shared state, and request logging
- `http.rs` owns the handcrafted HTTP request parser, response writer, and body
  variants
- `response.rs` owns JSON, raw proxy, and error response helpers
- `dto.rs` owns daemon request and response DTOs
- `routes/status.rs` owns `GET /healthz` and `GET /v1/status`
- `routes/store.rs` owns read-only store discovery and server DTO mapping
- `routes/lifecycle.rs` owns server create/start/stop/health and readiness
  probing
- `routes/chat.rs` owns daemon chat proxy selection and passthrough
- `routes/tests.rs` keeps route-level integration-style unit tests outside the
  production dispatcher

Review target:

- future daemon slices can change one capability area without editing a
  multi-thousand-line `lib.rs`

### Slice 7: Daemon Log Diagnostics API

Expose daemon and model-bound server logs through fixed, read-only diagnostics
endpoints.

Status: implemented in the active workspace.

Goals:

- expose daemon stdout/stderr metadata and tail content
- expose server stdout/stderr metadata and tail content by full ref or existing
  unique prefix resolution
- serve only fixed known log paths from Tentgent state, never arbitrary
  filesystem paths
- keep log content tailing byte-based with explicit `tail_bytes` validation
- document that local path fields may expose filesystem layout under the
  loopback-local MVP

Review target:

- external integrations can inspect daemon and server failures without shelling
  out or manually opening runtime log files

### Slice 8: Limited OpenAI-Compatible Chat Route

Add a compatibility route for tools that already know the OpenAI Chat
Completions wire shape.

Status: planned.

Goals:

- add a limited `POST /v1/chat/completions` daemon route
- map OpenAI-style `messages`, `max_tokens`, `temperature`, and `stream` into
  the existing daemon chat proxy path
- use `model` as an explicit server selector for the MVP, accepting the same
  full refs or unique prefixes as `server_ref`
- return OpenAI-shaped non-streaming and streaming responses where practical
- document that this is a compatibility shim, not full OpenAI API compatibility

Review target:

- common local SDK clients can call the daemon without custom Tentgent request
  code for the first chat path

### Slice 9: Local Token Guard And Bind Safety

Add a minimal local security layer before encouraging broader daemon
integration.

Status: planned.

Goals:

- keep loopback-local unauthenticated behavior available for development
- add an opt-in local bearer token for mutation and chat endpoints
- make non-loopback binds require an explicit unsafe flag or token
- document curl, SDK, and daemon lifecycle behavior with the token enabled

Review target:

- users can safely experiment with non-default host binding without accidentally
  exposing server lifecycle or chat endpoints

### Slice 10: Runtime Launcher Package Boundary Cleanup

Move Python model-bound server launch helpers out of the HTTP crate.

Status: planned.

Goals:

- stop having `tentgent-cli` depend on `tentgent-http::server_runtime`
- move runtime launch/auth argument construction to `tentgent-core` or a small
  runtime-focused crate
- keep CLI and daemon lifecycle behavior unchanged
- preserve existing server launch tests while relocating them to the new owner

Review target:

- package ownership matches capability ownership before the daemon grows more
  routes

### Slice 11: Daemon Session API Foundation

Prepare the daemon to support TUI and external chat session workflows.

Status: planned.

Goals:

- define a small session store under the Tentgent runtime home
- expose read-only session list/inspect endpoints first
- decide whether stored transcripts use `tentgent.chat.v1` records or a thinner
  session-specific schema
- avoid changing current stateless `/v1/chat` behavior until the session schema
  is stable

Review target:

- the future TUI can reuse daemon-backed session state instead of duplicating
  chat/session storage

## Open Questions

- Should daemon process management add a local socket after pid metadata is stable?
- Should Slice 9 store the local token in runtime state, keychain, or an
  operator-provided environment variable?
- Should Slice 10 move Python server runtime launch helpers into
  `tentgent-core` or a dedicated runtime crate?

Closed decisions:

- The first daemon entry is Rust-owned. It does not wrap the Python model server process.
- Slice 1 stores process metadata in `runtime/daemon.toml` and a pid in `runtime/tentgent.pid`; socket work remains future scope.
