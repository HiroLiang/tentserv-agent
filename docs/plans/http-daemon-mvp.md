# HTTP Daemon MVP

This plan defines the service-entry track: make Tentgent usable as a local HTTP subsystem that other projects can integrate without shelling out to every CLI command.

## Priority

Run this after the cloud provider server and first cloud dataset slices unless an external integration depends on it sooner. This track should land before a full TUI if the TUI needs stable API-backed state.

## Current State

- `tentgent server` already launches a model-bound local HTTP chat server.
- `POST /v1/chat` is defined in [../contracts/server-chat.md](../contracts/server-chat.md).
- `src/tentgent-http/` exists as a Rust scaffold only.
- `tentgent daemon` exists as a CLI scaffold only.

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

First endpoints:

```text
GET /healthz
GET /v1/status
GET /v1/models
GET /v1/adapters
GET /v1/datasets
GET /v1/servers
GET /v1/servers/{server_ref}
```

Later endpoints:

```text
POST /v1/servers
POST /v1/servers/{server_ref}/start
POST /v1/servers/{server_ref}/stop
POST /v1/chat
```

`POST /v1/chat` should either proxy to a selected local or cloud server spec, or return a clear error explaining how to start one.

## Execution Order

### Slice 1: Daemon Command Shape

Replace the CLI scaffold with a real command shape.

Goals:

- add `daemon run`, `daemon status`, and `daemon stop`
- choose default host and port
- persist process metadata under runtime state
- keep help text clear about local-only defaults

Review target:

- command UX is stable before adding HTTP behavior

### Slice 2: Rust HTTP Skeleton

Make `tentgent-http` serve basic local HTTP.

Goals:

- implement `GET /healthz`
- load runtime home and platform status
- share response/error conventions with the existing Python server where practical
- keep dependencies minimal

Review target:

- `tentgent-http` can be packaged and smoke-tested without model files

### Slice 3: Read-Only Store API

Expose current managed state.

Goals:

- list models, adapters, datasets, and server specs
- inspect one server
- avoid mutations while the API shape settles
- return stable JSON objects that are independent of terminal table formatting

Review target:

- another local project can discover Tentgent state over HTTP

### Slice 4: Server Lifecycle API

Add controlled mutations for stored model servers.

Goals:

- start an existing server spec
- stop a running server
- create a new server spec with explicit model, host, port, and lazy-load policy
- prevent unsafe deletion or mutation while a server is live

Review target:

- external tools can control `tentgent server` without shelling out

### Slice 5: Chat Proxy

Add a daemon-level chat entry.

Goals:

- accept the existing `server-chat` request shape
- require either a server reference or an unambiguous default policy
- proxy to the model-bound server when possible
- return clear errors when no server is running or streaming is requested before support exists

Review target:

- integrations can send chat through one stable daemon URL

## Open Questions

- Should the first daemon be Rust-only, or should it wrap the Python server process?
- Should daemon process management use pid files only, or a local socket as well?
- Should daemon auth be absent for loopback-only MVP, or use a local token from runtime state?
