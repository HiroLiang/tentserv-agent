# Cluster Runtime Ownership And Resource Guard Plan

Status: findings `R1`-`R20` are implemented and pass the local full-suite and
Rust 1.81 verification matrices for GitHub issue `#118` on
`feature/118-cluster-runtime-ownership`. Native Windows CI rerun remains
pending, so parent `#113` closeout is blocked.

Parent plans:

- [cluster-roadmap.md](./cluster-roadmap.md)
- [v1.x-roadmap.md](./v1.x-roadmap.md)

Companion architecture:

- [cluster-runtime-coordination-architecture.md](./cluster-runtime-coordination-architecture.md)

Active remediation:

- [cluster-runtime-ownership-remediation.md](./cluster-runtime-ownership-remediation.md)

Related contracts:

- [resource-blockers.md](../contracts/resource-blockers.md)
- [runtime-ownership.md](../contracts/runtime-ownership.md)
- [cluster.md](../contracts/cluster.md)
- [model-runtime-server.md](../contracts/model-runtime-server.md)
- [server-chat.md](../contracts/server-chat.md)

This document records the approved design and decision register for the final
selected `v1.1.0` cluster slice. Decisions `1`-`41` are confirmed. A July 16,
2026 audit found incomplete lifecycle and transition behavior; its remediation
is implemented and awaits the final verification gates.

## Objective

Add a shared, structured runtime ownership system and a shared, structured
resource guard system. Cluster routes, server runtimes, destructive use cases,
CLI output, and daemon REST adapters should consume these kernel boundaries
instead of implementing resource policy independently.

The slice must make route replacement, cancellation, server stop, runtime
shutdown, stale-process recovery, model mutation, and adapter mutation safe
when local runtime resources are shared.

## Implementation Baseline

The current `#118` change set added the planned subsystem baseline:

- added reusable file-backed resource coordination, focused guard validators,
  typed blockers, and matching reference-creation locks;
- added durable route claims, physical runtime generations, operation records,
  quarantine, PID-plus-token identity, and dry-run/apply reconciliation;
- integrated profile-aware ownership and first-spawner idle policy into the
  shared model runtime supervisor;
- added `drain` and `block` cluster route replacement, hybrid definition
  watching, request leases, 30-second stop drain, and stale-claim recovery;
- centralized managed `adapter_ref` resolution and aligned model, capability,
  adapter, dataset, train-plan, cluster, server, and runtime mutation guards;
- added ownership summaries to inspect and doctor, additive REST blockers, and
  `tentgent runtime reconcile` without adding an ownership REST route;
- preserved file-backed stores, legacy server/cluster TOML defaults, existing
  top-level REST conflict codes, and provider-target execution limits.

Focused Rust tests, all daemon REST tests, all Python runtime tests, and the
full Rust workspace test suite passed after the first remediation. A later
closeout audit found module, injection, CLI projection, watcher verification,
planning-alignment, and unused-operation gaps that the behavior matrix did not
reject. Those changes are implemented and pass the final local matrix. The
remaining external gates are recorded in
[cluster-runtime-ownership-remediation.md](./cluster-runtime-ownership-remediation.md).

## Pre-Implementation Baseline

`#114` defined structured resource operations and blocker vocabulary. `#115`
added stored cluster definitions and static model/capability reference probes.
`#116` added read-only readiness diagnostics. `#117` added the first runnable
cluster server and reused the shared Python model-runtime supervisor.

The approved slice started from these gaps:

- reference checks return feature-specific strings instead of one typed
  blocker model;
- no cross-process claim records which running server route generation still
  targets a managed resource;
- stored bindings do not cover an old target that remains active during route
  replacement;
- server stop has no route-claim retirement phase;
- stale runtime ownership cannot be inspected or reconciled centrally;
- model and adapter guards do not cover operational LoRA plan/run references or
  exact adapter base-model bindings;
- CLI train-plan removal can delete a live run workspace, while REST applies a
  different blanket run-record restriction;
- the public native chat contract accepts `adapter_ref`, but the direct local
  proxy path currently forwards the Python internal adapter payload shape
  instead of resolving the managed adapter through the existing kernel
  compatibility boundary.

The Python runtime already protects loaded model objects with in-process
leases, tracks active tasks, performs idle release, and delays shutdown until
tasks finish. This slice must not duplicate or override that lifecycle in Rust.
Rust claim records protect managed references and cross-process transitions;
they do not pin a Python model in memory.

## System Boundaries

Resource coordination, runtime ownership, and resource guarding are separate
kernel features. The companion architecture is the source of truth for their
module, dependency, supervisor, and platform polling shape.

`runtime_ownership` owns route-generation claims and physical runtime
generation transitions. It must not decide whether a model or adapter
operation is allowed, and a route claim must not control Python model
load/unload or idle policy.

