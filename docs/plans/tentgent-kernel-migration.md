# Tentgent Kernel Migration

This plan introduces `tentgent-kernel` as the next internal architecture layer.
It is the landing zone for unified runtime layout, application use cases,
machine capability state, and backend readiness gates.

## Summary

`tentgent-core` currently grew domain by domain. Each service owns some of its
own path resolution, store paths, manager construction, and cross-domain lookup
logic. That is practical for early iterations, but it makes new features such as
runtime capability detection, Linux/Windows backend gating, embedding/rerank,
and TUI readiness harder to keep consistent.

The migration path is intentionally non-disruptive. Add `src/tentgent-kernel`
as a new crate, move one coherent bundle at a time, keep `tentgent-core` as a
compatibility facade while CLI/HTTP continue to compile, then rename
`tentgent-kernel` back to `tentgent-core` after the cutover.

## Architecture Target

Architecture rules live in
[kernel-architecture.md](../contracts/kernel-architecture.md). This plan tracks
the migration order and current state.

Use Rust-flavored Clean Architecture with feature packages first. In this plan,
package means a Rust module folder inside `tentgent-kernel`, not a separate
Cargo crate:

```text
tentgent-kernel/
  foundation/       shared layout, platform facts, ids, errors
    layout/
      domain.rs
      resolver.rs
      usecases/
        query_runtime_layout.rs
        ensure_runtime_layout.rs
    platform/
      domain.rs
      probe.rs
      usecases/
        query_platform_facts.rs
  capabilities/     shared machine capability state and readiness gates
    usecases.rs
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
      usecases.rs
    daemon/
      usecases.rs
    session/
      usecases.rs
    runtime/
      usecases.rs
    train/
      usecases.rs
```

Package rules:

- feature packages own their domain rules, store adapters, and use cases
- each feature package exposes use cases through its own `usecases` module
- `foundation`: owns runtime-home path rules, platform facts, ids, filesystem
  helpers, clock, and shared errors
- `capabilities`: owns machine-local manifest types, stores, probes, and
  readiness use cases
- CLI/HTTP/TUI: parse input, call package use cases, render output
- package internals should stay small and can split into `domain`, `store`,
  `dto`, or `runtime` submodules only when that package needs them

## Runtime Layout

Create one shared layout object instead of per-service hard-coded path joins:

```rust
pub struct RuntimeLayout {
    pub home_dir: PathBuf,
    pub models_dir: PathBuf,
    pub adapters_dir: PathBuf,
    pub datasets_dir: PathBuf,
    pub servers_dir: PathBuf,
    pub sessions_dir: PathBuf,
    pub train_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub locks_dir: PathBuf,
    pub config_path: PathBuf,
    pub python_env_dir: PathBuf,
    pub bootstrap_dir: PathBuf,
    pub capability_manifest_path: PathBuf,
}
```

`RuntimeLayout` should support:

- create-capable resolution for normal mutation paths
- read-only resolution for diagnostics such as status and doctor
- env override handling from the workspace metadata names:
  `TENTGENT_HOME`, `TENTGENT_MODELS_DIR`, `TENTGENT_ADAPTERS_DIR`,
  `TENTGENT_DATASETS_DIR`, `TENTGENT_TRAIN_DIR`, `TENTGENT_CACHE_DIR`,
  `TENTGENT_RUNTIME_DIR`, `TENTGENT_LOG_DIR`, `TENTGENT_PYTHON_ENV_DIR`

## Application Context

Use a shared context to build feature packages from one layout:

```rust
pub struct AppContext {
    pub layout: RuntimeLayout,
    pub auth: features::auth::Package,
    pub models: features::model::Package,
    pub adapters: features::adapter::Package,
    pub datasets: features::dataset::Package,
    pub sessions: features::session::Package,
    pub servers: features::server::Package,
    pub daemon: features::daemon::Package,
    pub runtime: features::runtime::Package,
    pub training: features::train::Package,
    pub capabilities: capabilities::Package,
}
```

Use cases live inside their feature package and should own cross-domain
orchestration:

