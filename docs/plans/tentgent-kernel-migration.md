# Tentgent Kernel Migration

This plan introduces `tentgent-kernel` as the next internal architecture layer.
It is the landing zone for shared package shape, runtime layout data, platform
facts, capability state data, and future feature bundles.

## Summary

`tentgent-core` currently grew domain by domain. Each service owns some path
resolution, store paths, manager construction, and cross-domain lookup logic.
That was practical for early iterations, but it makes runtime capability
detection, Linux/Windows backend gating, embedding/rerank, and TUI readiness
harder to keep consistent.

The migration path is intentionally non-disruptive:

1. Add `src/tentgent-kernel` as a new crate.
2. Keep only package shape and domain data until the boundaries feel right.
3. Move one coherent bundle at a time.
4. Keep `tentgent-core` as a compatibility facade while CLI/HTTP continue to
   compile.
5. Rename `tentgent-kernel` back to `tentgent-core` only after the cutover.

## Architecture Contract

Architecture rules live in
[kernel-architecture.md](../contracts/kernel-architecture.md). This plan
tracks migration order and current state.

The current source tree is the authority for exact files. The crate remains
data-first where possible: `domain.rs` contains structures and enums,
`ports.rs` defines narrow package traits, and `infra/` or `infra.rs` contains
standard local-machine implementations. `usecases.rs` files are placeholders
only until a feature bundle actually moves.

## Current State

Implemented:

- `src/tentgent-kernel` workspace crate.
- Feature package folders for auth, model, adapter, dataset, server, daemon,
  session, runtime, and train.
- Foundation layout domain, port, and infra: `RuntimeLayoutInput`,
  `RuntimeLayout`, `RuntimeLayoutResolver`, `StdRuntimeLayoutResolver`, and
  centralized layout tests.
- Foundation platform domain objects: OS, arch, libc, CPU, GPU, CUDA, Metal.
- Foundation platform port and infra: `PlatformProbe`, `StdPlatformProbe`, and
  centralized platform tests.
- Capability domain objects: runtime profile readiness, backend kinds, backend
  readiness, machine capability state.
- Capability ports for machine capability probing, cached state load/save, and
  backend or runtime-profile readiness checks.
- Capability infra implementations split by port: file-backed TOML state store,
  lightweight platform/layout probe, and cached-state checker.
- Small feature input data objects for runtime, server, and training.

Not implemented by design:

- heavy Python import/backend probe implementations
- feature workflow use cases
- process/runtime adapters
- compatibility adapters from old core

## Migration Bundles

Move one bundle at a time. A bundle is complete only when CLI/HTTP behavior is
unchanged, tests cover the moved boundary, and new code uses kernel structures
instead of adding more ad hoc path or manager logic to old core.

### 1. Runtime Layout And Env Resolution

Move or centralize:

- project identity constants and workspace metadata usage
- `TENTGENT_HOME` and optional data-root override
- platform default runtime-home resolution
- standard directory names and file names
- read-only vs create-capable resolution
- Python project/env/bootstrap path resolution

Old areas affected:

- `runtime_assets.rs`
- `doctor.rs` standard directory checks
- model, adapter, dataset, server, session, train, and daemon stores

Done when:

- path construction has one source of truth in `RuntimeLayout`
- the first-class knobs are `home_dir` and optional `data_root_dir`, not a large
  set of per-directory overrides
- old managers can be constructed from kernel layout or produce identical paths
- no new code calls `ProjectDirs::from("com", "tentserv", "tentgent")`
  outside the layout bundle

### 2. Store Path Bundles

Move path ownership for:

- model store paths
- adapter store paths
- dataset store paths
- server store paths
- session store paths
- training store paths
- daemon/runtime/log paths
- config path
- capability state cache path

Do not move all read/write implementation at once unless the bundle stays
reviewable. It is acceptable to first move path objects and keep old store I/O
behind compatibility adapters.

### 3. Domain Types And Validation Rules

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

### 4. Filesystem Store Implementations

Move persistence adapters after paths and domain types are stable:

- model store read/write/index
- adapter store read/write/index
- dataset store read/write/index
- session metadata and transcript store
- server spec/process metadata store
- training plan/run store
- daemon process metadata store

Done when:

- package stores do filesystem I/O only
- cross-store workflow logic is not inside package store modules
- migration preserves on-disk schema and existing refs

### 5. Runtime Process And Python Adapters

Move process/runtime integration:

- Python runtime command construction
- packaged Python project resolution through kernel layout
- bootstrap script invocation
- server runtime process launch and health observation
- training runtime process launch
- provider/cloud server process separation
- backend probe commands

Done when:

- command construction is testable without launching when possible
- cloud provider paths remain separate from local backend capability checks
- Python lazy import guards remain as runtime safety

### 6. Capability State Refresh

Move/add:

- Python import/backend probes after explicit profile install
- stale-state handling

Done when:

- print-plan style diagnostics are non-mutating
- explicit refresh writes/updates the existing capability state cache
- doctor/status can render missing, stale, ready, blocked, and unsupported
  states
- no heavy dependencies are installed during lightweight probes

### 7. Backend-Gated Workflows

Move gates into feature workflows:

- local server start/run backend checks
- training launch backend checks
- future embedding/rerank backend checks
- CPU vs GPU readiness decisions
- actionable next-step errors

Done when:

- local backend work fails before launch when cached capability state already
  knows it is unavailable
- cloud provider server behavior is unaffected

### 8. Application Workflows

Move orchestration into kernel feature packages:

- model import/pull/add/remove/list/inspect
- adapter import/pull/add/remove/list/inspect
- dataset import/export/diff/synth/eval/list/inspect
- server create/run/start/stop/list/inspect/logs
- daemon status/start/stop/doctor/logs
- session create/list/inspect/chat/messages/compact/delete
- runtime bootstrap/init/status/doctor
- training plan create/list/inspect/run/logs/metrics

Done when:

- CLI and HTTP call the same boundary where they share behavior
- package workflows own cross-domain loading and validation
- CLI/HTTP/TUI do not manually orchestrate store reads and writes

### 9. CLI And HTTP Cutover

Move call sites:

- CLI commands call kernel packages
- HTTP routes call kernel packages
- rendering stays in CLI/TUI
- request/response DTOs stay in HTTP layer
- old core service managers become compatibility wrappers until removed

Done when:

- duplicate orchestration is removed from CLI/HTTP
- route behavior and error shapes stay stable
- regression tests pass for both CLI and HTTP surfaces

### 10. Final Crate Rename

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

- Runtime capability planning should use this kernel track and the capability
  state cache vocabulary.
- Linux optional expansion should wait for runtime layout, capability state,
  runtime adapter, and backend-gated workflow bundles before advertising
  profile-specific Linux readiness.
- Embedding/rerank local backend work should use kernel readiness gates rather
  than adding endpoint-specific machine detection.
- TUI V2 should render kernel-backed readiness instead of deriving backend
  state in the UI layer.

## Verification Themes

- Layout tests for env override precedence and read-only vs create-capable
  behavior.
- Compatibility tests proving old managers resolve the same paths when
  constructed from kernel layout.
- Store tests with temp runtime homes.
- Capability state tests for missing, stale, ready, blocked, and unsupported
  states.
- CLI/HTTP regression tests after every workflow cutover.
