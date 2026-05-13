# Kernel Architecture

This contract defines the internal architecture boundary for `src/tentgent-kernel`.
It is the source of truth for kernel module placement during the migration from
`tentgent-core`.

## Top-Level Shape

`tentgent-kernel` has three top-level areas:

```text
foundation/      low-level shared primitives
capabilities/    shared machine readiness state
features/        user-visible product feature packages
```

Rules:

- `foundation` does not know product workflows such as model import, server
  launch, session chat, or training.
- `capabilities` is shared readiness state, not a user feature package.
- `features/*` owns product behavior and may call `foundation` and
  `capabilities`.
- CLI, HTTP, and TUI parse input, call feature use cases, and render output.

## Foundation

`foundation` owns primitives that multiple feature packages need but that do
not make product decisions.

Allowed areas:

```text
foundation/layout/      runtime home, env overrides, standard paths
foundation/platform/    OS, arch, libc, CPU, GPU, CUDA, Metal facts
foundation/fs/          atomic write, bounded scans, locks, JSONL helpers
foundation/ids/         refs, short refs, hash identifiers
foundation/time/        clock traits and time helpers
foundation/error.rs     shared kernel errors/results
```

Current implemented shape:

```text
foundation/layout/
  domain.rs
  resolver.rs
  usecases/
    query_runtime_layout.rs
    ensure_runtime_layout.rs

foundation/platform/
  domain.rs
  probe.rs
  usecases/
    query_platform_facts.rs
```

Foundation use cases may query facts or resolve paths. They must not write
capability manifests or decide whether a backend is ready.

## Capabilities

`capabilities` owns machine-local readiness:

- `domain.rs` owns readiness language such as backend kinds and capability
  states
- `manifest.rs` owns the persisted manifest wrapper such as
  `MachineCapabilityManifest`
- platform/runtime/backend readiness states
- manifest load/save boundaries
- lightweight and profile-specific probes
- `CapabilityRead`, `CapabilityProbe`, and `CapabilityManifestStore`
  interfaces

The manifest is a local cache, not user data identity:

```text
TENTGENT_HOME/runtime/capabilities.toml
```

Recommended persistence format is TOML. Default behavior should prefer the
cached manifest when present, initialize only when missing, and refresh only on
explicit refresh, bootstrap/profile changes, schema changes, or stale-state
handling.

Feature packages must not directly probe OS, GPU, Python imports, or backend
dependencies. They should ask `CapabilityRead`.

`CapabilityRead` has two kinds of operations:

- `check_*` returns state for status, doctor, and TUI display.
- `ensure_*_ready` gates feature execution and returns an actionable error when
  the cached manifest says a profile or backend is not ready.

`ensure_*_ready` must not imply that hardware or dependencies can be made ready
by the call itself. It only asserts readiness before starting work.

Capability probes should return probe reports, not persisted manifests. Refresh
use cases assemble `MachineCapabilityManifest` from probe reports and write it
through `CapabilityManifestStore`.

## Features

`features/*` contains user-visible product capabilities:

```text
features/auth/
features/model/
features/adapter/
features/dataset/
features/server/
features/daemon/
features/session/
features/runtime/
features/train/
```

Each feature package may contain:

```text
domain.rs
store.rs
service.rs
runtime.rs
usecases/
```

Only create these files when the feature needs them. Keep small features as a
`usecases.rs` file until splitting reduces reading cost.

Feature use cases own product orchestration. Store modules do filesystem I/O
only. Runtime modules adapt external processes or Python helpers.

## Dependency Flow

Preferred flow:

```text
features/server/usecases
  -> capabilities::CapabilityRead
    -> capabilities use cases / manifest
      -> foundation/platform::QueryPlatformFacts
        -> foundation/platform::PlatformProbe
          -> PlatformFacts
```

Rules:

- `features/*` may depend on `foundation` and `capabilities`.
- `capabilities` may depend on `foundation`.
- `foundation` must not depend on `capabilities` or `features`.
- Feature packages should not depend on each other directly unless a use case
  explicitly coordinates cross-feature behavior through a shared context.

## Platform And Backend Rules

`PlatformFacts` describes machine facts only:

- OS and architecture
- Linux libc family/version
- CPU vendor, brand, and features
- GPU devices and hardware/runtime facts such as CUDA or Metal visibility

Platform facts do not mean a backend is ready.

Backend readiness belongs to `capabilities`. For example, CUDA belongs in GPU
facts, while `llama-cpp CUDA ready` or `training CUDA ready` belongs in backend
capability state.

## Layout Rules

All runtime-home and path resolution should flow through
`foundation/layout`.

Use:

- `QueryRuntimeLayout` for read-only diagnostics and print-plan behavior.
- `EnsureRuntimeLayout` for mutation paths that may create standard
  directories.

Feature packages should receive `RuntimeLayout` or a layout resolver. They
should not construct paths from `TENTGENT_HOME` manually.

## Persistence Rules

The source of truth remains file-based by default. SQLite may be added later as
a rebuildable index/cache, but should not become the first source of truth for
models, adapters, datasets, sessions, servers, runtime manifests, or training
runs.

Low-level file helpers should live under `foundation/fs`. Product-specific
persistence should live in the owning feature package or in `capabilities` for
the capability manifest.

## Migration Rule

During migration, `tentgent-core` may remain a compatibility facade. New or
moved behavior should follow this contract instead of adding new ad hoc path,
probe, or manager logic to old core.
