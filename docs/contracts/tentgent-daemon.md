# Tentgent Daemon

This document defines the Rust daemon application host boundary.

## Scope

`src/tentgent-daemon/` is the long-running Rust process that owns app
bootstrap, transport listeners, daemon-local state, and background runtime
systems.

The crate should stay thin over `tentgent-kernel` use cases. It may keep
process-local services such as caches, schedulers, job registries, and transport
handler wiring, but product behavior should remain in kernel domain, ports,
infrastructure, and use cases.

## Dependency Direction

- `tentgent-daemon` depends on `tentgent-kernel`.
- `tentgent-daemon` may temporarily call older crates only through explicit
  migration bridges.
- `tentgent-kernel` must not depend on `tentgent-daemon`.
- `tentgent-cli` may launch or control the daemon process, but daemon request
  handling should live in `tentgent-daemon`.
- `tentgent-http` is the legacy HTTP entrypoint and should be treated as a
  migration source, not the final daemon architecture.
- `python/tentgent-daemon` remains the Python model runtime/backend subproject
  until that lower-level adapter is redesigned.

## Module Shape

- `src/main.rs`
  CLI-shaped process entrypoint for starting the daemon host.
- `src/bootstrap/`
  Builds logging, config, kernel adapter bundles, daemon services, and app
  state.
- `src/app/`
  Owns `DaemonApp`, shared app state, and service registry accessors.
- `src/kernel/`
  Owns daemon-local composition of kernel infrastructure components and exposes
  use-case builders to app services.
- `src/transport/`
  Owns long-running listeners such as REST, local sockets, or future streaming
  transports.
- `src/handlers/`
  Maps transport DTOs into daemon app services and kernel use cases.
- `src/runtime/`
  Owns daemon-local cache, scheduler, job registry, and future memory-like
  process state.

## Bootstrap Boundary

Daemon startup should be split into stable steps:

1. Parse process arguments or external config.
2. Resolve runtime layout enough to find `logs_dir`.
3. Initialize logging and tracing.
4. Build kernel infrastructure bundles.
5. Build daemon-local runtime systems.
6. Build transport entrypoints.
7. Run the enabled transports until shutdown.

Startup code should not embed route behavior. Route behavior belongs under
`handlers/`, with kernel-facing work delegated to app services.

## Logging Boundary

Daemon structured logs are written through `tracing`. The file sink should use
`RuntimeLayout.logs_dir` from the kernel layout resolver instead of hard-coded
paths.

The daemon tracing log uses a rolling `daemon.log` prefix under `logs/`.
Detached-process stdout and stderr files such as `daemon.stdout.log` and
`daemon.stderr.log` remain lifecycle-launch artifacts, not the primary
structured application log.

## Kernel Component Boundary

`src/kernel/` is allowed to know which concrete kernel infrastructure structs
compose a feature. Code outside this daemon composition layer should prefer
use-case builders such as `models().catalog_usecase()` or
`server_usecase()` instead of directly constructing filesystem stores, probes,
or runtime clients.

Cross-feature use cases should be built at the component registry level so
handlers do not need to know supporting dependencies. For example, chat can
receive a chat use case while the registry wires runtime resolution, model
resolution, adapter compatibility, and runtime execution behind it.

## Transport Boundary

REST is one transport entrypoint, not the daemon architecture itself. Future
local sockets or internal control channels should be added under `transport/`
and wired through the same `DaemonAppState`.

Transport handlers should:

- Parse request DTOs.
- Call daemon services or kernel use cases.
- Map domain results to response DTOs.
- Avoid owning persistence or runtime capability decisions directly.

## Runtime Boundary

Daemon-local runtime state is allowed when it is process-scoped:

- Memory cache.
- Job registry.
- Scheduler.
- Connection or session bookkeeping.

Persistent state and product decisions should remain in `tentgent-kernel`
unless the state is explicitly transport-only.
