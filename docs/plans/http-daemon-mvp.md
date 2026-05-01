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
- Treat the daemon HTTP API as the long-term programmatic peer of the CLI:
  durable local workflows should move through shared core services and be
  reachable from both CLI and HTTP unless there is a documented reason not to.
- Close CLI parity incrementally through reviewable slices instead of leaving
  the daemon as a partial chat-only facade.

## Non-Goals

- Do not expose a public internet service by default.
- Do not add multi-user auth in the MVP.
- Do not define provider-specific cloud chat behavior in this track; consume server specs created by the cloud provider server track.
- Do not make the daemon a scheduler for multiple loaded models yet.
- Do not promise full OpenAI API compatibility.
- Do not claim full CLI parity until the planned store, dataset, training,
  auth/diagnostics, and session mutation slices are implemented.

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
GET /v1/models/{model_ref}
GET /v1/adapters
GET /v1/adapters/{adapter_ref}
GET /v1/datasets
GET /v1/datasets/{dataset_ref}
GET /v1/servers
GET /v1/servers/{server_ref}
GET /v1/servers/{server_ref}/health
GET /v1/daemon/logs
GET /v1/daemon/logs/stdout
GET /v1/daemon/logs/stderr
GET /v1/servers/{server_ref}/logs
GET /v1/servers/{server_ref}/logs/stdout
GET /v1/servers/{server_ref}/logs/stderr
GET /v1/sessions
GET /v1/sessions/{session_ref}
GET /v1/sessions/{session_ref}/messages
POST /v1/servers
POST /v1/servers/{server_ref}/start
POST /v1/servers/{server_ref}/stop
POST /v1/chat
POST /v1/chat/completions
DELETE /v1/models/{model_ref}
DELETE /v1/adapters/{adapter_ref}
DELETE /v1/datasets/{dataset_ref}
DELETE /v1/servers/{server_ref}
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
- leave runtime launcher package ownership as a follow-up cleanup

Implemented module split:

- `lib.rs` exposes only `DaemonHttpServer` and `DaemonHttpState`
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
- `routes/diagnostics.rs` owns daemon and server log metadata/tail endpoints
- `routes/openai.rs` owns the limited OpenAI-compatible chat-completions wrapper
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

### Slice 8: Local Token Guard And Bind Safety

Add a minimal local security layer before encouraging broader daemon
integration.

Status: implemented in the active workspace.

Goals:

- keep loopback-local unauthenticated behavior available for development
- add an opt-in local bearer token for all daemon `/v1/*` routes
- keep `GET /healthz` public for readiness probes
- require auth before returning `404` for unknown `/v1/*` routes
- make wildcard and non-loopback binds require a token or an explicit unsafe flag
- prevent `TENTGENT_DAEMON_TOKEN` from being inherited by model-bound server
  child processes
- document curl and daemon lifecycle behavior with the token enabled

Implemented behavior:

- `TENTGENT_DAEMON_TOKEN` is env-only; unset, empty, and whitespace-only values
  disable auth
- non-empty token values are trimmed before comparison
- missing, malformed, and wrong bearer tokens all return the same JSON `401`
  with `WWW-Authenticate: Bearer`
- status responses expose only `auth.token_enabled`, never the token value
- host classification treats parsed loopback IPs and literal `localhost` as
  loopback, parsed unspecified IPs as wildcard, and all other hosts as unsafe
  non-loopback without DNS resolution
- `--allow-unsafe-bind` is available on both `tentgent daemon run` and the
  low-level `tentgent-http` binary

Bind matrix:

```text
host class             token enabled  allowed
loopback               no             yes
loopback               yes            yes
wildcard/non-loopback  no             no, unless --allow-unsafe-bind
wildcard/non-loopback  yes            yes, with warning
```

Review target:

- users can safely experiment with non-default host binding without accidentally
  exposing server lifecycle or chat endpoints

### Slice 9: Limited OpenAI-Compatible Chat Route

Add a compatibility route for tools that already know the OpenAI Chat
Completions wire shape.

Status: implemented in the active workspace.

Goals:

- add a limited `POST /v1/chat/completions` daemon route
- map OpenAI-style `messages`, `max_tokens`, `temperature`, and `stream` into
  the existing daemon chat proxy path
- use `model` as an explicit Tentgent server selector for the MVP, accepting the
  same full refs or unique prefixes as `server_ref`
- document that `model` selects a Tentgent server reference in this route, not a
  provider model name
- return OpenAI-shaped non-streaming and streaming success responses
- keep daemon-owned errors in the existing `{ "error", "message" }` shape
- ignore unsupported OpenAI fields in the MVP
- document that this is a compatibility shim, not full OpenAI API compatibility

Non-goals:

- do not support full OpenAI API compatibility
- do not support model-name based routing yet
- do not auto-start servers from `/v1/chat/completions`
- do not persist chat sessions
- do not support multimodal message content
- do not support OpenAI tools or function calling
- do not provide full OpenAI-compatible error objects in this slice
- do not encourage non-loopback binding; this route assumes the Slice 8 safety
  rules are already in place

Review target:

- OpenAI-style local clients can send basic chat-completion requests to an
  already-running Tentgent server through the daemon, while routing, auth, and
  lifecycle behavior remains daemon-owned

### Slice 10: Runtime Launcher Package Boundary Cleanup