`resource_guard` owns operation policy. It reads stored references, active
owners, and live runtime state through probes. It must not write ownership
state or inspect files from entrypoint code.

The guard is not a separately started validator service and has no elected
leader. CLI, daemon, cluster worker, and direct server entrypoints call the
same kernel use case in their own process. Pure validators can run
concurrently; mutation permits and ownership transitions provide cross-process
serialization only where state can change.

CLI and REST adapters render guard and ownership results. They must not scan
server specs, cluster TOML, ownership records, or runtime metadata directly.

## Structured Domain

The first `ResourceOperation` enum should cover:

- `delete-model`
- `remove-model-capability`
- `replace-model-capabilities`
- `delete-adapter`
- `rebind-adapter`
- `delete-dataset`
- `delete-train-plan`
- `delete-cluster`
- `replace-cluster`
- `delete-server-spec`

Each operation maps to one operation-specific validator file. One validator may
combine multiple probes and return zero or more typed blockers. Adding a new
operation requires an enum variant, validator file, registry entry, and tests.
Adding a new reference source to an existing operation requires a probe and an
update only to the affected validator files.

Do not reserve guard operations for future logging. Cluster route unbinding,
runtime shutdown, and runtime-resource removal are omitted until each has a
real mutation use case, operation-specific policy, lock derivation, adapter
mapping, and behavior tests.

Route claims use a separate `RouteClaimOperation` enum:

- `acquire`
- `retire`
- `release`
- `reconcile`

Physical runtime generations use `RuntimeGenerationOperation` values for
`start`, `ready`, `close`, and `reconcile`. These operations may later
feed an audit log, but they are not resource guard decisions and must not be
added to `ResourceOperation` merely for shared logging.

The first route-generation claim record should contain:

- schema version and owner id;
- claim kind and `active` or `retiring` lifecycle state;
- server ref, cluster ref, route, and definition hash;
- protected target resource identity;
- owning server process identity;
- acquisition and update timestamps.

Runtime resource identity should separate physical identity from execution
context. The current Python daemon key is only model ref plus capability, but a
cluster route can select a runtime profile whose backend/runtime family differs.
Every local caller therefore resolves one `RuntimeExecutionIdentity` containing
model ref, capability, and effective runtime profile id/version. An explicit
profile wins, a known profile is inferred when omitted, and an explicit default
execution identity is used only when no known profile exists. The backend
remains recorded context when it is already implied by the selected profile.

Claims are written once when a server route generation is first used, not once
per request. They remain independently of Python model idle release and are
removed after a clean server-stop or replacement drain. A stop-time drain
timeout leaves the claim stale for reconciliation. Released claims are removed
instead of retained as history.

Physical runtime process state is separate from logical ownership and uses
`starting`, `ready`, and `closing`. This record lets the shared supervisor
protect runtime startup and shutdown even when the caller is a one-shot CLI,
daemon execution, or direct local server rather than a cluster route.

Identifiers have separate meanings and must not be reused interchangeably:

- `operation_id` identifies one guard authorization, mutation, or ownership
  transition and remains in diagnostic state while that transition is active;
- `execution_id` identifies one inference request and is used only for
  in-process request tracking, cancellation, and response cleanup;
- `owner_id` is the stable claim identity of one server route generation and
  must include the resolved target resource identity rather than relying on the
  definition hash alone;
- `runtime_generation_id` identifies one physical runtime process generation.

A steady-state inference request does not write an ownership record and does
not hold a cross-process file lock for its duration. The first request for a
route generation completes the short claim transition before taking an
in-process request lease; later requests take only the in-process lease.

## Validator And Probe Rules

Validators are pure policy over probe results:

- model deletion combines server-spec, cluster-route, adapter-base-binding,
  LoRA plan/run, runtime-owner, and live runtime-process blockers;
- capability removal and replacement filter server, cluster, ownership, and
  adapter-base-binding blockers by capability; an adapter without an explicit
  target capability follows the existing legacy `chat` rule;
- adapter deletion and rebind combine stored server and LoRA resume references
  with the approved active-runtime adapter safety rule;
- dataset deletion adapts the existing train plan/run reference guard into the
  shared blocker model without changing dataset persistence or policy;
- train-plan deletion blocks a verified live run or an unverifiable process
  state, but permits terminal or proven-stale run cleanup through the same
  guarded kernel use case for CLI and REST;
- cluster deletion preserves the current no-cascade server-spec blocker;
- cluster replacement and route unbind apply the approved owner-retirement
  rule instead of silently discarding old ownership;
- runtime shutdown is decided from physical process and Python task state, not
  from route-claim count;
- server-spec deletion remains blocked while its process is running.

All blockers are sorted and deduplicated centrally. Destructive operations
must fail closed when blocker state cannot be read safely.

Mutation use cases should depend on one `ResourceGuardUseCase` boundary rather
than individual probes. Existing feature-specific reference probes may be
adapted behind the guard during migration, but duplicate filesystem scans
should be removed after equivalent typed coverage exists.

