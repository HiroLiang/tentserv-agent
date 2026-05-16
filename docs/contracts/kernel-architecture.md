# Kernel Architecture

This contract defines the internal shape of `src/tentgent-kernel` while the
project migrates behavior out of `tentgent-core`.

The current kernel is intentionally incremental. It should make package
boundaries and shared data objects obvious first, then move behavior one
coherent bundle at a time.

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

## Current Package Shape

The source tree is the authority for exact files. Current packages follow this
shape:

- `domain.rs` owns structures and enums.
- `ports.rs` owns narrow traits for package boundaries.
- `infra/` or `infra.rs` owns standard implementations that touch the local
  machine, filesystem, environment, or subprocesses.
- `usecases.rs` may exist as a placeholder in feature packages, but should not
  hide implementation logic before the relevant bundle is moved.

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

- `foundation/layout/domain.rs`: runtime home, data root, and standard path
  data objects.
- `foundation/layout/infra.rs`: `StdRuntimeLayoutResolver`, the standard
  implementation that resolves roots and derived paths.
- `foundation/layout/ports.rs`: `RuntimeLayoutResolver`, the trait for
  resolving runtime layout in read-only or create-capable modes.
- `foundation/layout/tests.rs`: explicit root, env root, read-only, and create
  mode tests.
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

`capabilities/ports.rs` defines the narrow boundaries for:

- probing machine capability state from runtime layout and platform facts
- loading and saving cached capability state
- checking backend and runtime-profile readiness for feature gates
- resolving current or refreshed capability snapshots for callers without
  making CLI, HTTP, or TUI assemble layout and platform probes themselves
- enforcing backend and runtime-profile readiness without exposing checker
  details to feature packages

Current standard implementations are `FileCapabilityStateStore`,
`StdMachineCapabilitiesProbe`, and `StdCapabilityChecker`. Heavy Python import
probes and backend launch checks should be added later as an explicit probe
bundle, not hidden in the lightweight probe.

`capabilities/usecases/` owns orchestration implementations and their local
request/response structs. For example, `resolver.rs` keeps
`MachineCapabilitiesInput` and `MachineCapabilitiesSnapshot` next to
`StdMachineCapabilitiesResolver`. CLI, HTTP, and TUI should call use cases
instead of assembling layout, platform, cache, and probe steps themselves.

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
features/config/
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

`features/runtime/domain.rs` owns pure runtime setup names and state:

- bootstrap profiles and resolved bootstrap plans
- Python runtime source and resolved Python project/environment layout
- Python entrypoint script names exposed by the daemon package
- runtime initialization/readiness snapshots

It must not spawn bootstrap scripts, run Python, inspect installed packages, or
read environment variables directly. Those jobs belong in runtime infra or
use cases once their migration bundle moves.

`features/config/domain.rs` owns pure user-config names and rules:

- config file name, schema version, and config section data
- daemon URL and token resolution source enums
- daemon endpoint formatting and default daemon endpoint values
- pure daemon URL/token precedence rules
- secret-like config key classification

It must not read environment variables, load or save TOML files, traverse TOML
values, or read daemon process metadata directly. Those jobs belong in config
infra or callers that map local state into config domain inputs.

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

Current layout package defines `RuntimeLayoutInput`, `RuntimeLayout`, and
`StdRuntimeLayoutResolver`.

The public layout shape should stay small:

- `home_dir`: control-plane root for config, sessions, servers, runtime, logs,
  locks, managed Python, bootstrap tools, and capability state.
- `data_root_dir`: data-plane root for models, adapters, datasets, training
  data, and cache.

If `data_root_dir` is unset, it resolves to `home_dir`. Avoid exposing many
per-directory path overrides as the main user-facing contract; advanced users
can use a different `home_dir`, a different `data_root_dir`, or OS-level
mounts/symlinks/junctions.

Implemented path resolution covers:

- `TENTGENT_HOME` / explicit `home_dir`
- `TENTGENT_DATA_ROOT` / optional explicit `data_root_dir`
- fixed standard subpaths under those roots
- read-only vs create-capable resolution mode

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