Move Python model-bound server launch helpers out of the HTTP crate.

Status: implemented in the active workspace.

Goals:

- stop having `tentgent-cli` depend on `tentgent-http::server_runtime`
- move runtime launch/auth argument construction to `tentgent-core`
- keep CLI and daemon lifecycle behavior unchanged
- preserve existing server launch tests while relocating them to the new owner

Implemented boundary:

- `tentgent-core::server_runtime` owns Python model-bound server launch helpers,
  provider auth preflight, runtime command argument construction, and launcher
  environment sanitization
- `tentgent-cli` server commands and `tentgent-http` daemon lifecycle routes use
  the same core-owned launcher
- `tentgent-http` no longer exposes a `server_runtime` module; it remains the
  daemon HTTP entry and maps core launcher errors to daemon JSON responses
- `tentgent-cli` still depends on `tentgent-http` for `tentgent daemon run`
  entry types; removing that dependency is separate from this launcher cleanup

Review target:

- package ownership matches capability ownership before the daemon grows more
  routes

### Slice 11: Daemon Session API Foundation

Prepare the daemon to support TUI and external chat session workflows.

Status: implemented in the active workspace.

Goals:

- define a small session store under the Tentgent runtime home
- expose read-only session list, inspect, and message-tail endpoints
- use `tentgent.session.v1` metadata and `tentgent.session.message.v1`
  transcript records instead of `tentgent.chat.v1` training records
- avoid changing current stateless `/v1/chat` behavior until the session schema
  is stable
- keep session APIs additive; existing chat endpoints remain stateless
- defer session creation, append, repair, export, search, and TUI UI

Review target:

- the future TUI can reuse daemon-backed session state instead of duplicating
  chat/session storage

## CLI Parity Roadmap

The daemon is not yet a complete CLI replacement. The remaining HTTP work
should prioritize shared core services first, then thin CLI and HTTP wrappers.

### Slice 12: Store Inspect And Remove Parity

Status: implemented in the active workspace.

Goals:

- add inspect endpoints for models, adapters, and datasets
- add safe remove endpoints for models, adapters, datasets, and server specs
- refuse destructive store changes when dependent records would become invalid
- keep full-ref and unique-prefix resolution consistent with the CLI
- return pre-removal metadata on successful `DELETE`
- reject non-empty `DELETE` request bodies instead of accepting hidden options

Review target:

- external tools can inspect and clean managed store entries without shelling
  out

### Slice 13: Store Import And Pull Mutations

Status: planned.

Goals:

- add model import and Hugging Face pull endpoints
- add adapter import, pull, and bind endpoints
- add dataset import endpoints
- keep long-running local file and network operations explicit in the response
  contract

Review target:

- external tools can populate the model, adapter, and dataset stores through the
  daemon

### Slice 14: Dataset Tooling Parity

Status: planned.

Goals:

- expose dataset validate, template, synth, eval, export, and diff workflows
- preserve existing canonical dataset schema validation behavior
- make cloud synthesis progress, retry, timeout, and debug output visible over
  HTTP

Review target:

- dataset authoring and verification can run from local applications without
  CLI-only steps

### Slice 15: Training Plan API

Status: planned.

Goals:

- expose LoRA train-plan create, list, inspect, and remove endpoints
- keep training configuration identity and validation in `tentgent-core`
- avoid starting training runs in the plan-management slice

Review target:

- external tools can prepare reviewable training plans before launching work

### Slice 16: Training Run API

Status: planned.

Goals:

- expose LoRA run start, list, inspect, logs, and metrics endpoints
- keep run state and adapter registration consistent with CLI behavior
- make long-running training status observable without tailing files manually

Review target:

- training can be launched and monitored through the daemon with the same store
  side effects as the CLI

### Slice 17: Auth, Doctor, And Daemon Control Parity

Status: planned.

Goals:

- expose provider auth status without leaking secrets
- evaluate whether auth set/remove should remain CLI-only or require stricter
  daemon controls
- expose local doctor/status diagnostics
- add a daemon shutdown endpoint if it can preserve the Slice 8 safety model

Review target:

- local applications can diagnose Tentgent setup through HTTP while secret
  mutation remains intentionally constrained

### Slice 18: Session Mutation And Session-Aware Chat

Status: planned.

Goals:

- add session create, update, message append, and remove endpoints
- add optional `session_ref` recording for native and OpenAI-compatible chat
- keep stateless chat behavior available
- defer dataset export from sessions until transcript semantics settle

Review target:

- TUI and external agents can share durable chat context through the daemon
  without forcing every chat request to be stateful

## Open Questions

- Should daemon process management add a local socket after pid metadata is stable?
- Should a future auth slice add keychain or runtime-token storage beyond the
  Slice 8 env-only token?

Closed decisions:

- The first daemon entry is Rust-owned. It does not wrap the Python model server process.
- Slice 1 stores process metadata in `runtime/daemon.toml` and a pid in `runtime/tentgent.pid`; socket work remains future scope.
- Slice 8 uses only `TENTGENT_DAEMON_TOKEN`; token values are not written to
  runtime state, keychain, logs, server specs, or status responses.
- Slice 10 moved Python model-bound server launch helpers into `tentgent-core`
  instead of adding a dedicated runtime crate.
- Slice 11 keeps sessions read-only and separate from `tentgent.chat.v1`;
  training/eval export remains future work.