Authorization must return a `ResourceMutationPermit` that retains the acquired
resource coordination locks until the caller completes the mutation. A plain
`validate()` call followed by an unlocked delete is not sufficient. Blocked
authorization returns a typed guard rejection containing
`Vec<ResourceBlocker>` and no permit.

The foundation error module must not depend back on the resource guard feature.
Guarded mutation use cases return
`KernelResult<ResourceMutationOutcome<T>>`, where the outcome is either the
applied result or a typed `ResourceGuardRejection`. Infrastructure failures
remain `KernelError` values. CLI and REST adapters map the same typed rejection
without importing feature domain types into foundation or parsing rendered
messages back into structured data. The existing string-backed
`KernelError::ResourceOperationBlocked` remains only as a compatibility bridge
for mutation use cases not yet migrated in this slice.

## Ownership Persistence

The draft uses a dedicated file-backed store below `TENTGENT_HOME/runtime/`.
Records must use atomic temporary-write and rename behavior. Record filenames
must be derived from validated or hashed identifiers and must not contain raw
user paths.

The supported runtime home is a local filesystem. Record replacement uses a
temporary file in the destination directory, file sync, same-directory rename,
and directory sync where crash durability requires it. Network filesystem
locking and rename semantics are outside the supported boundary for this
slice.

Ownership records must not store prompts, request bodies, generated text,
secrets, provider credentials, or adapter/model source paths.

SQLite is not required for `#118`. The ownership port must keep a later storage
migration isolated from validators and entrypoints.

## Read And Write Boundaries

`#118` does not introduce one global settings service. Model, adapter,
dataset, cluster, server-spec, and ownership files remain behind their existing
resource-specific store ports. Mutation use cases acquire a shared coordination
permit and then call the owning store.

The infrastructure layer may share atomic-file and lock-path helpers, but one
store must not read or rewrite another store's files directly. This centralizes
file semantics without coupling all resources to one persistence module.

Focused inspect commands acquire bounded shared locks, read canonical records
into one immutable in-memory result, release the locks, and then render that
result. They do not maintain a second persisted snapshot cache. Doctor remains
non-blocking and reads atomically replaced records without acquiring all
resource locks; its summary may be briefly stale and must report transition or
unknown state instead of making a mutation decision.

## Coordination And Race Safety

Atomic ownership files do not prevent a check-then-mutate race by themselves.
Ownership acquisition and destructive mutation must share a cross-process
resource coordination lock. This is an operating-system advisory lock held on
an open file handle, not a rule based on whether a lock file exists. A process
exit or crash releases the held lock automatically; persistent file contents
are diagnostic metadata only and cannot prove that a resource is locked.

There is no validator startup check, central single-thread validator queue, or
daemon/CLI leader election. Every process opens the canonical lock for the
same resource key and lets the operating system serialize competing holders.
Steady-state inference does not acquire or retain these locks. First-use claim
creation, runtime-generation transitions, reference-producing transitions, and
guarded mutations acquire them only for the short state change. Waiting callers
use non-blocking `try_lock` attempts with both a maximum attempt count and
wall-clock deadline. Failure returns a typed `resource-busy` result. Lock
fairness is not guaranteed and must not be presented as a job scheduler
contract.

Locks are keyed by canonical resource identity, not by validator or source
file. An operation declares the complete key set it may change, and the shared
coordinator acquires that set. Independent operations on disjoint key sets can
run concurrently.

Mutations acquire the complete sorted resource-key set, re-read authoritative
state, return typed blockers or an RAII permit, and persist atomically before
release. Partial lock sets are released before bounded retry. Runtime startup
and closing use generation-matched two-phase transitions so process launch,
inference, streaming, and graceful shutdown never occur while resource locks
are held. The full protocol and infrastructure boundaries are defined in the
companion architecture.

The shared `ModelRuntimeDaemonSupervisor` must participate in this protocol for
every caller. Cluster route claims provide routing attribution and replacement
protection, while the supervisor's physical process record protects startup
and closing transitions for direct server, daemon, one-shot, and cluster
callers. Runtime admission locks the model and capability keys shared and the
physical-runtime key exclusive until the `starting` record is durable. Model,
capability, and adapter mutations acquire the corresponding conflicting keys,
so a destructive mutation cannot pass between runtime lookup and generation
record creation.

Stale or unreadable ownership never disappears silently. Blocked results must
identify the holder kind and safe reference, lifecycle operation and state,
reason, observed time, and structured next actions. Recovery guidance names
the server, daemon, or runtime entrypoint that must finish or stop first. After
that process is confirmed inactive, reconciliation removes only the matching
owner or runtime generation; users must not edit ownership files manually.

## Cluster Runtime Integration

The cluster server should create route claims through a daemon adapter backed
by kernel ownership use cases. Handler-specific code may hold in-memory request
counts, but it must not define blocker or retirement policy.