```rust
features::server::usecases::StartServer
features::session::usecases::Chat
features::runtime::usecases::Bootstrap
features::runtime::usecases::Doctor
features::train::usecases::CreatePlan
features::train::usecases::RunTraining
```

Managers in the old core can remain during migration, but new behavior should
land in kernel use cases.

## Capability Manifest

The runtime capability manifest is part of kernel, not a separate architecture
track:

```text
TENTGENT_HOME/runtime/capabilities.toml
```

It records machine-local facts such as:

- OS, architecture, Linux libc family/version
- CPU vendor and selected features
- GPU presence and driver/CUDA visibility when observable
- Python runtime source, env path, Python version
- bootstrap profile readiness: `base`, `local-model`, `training`, `full`
- backend readiness: CPU GGUF, safetensors/PEFT, MLX, training, future CUDA

It is local cached runtime state. It is regenerable, may be stale, and should
not affect model/store identity.

## Migration Bundles

Move one bundle at a time. A bundle is complete only when CLI/HTTP behavior is
unchanged, tests cover the moved boundary, and new code uses kernel structures
instead of adding more ad hoc path or manager logic to old core.

### Kernel Crate Shell

Move or add:

- workspace member: `src/tentgent-kernel`
- crate metadata and dependency baseline
- feature package root: `features`
- feature packages: `auth`, `model`, `adapter`, `dataset`, `server`,
  `daemon`, `session`, `runtime`, `train`
- shared packages: `foundation`, `capabilities`
- `usecases` module inside each feature package
- compatibility strategy for `tentgent-core` facade re-exports or adapters

Done when:

- `cargo test -p tentgent-kernel` runs
- CLI/HTTP still compile without behavior changes
- new kernel packages can be used without circular dependency on old core

Current state:

- `src/tentgent-kernel` exists as a compile-only workspace crate.
- It exposes feature package folders under `features/` with an initial
  `usecases` module.
- Shared runtime layout already follows the package shape:
  `foundation/layout/{domain.rs,resolver.rs,usecases/...}`.
- Platform facts already follow the package shape:
  `foundation/platform/{domain.rs,probe.rs,usecases/query_platform_facts.rs}`.

### Runtime Layout And Env Resolution

Move or centralize:

- project identity constants and workspace metadata usage
- `TENTGENT_HOME` and all path override env names
- platform default runtime-home resolution
- standard directory names and file names
- read-only vs create-capable resolution
- Python project/env/bootstrap path resolution

Old areas affected:

- `runtime_assets.rs`
- `doctor.rs` standard directory checks
- `model/store.rs`, `adapter/store.rs`, `dataset/store.rs`
- `server/store.rs`, `session/store.rs`, `train/store.rs`
- `daemon/store.rs`

Done when:

- path construction has one source of truth in `RuntimeLayout`
- old managers can be constructed from kernel layout or produce identical paths
- no new code calls `ProjectDirs::from("com", "tentserv", "tentgent")`
  outside `foundation::layout`

### Store Path Bundles

Move path ownership for:

- model store paths
- adapter store paths
- dataset store paths
- server store paths
- session store paths
- training store paths
- daemon/runtime/log paths
- config path
- capability manifest path

Do not move all read/write implementation at once unless the bundle stays
reviewable. It is acceptable to first move path objects and keep old store I/O
behind compatibility adapters.

Done when:

- each package store path object is derived from `RuntimeLayout`
- env override precedence is shared
- tests prove old and new paths match for representative temp homes

### Domain Types And Validation Rules

Move pure types and rules into the package that owns them:

- refs, short refs, and ambiguity rules
- model format / model capability types
- adapter compatibility metadata
- dataset split/schema summaries
- server specs and route capability types
- session context planning types
- training plan defaults and backend selection rules
- runtime capability state enums

Do not move persistence, process spawning, provider calls, or HTTP DTOs into
package domain submodules.

Done when:

- package domain modules are filesystem-free
- validation can be unit tested without temp directories
- CLI/HTTP DTOs can convert to/from domain inputs without owning rules

