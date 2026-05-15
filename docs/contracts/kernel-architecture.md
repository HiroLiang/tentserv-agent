# Kernel Architecture

This contract defines the internal shape of `src/tentgent-kernel` while the
project migrates behavior out of `tentgent-core`.

The current kernel is intentionally a skeleton. It should first make package
boundaries and shared data objects obvious. Behavior such as probing, file
persistence, process spawning, and workflow orchestration moves later, one
bundle at a time.

## Top-Level Shape

`tentgent-kernel` has three top-level areas:

```text
foundation/      low-level shared facts and path data
capabilities/    machine capability domain data
features/        product feature packages
```

Rules:

- `foundation` contains shared primitives and machine facts. It does not know
  product workflows such as model import, server launch, session chat, or
  training.
- `capabilities` contains shared readiness vocabulary. It is not a user-facing
  feature package.
- `features/*` maps to product areas and command families.
- CLI, HTTP, and TUI stay as input/rendering layers. They should not gain new
  ad hoc path, probe, or backend readiness logic while the migration is active.

## Current Skeleton

The current implemented shape is data-first:

```text
src/tentgent-kernel/src/
  foundation/
    error.rs
    layout/
      domain.rs
    platform/
      domain.rs
      infra.rs
      ports.rs
      tests.rs
  capabilities/
    domain.rs
  features/
    auth/
      usecases.rs
    model/
      usecases.rs
    adapter/
      usecases.rs
    dataset/
      usecases.rs
    server/
      domain.rs
      usecases.rs
    daemon/
      usecases.rs
    session/
      usecases.rs
    runtime/
      domain.rs
      usecases.rs
    train/
      domain.rs
      usecases.rs
```

For now, `domain.rs` files own structures and enums. `ports.rs` files may
define narrow traits for external facts the package needs. `usecases.rs` files
may exist as placeholders, but they should not hide implementation logic before
the relevant bundle is moved.

## Domain Files

Use `domain.rs` for:

- pure structs and enums
- stable names shared by later stores, probes, services, or use cases
- data that can be tested without filesystem, network, subprocess, or Python
  runtime access

Do not put these in `domain.rs`:

- filesystem reads or writes
- environment-variable lookup
- process spawning
- backend probing
- CLI/HTTP/TUI rendering
- hidden dependency injection

## Foundation

`foundation` owns low-level shared structures.

Current domain areas:

- `foundation/layout/domain.rs`: runtime home and standard path data objects.
- `foundation/platform/domain.rs`: OS, arch, libc, CPU, GPU, CUDA, and Metal
  fact objects.
- `foundation/platform/ports.rs`: `PlatformProbe`, the trait for reading
  current platform facts.
- `foundation/platform/infra.rs`: `StdPlatformProbe`, the standard
  implementation that reads current platform facts.
- `foundation/platform/tests.rs`: fake probe and standard probe smoke tests.
- `foundation/error.rs`: shared kernel errors and result alias.

Future implementation files may be added when the bundle moves:

```text
foundation/layout/resolver.rs
foundation/fs/
foundation/ids/
foundation/time/
```

These should remain internal helpers for shared mechanics, not product
workflow owners.

## Capabilities

`capabilities/domain.rs` owns machine readiness vocabulary:

- runtime profile readiness
- backend kinds
- backend readiness state
- machine capability state snapshots

Use the term capability state for the data and cache. Do not add a separate
persisted-wrapper module unless a later migration proves that split removes
real complexity.

The likely persisted cache path remains:

```text
TENTGENT_HOME/runtime/capabilities.toml
```

That file is local cached state, not user data identity. It should be
regenerable and safe to refresh later.

## Features

Each feature package maps to a product area:

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

Feature packages may eventually contain:

```text
domain.rs
store.rs
service.rs
runtime.rs
usecases.rs
```

Only add these files when the feature needs them. Prefer a small, consistent
package over a theoretical Clean Architecture layout. If a file has no real
job yet, keep it empty or do not create it.

## Dependency Direction

Allowed direction:

```text
features/* -> capabilities -> foundation
features/* -> foundation
```

Disallowed direction:

```text
foundation -> capabilities
foundation -> features/*
capabilities -> features/*
```

Cross-feature behavior should be explicit. If `server` later needs runtime
layout or capability state, pass those data objects or call a clear package
boundary. Do not hide feature-to-feature coupling inside probes or stores.

## Runtime Layout Rules

All runtime-home and standard path data should eventually flow through
`foundation/layout`.

Current skeleton only defines the `RuntimeLayout` data object. Resolver logic,
environment overrides, and create/read-only behavior are deferred.

Future path resolution should cover:

- `TENTGENT_HOME`
- standard model, adapter, dataset, session, server, train, cache, runtime,
  log, and lock directories
- managed Python env and bootstrap cache paths
- capability state cache path

## Platform Rules

`PlatformFacts` describes machine facts only:

- OS and architecture
- Linux libc family/version
- CPU vendor, brand, and features
- GPU devices and hardware/runtime visibility such as CUDA or Metal

Platform facts do not mean a backend is ready. Backend readiness belongs to
capability state.

For example:

- CUDA visibility belongs in platform/GPU facts.
- “training backend can use CUDA” belongs in capability state.

## Persistence Rules

The source of truth remains file-based by default. SQLite may be added later as
a rebuildable index/cache, but it should not become the first source of truth
for models, adapters, datasets, sessions, servers, runtime state, or training
runs.

When persistence moves into kernel, product-specific stores should live in the
owning package. Shared low-level file helpers can live under `foundation/fs`.

## Migration Rule

During migration, `tentgent-core` remains the behavior owner. New kernel code
should only add structure or move a coherent bundle with tests. Do not add new
ad hoc path, probe, or manager logic to old core when the matching kernel
package already exists.