The proposed route-generation model is:

1. Resolve one route against one immutable definition snapshot.
2. On first use of that server, route, and definition hash, acquire one
   cross-process route claim for the protected target. Do not repeat the file
   write for later requests against the same generation.
3. Count active requests against that route generation in process memory.
4. Let Python independently load, lease, idle-release, and reload the physical
   model resource; the route claim does not keep it warm.
5. Mark the old claim `retiring` when hot reload changes or removes its target.
6. Send new requests only to the new route generation.
7. Release the retired claim after its in-process request count reaches zero.

Each route generation owns an in-process atomic request count exposed through
an RAII `RequestLease`. The lease increments before execution and decrements on
success, error, cancellation, streaming response drop, or task abort. No async
mutex or OS resource lock may remain held across model execution or response
streaming.

Cluster server shutdown should stop accepting new requests, use a bounded drain,
retire its claims, and release records through the ownership use case. Route
claims follow Rust proxy request leases, not Python task state. When the drain
deadline expires, the proxy stops and leaves unresolved claims stale for
reconciliation; the live physical-runtime generation continues protecting any
Python work. Shutdown must not directly unload the Python model or shut down a
shared physical runtime.

Hot reload reconciliation cannot depend only on a later inference request.
The cluster worker runs one cancellable definition watcher that uses a cheap
metadata probe every second, hashes after detected changes, and performs one
strong hash verification every 30 seconds. Requests and health checks retain
their immediate revision check. Platform probes and same-platform strategies
remain isolated behind the watch port defined in the companion architecture.

Route claims and physical runtime lifecycle are deliberately independent. A
route claim protects the stored target across another process applying a new
definition, especially while an old request drains. Python may release a model
or idle-shutdown its process while the claim remains, and later route use may
start a new physical generation without rewriting the claim.

The proposed cluster-level configuration is an enum rather than a boolean:

```toml
route_update_policy = "drain" # or "block"
```

`drain` permits old and new route generations to coexist until old requests
finish. `block` rejects target-changing apply operations while a running server
uses the cluster. The default is `drain` to preserve the hot-reload behavior
introduced in `#117`.

The currently stored policy governs an apply operation. A cluster using
`block` cannot bypass protection by changing the policy and target in the same
apply: it must first apply a policy-only change to `drain`, then apply the
target change. A cluster currently using `drain` may change the target and set
the resulting policy to `block` in one apply; later changes use `block`.

First route use and cluster apply compete through the same cluster and target
resource locks. The claim is persisted before execution begins, so an apply
cannot remove the old static reference between request resolution and claim
creation.

## Adapter Alignment

The public direct and cluster native chat shape uses `adapter_ref`. The Rust
server path should resolve that ref through managed adapter lookup and existing
model/backend compatibility checks, then build the Python internal adapter
record. Public callers must not provide trusted internal adapter paths.

Cluster definitions continue to bind base models only. This slice must not add
fixed `model + adapter` route targets, adapter defaults, or adapter allowlists.

The confirmed adapter mutation rule blocks deletion or rebind of adapters
bound to a model while any live physical runtime for that base model exists.
This intentionally conservative rule closes the race where an already-running
runtime accepts a new dynamic adapter request while another process deletes
the adapter. `#118` does not add request-scoped adapter claims, Python
task-introspection, or long-held execution locks.

## Inspect And Error Surfaces

CLI cluster/server inspect and their existing REST responses should receive an
additive ownership summary from kernel result types. Safe fields include owner
state, server ref, cluster ref, route, model ref, capability, definition hash,
and timestamps.

CLI doctor and REST `/v1/doctor` receive one compact `runtime` category check
for ownership state. Clean state passes; stale or quarantined records warn and
point to `tentgent runtime reconcile`; unreadable or unverifiable state reports
unknown/failure without authorizing repair. Detailed claims and generations
remain in focused cluster/server inspect output.

Raw PID values, local paths, internal lease ids, and full runtime metadata
should remain internal unless a later contract explicitly exposes them.

Existing mutation routes retain their current conflict codes, including
`model_in_use`, `capability_in_use`, `adapter_in_use`, and `cluster_in_use`.
Their REST error bodies gain an additive, deterministically sorted
`blockers` field while preserving existing code, reason, and detail fields.
CLI and REST projections consume the same `Vec<ResourceBlocker>`.

One local maintenance surface is added:

```text
tentgent runtime reconcile [--home <HOME>] [--apply] [--purge-quarantine]
```

It is a dry run by default. `--apply` removes valid records only after process
identity proves them stale. Malformed records are atomically moved out of the
active store into quarantine only when no live Tentgent process under the same
runtime home could own them. The command never deletes model, adapter, dataset,
or cluster content and has no force bypass. No ownership-management REST route
is added.