### Filesystem Store Implementations

Move persistence adapters after paths and domain types are stable:

- model store read/write/index
- adapter store read/write/index
- dataset store read/write/index
- session metadata and transcript store
- server spec/process metadata store
- training plan/run store
- daemon process metadata store
- capability manifest store

Done when:

- package stores do filesystem I/O only
- cross-store workflow logic is not inside package store modules
- migration preserves on-disk schema and existing refs

### Application Use Cases

Move orchestration into kernel use cases:

- model import/pull/add/remove/list/inspect
- adapter import/pull/add/remove/list/inspect
- dataset import/export/diff/synth/eval/list/inspect
- server create/run/start/stop/list/inspect/logs
- daemon status/start/stop/doctor/logs
- session create/list/inspect/chat/messages/compact/delete
- runtime bootstrap/init/status/doctor
- training plan create/list/inspect/run/logs/metrics

Done when:

- CLI and HTTP call the same use-case boundary where they share behavior
- package use cases own cross-domain loading and validation
- CLI/HTTP/TUI do not manually orchestrate store reads and writes

### Runtime Process And Python Adapters

Move process/runtime integration:

- Python runtime command construction
- packaged Python project resolution through kernel layout
- bootstrap script invocation
- server runtime process launch and health observation
- training runtime process launch
- provider/cloud server process separation
- backend probe commands

Done when:

- use cases call runtime adapters instead of spawning directly
- command construction is testable without launching when possible
- cloud provider paths remain separate from local backend capability checks

### Capability Manifest And Probes

Move/add:

- `TENTGENT_HOME/runtime/capabilities.toml`
- manifest structs and schema versioning
- lightweight platform probe
- profile readiness update after bootstrap
- Python import/backend probes after explicit profile install
- stale manifest detection

Done when:

- `tentgent runtime init --print` is non-mutating
- `tentgent runtime init` writes/refreshes the manifest
- doctor/status can render missing, stale, ready, blocked, and unsupported
  states
- no heavy dependencies are installed during lightweight probes

### Backend-Gated Workflows

Move gates into kernel use cases:

- local server start/run backend checks
- training launch backend checks
- future embedding/rerank local backend checks
- CPU vs GPU readiness decisions
- actionable next-step errors

Done when:

- local backend work fails before launch when the manifest already knows it is
  unavailable
- Python lazy import guards remain as runtime safety, not the first user-facing
  error
- cloud provider server behavior is unaffected

### CLI And HTTP Cutover

Move call sites:

- CLI commands call kernel use cases
- HTTP routes call kernel use cases
- rendering stays in CLI/TUI
- request/response DTOs stay in HTTP layer
- old core service managers become compatibility wrappers until removed

Done when:

- duplicate orchestration is removed from CLI/HTTP
- route behavior and error shapes stay stable
- regression tests pass for both CLI and HTTP surfaces

### Final Crate Rename

Move/rename:

- delete old core internals
- rename `tentgent-kernel` crate to `tentgent-core`
- update workspace members, crate dependencies, imports, docs, and release
  scripts

Done when:

- repository has one core crate again
- public CLI/HTTP behavior is stable
- docs no longer mention `tentgent-kernel` except in archived migration notes

## Affected Existing Plans

- Linux optional expansion should wait for the runtime layout, capability
  manifest, runtime adapter, and backend-gated workflow bundles before
  advertising profile-specific Linux readiness.
- Embedding/rerank local backend work should use kernel use cases and manifest
  gates rather than adding endpoint-specific platform probes.
- TUI V2 should render kernel-backed readiness instead of deriving backend
  state in the UI layer.

## Verification Themes

- Layout tests for env override precedence and read-only vs create-capable
  behavior.
- Compatibility tests proving old managers resolve the same paths when
  constructed from kernel layout.
- Use-case tests with temp runtime homes and mocked stores/probes.
- Doctor/status tests for missing, stale, ready, blocked, and unsupported
  manifest states.
- CLI/HTTP regression tests after every use-case cutover.