Every ownership transition briefly acquires a shared runtime-home maintenance
key. `--apply` acquires that key exclusively before process inventory, health,
legacy metadata, and record checks, preventing a new ownership transition from
racing reconciliation. Unverifiable process state fails closed. Normal
inference does not retain this maintenance lock during execution.

`--purge-quarantine` requires `--apply` and removes only records that were
already quarantined before the current command. It uses the same exclusion
checks and never removes canonical managed content. Records newly quarantined by
the current invocation remain available for one later inspection and purge.

## Implementation Slices

1. Add ownership and guard domain types, ports, validator registry, and test
   fakes, including the mutation-permit boundary, without changing mutation
   behavior.
2. Add the file-backed ownership store, atomic writes, deterministic list
   behavior, process identity checks, stale reconciliation, and shared resource
   coordination locks.
3. Integrate physical runtime `starting`, `ready`, and `closing` records with
   the shared model-runtime supervisor for every caller.
4. Convert cluster/server/model/capability/adapter/dataset/train-plan checks to
   typed validators, add the approved operational-reference probes, and align
   CLI/REST train-plan removal while preserving existing conflict codes.
5. Add route-generation ownership to the cluster server and reconcile hot
   reload, cancellation, stop, shutdown, and forced-exit behavior.
6. Align native chat `adapter_ref` resolution and apply the approved adapter
   ownership rule.
7. Add CLI/REST inspect summaries, the dry-run/apply runtime reconciliation
   command, documentation, compatibility tests, and end-to-end shared-runtime
   verification.

## Test Plan

- Domain tests cover operation serialization, owner states, blocker ordering,
  deduplication, and registry coverage for every operation enum variant.
- Store tests cover atomic save, list, retire, release, malformed records,
  stale owner recovery, process identity mismatch, lock ordering, maintenance
  exclusion, acquisition versus mutation races, and closing-generation
  behavior.
- Validator tests cover model deletion, capability mutation, adapter deletion,
  adapter rebind, dataset deletion, train-plan deletion, cluster replacement,
  cluster deletion, and server deletion. They distinguish adapter base bindings
  and LoRA plan inputs from non-blocking provenance. Runtime shutdown is not an
  executable guard operation; the supervisor and ownership lifecycle own it.
- Cluster tests cover first route use, concurrent requests, shared targets,
  hot replacement, route removal, cancellation, stop, and crash recovery.
- Supervisor tests cover one-shot, direct server, daemon, and cluster callers
  converging on one physical runtime generation while racing with mutation or
  shutdown of the same physical runtime resource.
- Shared-policy tests cover different caller `idle_seconds` values, physical
  idle shutdown independent of route claims, claim reconciliation after server
  exit, and later runtime restart.
- Adapter tests prove `adapter_ref` is resolved through managed metadata and
  internal paths cannot be supplied by the public request.
- Train tests prove model/dataset/resume-adapter reference creation cannot race
  deletion, concurrent run starts preserve the existing single-run rule, live
  or unverifiable runs block plan removal, and terminal or proven-stale runs can
  be removed consistently through CLI and REST.
- Capability tests prove an adapter's explicit target capability, or legacy
  `chat` default, blocks removal from its bound model.
- CLI and REST tests cover ownership summaries, compact doctor ownership checks,
  and existing conflict codes.
- Runtime reconcile tests cover dry run, stale removal, malformed quarantine,
  prior-quarantine purge, live-process blockers, transition exclusion, and
  refusal to delete managed resource content.
- Python tests confirm graceful shutdown does not release active model tasks.

Primary commands:

```bash
cargo test -p tentgent-kernel resource_guard
cargo test -p tentgent-kernel runtime_ownership
cargo test -p tentgent-kernel cluster
cargo test -p tentgent-kernel server
cargo test -p tentgent-kernel adapter
cargo test -p tentgent-kernel train
cargo test -p tentgent-cli cluster
cargo test -p tentgent-cli server
cargo test -p tentgent-cli train
cargo test -p tentgent-daemon server::cluster
cargo test -p tentgent-daemon transport::rest::tests
uv run --project python/tentgent-model-runtime pytest
cargo test --workspace
git diff --check
```

## Dependency Audit Boundary

The final pre-implementation audit distinguishes operational dependencies from
provenance. The guard must protect references whose removal would make a stored
definition, repeatable workflow, or active process invalid. Historical evidence
may remain stale when its source resource is removed and must not become a
permanent deletion blocker.

The following existing references are provenance and do not require a new
`#118` blocker:

- session `default_server_ref`, session `adapter_ref`, and message or summary
  provenance refs; the session contract already permits these fields to become
  stale after the referenced resource is removed;
- adapter `training_dataset_ref`, `training_run_ref`, and
  `training_config_ref`; imported adapter content remains usable without its
  training-source records;
- a completed train run's output `adapter_ref`; it records the result that was
  produced and does not make the adapter a prerequisite for reading the run;
- capability proof records; model deletion removes them with the canonical
  model directory, while capability resolution treats missing capability
  metadata as unsupported before proof evidence can grant support;
- generic job target and artifact strings, which are workflow descriptions and
  are not canonical managed-resource bindings;
- runtime profiles, which are compiled selections in the current product and
  have no independent delete mutation to guard.

Decisions `27` and `28` confirm the operational side of this boundary. LoRA
plan/run model and dataset refs, plan `resume_adapter_ref`, and exact adapter
`base_model_ref` are protected dependencies. Train-plan removal is a guarded
operation that blocks live or unverifiable runs and permits terminal or
proven-stale cleanup consistently through CLI and REST.

Managed LoRA plans do not currently persist a Tentgent `ModelCapability` value;
they select training support from model format, backend, and platform. They are
therefore model-level blockers in `#118`, not capability-mutation blockers.
Adding a tuple-aware LoRA capability gate remains separate later work. Adapter
bindings do have an effective target capability: an explicit value wins and an
omitted value means legacy `chat`, so removing that capability from the bound
model is blocked.

## Decision Register

Decision numbers are stable. Resolved rows remain in this table, new questions
receive a new number, and every later design review must repeat every unresolved
number until the user confirms it. A discussion batch may contain at most five
decisions. Each presented decision must include the complete question, the
recommended choice, and the concrete risks; the table below is the persistent
index rather than a substitute for that explanation.

| No. | Status | Decision | Recommended Draft | Alternative / Cost |
| --- | --- | --- | --- | --- |
| 1 | confirmed | Route-claim lifetime | Write one route-generation claim on first use, retain it until a clean server stop or route replacement drain completes, leave timed-out claims stale, and never write it per request or tie it to Python model warmth. | Request-only records create frequent writes and protection gaps; server-long claims require stale-process reconciliation. |
| 2 | confirmed | Runtime resource key | Include model, capability, and effective runtime profile id/version; infer a known profile when omitted and use an explicit default identity only when no known profile exists. | Keeping the current model-plus-capability key can merge incompatible runtime families. Changing it requires supervisor metadata compatibility and migration tests. |
| 3 | confirmed | Route replacement | Add cluster-level `route_update_policy` with default `drain` and optional `block`; the currently stored policy governs each apply. | Drain may temporarily occupy two targets; block requires a policy-only transition or server downtime before target replacement. |
| 4 | confirmed | Python lifecycle separation | Route claims never load, pin, unload, or shut down Python model resources. Python leases, active tasks, idle release, and supervisor process state remain authoritative. | Physical runtimes may remain until existing idle timeout; coupling shutdown to route claims could interrupt shared callers. |
| 5 | confirmed | Stop behavior | Stop accepting new requests, use a bounded Rust request-lease drain, and stop the proxy without killing shared Python runtime work. Timed-out claims remain stale until reconciliation. | Unbounded drain can hang stop; coupling claims to Python tasks violates lifecycle separation. |
| 6 | confirmed | Stale recovery | Match server metadata, process identity, runtime generation, and health; explain the blocker and required stop-before-reconcile action. Malformed state fails closed. | PID-only recovery is unsafe under PID reuse; silent cleanup can remove a live owner. |
| 7 | confirmed | Adapter safety | Block adapter delete/rebind while any live physical runtime exists for its base model; do not add per-request adapter files or long-held locks in `#118`. | This safely closes request/delete races but can temporarily overblock unrelated adapters until the runtime exits. |
| 8 | confirmed | Ownership visibility | Add safe claim and runtime-generation summaries to existing cluster/server inspect CLI and REST responses without raw PID or paths. | Internal-only state makes blocker diagnosis difficult; excessive detail can expose host internals and clutter normal output. |
| 9 | confirmed | REST blockers | Preserve current code/reason and add an additive structured `blockers` field using the shared blocker contract. | Internal-only blockers limit automation; additive fields require stable serialization and compatibility tests. |
| 10 | confirmed | Persistence | Use dedicated file-backed claim/runtime stores behind ports for `#118`; keep SQLite as a later replaceable adapter. | SQLite adds migration and transaction scope now; files require explicit lock ordering and reconciliation. |
| 11 | confirmed | Runtime profile mutation | Record profile identity in keys and diagnostics but add no runtime-profile CRUD or mutation operation. | Profile CRUD expands `#118`; version changes must create a distinct runtime identity and stale old records must reconcile. |
| 12 | confirmed | Resource coordination | Use canonical per-resource cross-process OS advisory locks through one coordinator; do not start a validator service or elect daemon/CLI leadership. | A central queue requires IPC, startup election, takeover, and split-brain handling. Atomic records alone remain racy. |
| 13 | confirmed | Lock wait policy | Use bounded non-blocking attempts, release partial lock sets before retry, and return typed `resource-busy` with holder guidance. Never force-unlock a live holder. | Force-unlock permits concurrent mutation; a durable FIFO queue is a separate scheduler subsystem. |
| 14 | confirmed | Reload reconciliation | Run one cancellable hybrid metadata/hash poller per cluster worker, isolate platform probes in separate adapters, and keep request/health checks as secondary triggers. | Request-only reload can retain retiring claims indefinitely; polling frequency must be measured per supported platform. |
| 15 | confirmed | Closing-runtime acquisition | Treat `closing` as a generation barrier, wait for a bounded interval, and return retry guidance on timeout. Start a later generation only after prior termination is confirmed. | Reusing a closing endpoint can reject work; overlapping generations can duplicate resource use. |
| 16 | confirmed | Shared caller coverage | Put physical generation coordination in `ModelRuntimeDaemonSupervisor` so one-shot CLI, daemon REST, direct server, and cluster callers use the same records and locks. | Cluster-only coordination leaves duplicate-start and shutdown races in other entrypoints; shared changes increase regression scope. |
| 17 | confirmed | Shared idle policy | Preserve the first spawner's effective idle policy until that physical generation exits; later callers reuse it and receive a visible mismatch flag. | Order-dependent policy can surprise callers; renegotiation or keying by policy can duplicate runtimes or require a new runtime control API. |
| 18 | confirmed | Dataset guard migration | Adapt the existing train-plan/run reference probe into the shared guard registry while preserving current dataset behavior and errors. | Leaving parallel guards weakens the architecture; migration adds tests but no new dataset policy. |
| 19 | confirmed | Force behavior | Do not let `--force` bypass active claims, live runtime blockers, lock timeouts, or unreadable guard state; reserve force for existing input-location warnings. | A bypass can corrupt active work; fail-closed recovery may require stopping the named process and reconciliation. |
| 20 | confirmed | External filesystem changes | Do not monitor or prevent manual changes. Diagnose damaged state and provide dry-run/apply runtime reconciliation that removes proven-stale records, quarantines malformed records after live-process exclusion, and explicitly purges prior quarantine records. | External tools can still corrupt managed state; recovery must not bypass active ownership or delete canonical resource content. |
| 21 | confirmed | Diagnostic read consistency | Keep doctor non-blocking over canonical atomic records; focused inspect reads an in-memory snapshot under bounded shared locks. Do not add a persisted snapshot cache. | Doctor may be briefly stale; locking every doctor probe or maintaining a second cache adds latency and synchronization failure modes. |
| 22 | confirmed | Stop-time claim boundary | Bind route claims to Rust proxy request leases, not Python tasks. After a bounded drain timeout, stop the proxy, leave unresolved claims stale, and let live physical-runtime records continue protecting Python work until reconciliation can prove the proxy dead. | Keeping the proxy alive preserves in-memory counts but can make stop unbounded. Waiting for Python tasks requires cross-process task leases and conflicts with the confirmed lifecycle separation. |
| 23 | confirmed | Reconcile exclusion protocol | Add one runtime-home maintenance coordination key: ownership transitions take it shared for a short period, while `runtime reconcile --apply` takes it exclusive before process inventory, health checks, and quarantine. Fail closed when process identity cannot be verified. | Process scans without a gate race new starts. A lifetime-wide global lock would serialize normal execution and violate the short-lock rule. |
| 24 | confirmed | Structured blocker transport | Return an explicit typed mutation outcome from guarded use cases, carrying either the applied result or `ResourceGuardRejection`; keep infrastructure failures in `KernelError` and map the typed rejection in CLI/REST adapters. | A neutral blocker DTO in foundation weakens dependency boundaries. Typed outcomes require coordinated signature changes across every migrated mutation use case. |
| 25 | confirmed | Effective runtime profile across callers | Resolve one `RuntimeExecutionIdentity` for every local caller. Explicit profiles win, known profiles are inferred when omitted, and the default identity is used only when no profile exists, so one-shot, daemon, direct-server, and cluster calls share the same physical key. | Leaving one-shot calls unprofiled creates duplicate generations and conflicts with shared caller coverage. Inference changes existing proof/profile context and requires legacy metadata reconciliation. |
| 26 | confirmed | Quarantine disposal | Keep `--apply` as active-state repair and add an explicit purge mode for previously quarantined ownership records after the same exclusion checks; never purge canonical managed content. | Permanent quarantine has no supported removal path. Immediate deletion loses recovery evidence and makes accidental cleanup harder to audit. |
| 27 | confirmed | Operational dependency versus provenance policy | Treat adapter `base_model_ref` plus its explicit or legacy `chat` target capability, LoRA plan/run `model_ref`, existing LoRA plan/run `dataset_ref`, and plan `resume_adapter_ref` as hard dependencies. Block the corresponding model, model capability, or adapter deletion/rebind until the user removes or replaces the dependent plan, or deletes/rebinds the dependent adapter. Keep session refs, adapter training-source refs, and completed-run output adapter refs as non-blocking provenance. | This adds train and adapter probes plus coordination coverage, and users must explicitly remove dependent plans or rebind adapters before cleanup. Treating all refs as blockers makes historical records impossible to retain; treating operational refs as provenance permits saved workflows and adapter bindings to break silently. |
| 28 | confirmed | LoRA train-plan removal safety | Add `delete-train-plan` to `ResourceOperation`. Block removal while a run for that plan has a verified live process or process state cannot be verified safely. Otherwise let both CLI and REST remove the plan and its run records through the same guarded kernel use case. | This closes the live-write race and removes entrypoint-specific policy, but it broadens `#118` and changes the experimental REST API from blocking every plan with runs to allowing terminal or proven-stale run cleanup. Preserving the current split leaves CLI able to delete a live workspace and keeps deletion policy outside the shared guard. |
| 29 | confirmed | Post-spawn runtime ownership | Persist planned and spawned child identity, keep an RAII cleanup guard until ready, and retain diagnosable ownership whenever termination cannot be verified. | Unconditional record removal can orphan a worker; PID-only or port-only cleanup can affect an unrelated process. |
| 30 | confirmed | Streaming request drain | Make the HTTP body own the lease, close admission on stop, and continue polling Axum through one bounded graceful drain. | Response extensions may drop after headers; dropping the server future freezes active work; unbounded shutdown can hang stop. |
| 31 | confirmed | Stable resource transitions | Re-derive operation and dependency fingerprint after locking for mutations and LoRA creation; release all permits and retry when the complete key set changed. | Stale preflight data can authorize a wrong mutation or persist a workflow after its dependency changed; in-place lock upgrades can deadlock. |
| 32 | confirmed | Cross-platform state coordination | Share one platform-aware atomic replacement adapter, make provisional held locks clean metadata through RAII, and use per-operation bounded jitter. | Delete-then-rename creates a visibility gap; plain rename fails the Windows update contract; partial lock attempts can leak diagnostics and synchronize contenders. |
| 33 | confirmed | Focused ownership inspection | Add kernel-scoped claim/generation projections for cluster and server inspect while keeping doctor global and compact. | Entrypoint filtering duplicates policy; global counts misrepresent the inspected resource. |
| 34 | confirmed | Validator file isolation | Keep one focused validator file per `ResourceOperation`, with shared evidence scans behind probe modules and dispatch outside composition files. | Resource-family validator files make operation growth less isolated; per-operation files require disciplined probe reuse to avoid duplicate scans. |
| 35 | confirmed | Guard interface injection | Make mutation use cases depend on `ResourceGuardUseCase`; keep standard file-backed composition in default constructors and explicit injection for tests and later adapters. | Constructor wiring grows, but retaining concrete guards prevents isolated tests and later adapter replacement. |
| 36 | confirmed | CLI blocker projection | Preserve typed mutation outcomes through CLI adapters and render concise stable codes, blocker summaries, and next actions from the shared vector. | CLI error text changes and needs render tests; flattening loses structure and diverges from REST. |
| 37 | confirmed | Watcher verification | Inject watcher probe, schedule, and test clock boundaries; add deterministic lifecycle tests and non-gating platform cost evidence. | Hard timing thresholds are flaky; hard-coded dependencies leave cancellation and reload behavior under-tested. |
| 38 | confirmed | Tracking state | Keep `#118` open as `In Progress` and `P1`, link all active records, and leave parent `#113` incomplete until closeout gates pass. | Stale tracking makes later sessions and release planning treat incomplete work as finished. |
| 39 | confirmed | Interface injection scope | Inject guard, coordination, ownership-store, process-probe, and health-probe ports at every new `#118` orchestration boundary; default constructors compose file-backed adapters and leaf helpers remain concrete port implementations. | This increases constructor wiring, but partial injection would leave runtime and ownership behavior difficult to isolate or replace. |
| 40 | confirmed | Typed codes and CLI exits | Use typed kernel enums for guard, blocker, and coordination codes while preserving serialized strings; keep CLI exits `0` for success, `1` for blocked/busy or execution failure, and parser-owned `2` for usage errors. | A code-enum migration touches DTOs and tests; per-blocker numeric exits would add an unnecessary CLI compatibility contract. |
| 41 | confirmed | Executable guard vocabulary | Remove `unbind-cluster-route`, `shutdown-runtime`, and `remove-runtime-resource` until each has a real caller, validator, lock derivation, adapter mapping, and behavior tests. | Placeholder variants imply unsupported policy is complete; adding them later requires an intentional enum and contract change. |

## Non-Goals

- no general CPU, GPU, memory, or request scheduler;
- no broad SQLite migration;
- no provider target execution or automatic route fallback;
- no fixed adapter cluster targets;
- no audit history or ownership event log;
- no force-delete bypass for active ownership;
- no migration of unrelated dataset, job, proof, or session stores.
