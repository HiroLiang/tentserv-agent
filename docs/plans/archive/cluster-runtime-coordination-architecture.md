# Cluster Runtime Coordination Architecture

Status: archived after GitHub issues `#118` and `#113` completed. The lifecycle
and coordination architecture passed local, Rust 1.81, native Windows, and
five-route Cluster smoke verification.

The parent decision register confirms decisions `1`-`28`, including stop-time
claim ownership, reconcile exclusion, typed blocker projection, effective
profile inference, quarantine disposal, operational-reference protection, and
safe train-plan removal.

Parent plan:

- [cluster-runtime-ownership-plan.md](./cluster-runtime-ownership-plan.md)
- [cluster-runtime-ownership-remediation.md](./cluster-runtime-ownership-remediation.md)

Related contracts:

- [resource-blockers.md](../../contracts/resource-blockers.md)
- [runtime-ownership.md](../../contracts/runtime-ownership.md)
- [cluster.md](../../contracts/cluster.md)
- [model-runtime-server.md](../../contracts/model-runtime-server.md)

## Purpose

Define one reusable coordination architecture for resource mutation, cluster
route claims, and physical model-runtime process generations. CLI, daemon REST,
direct local servers, cluster servers, and one-shot inference must use the same
kernel boundaries without depending on one elected process or global singleton.

This architecture does not replace Python model lifecycle management. Python
remains authoritative for loaded model leases, active tasks, idle release, and
graceful runtime shutdown.

## Responsibility Split

The implementation has three separate kernel features:

```text
resource_coordination
  canonical resource keys, advisory lock sets, bounded acquisition, RAII permits

runtime_ownership
  route-generation claims and physical runtime-generation records

resource_guard
  operation-specific policy that converts probes into typed blockers
```

Dependency direction:

```text
entrypoint adapters
  -> feature use cases / model runtime supervisor
  -> runtime_ownership or resource_guard
  -> resource_coordination
  -> file/process infrastructure adapters
```

`resource_coordination` must not import runtime ownership or guard policy.
`runtime_ownership` may use coordination permits. `resource_guard` may probe
ownership and use coordination permits, but validators must not acquire locks
or write records themselves.

## Kernel Module Shape

```text
src/tentgent-kernel/src/features/
|-- resource_coordination/
|   |-- domain.rs
|   |-- ports.rs
|   |-- usecases/
|   |   |-- acquire.rs
|   |   `-- mod.rs
|   `-- infra/
|       |-- file_coordinator.rs
|       |-- holder_metadata.rs
|       |-- layout.rs
|       `-- mod.rs
|-- runtime_ownership/
|   |-- domain.rs
|   |-- ports.rs
|   |-- usecases/
|   |   |-- claims.rs
|   |   |-- generations.rs
|   |   |-- interfaces.rs
|   |   |-- inspect.rs
|   |   |-- operation.rs
|   |   |-- reconcile.rs
|   |   |-- scope.rs
|   |   `-- mod.rs
|   `-- infra/
|       |-- health_probe.rs
|       |-- process_identity.rs
|       |-- store.rs
|       `-- mod.rs
`-- resource_guard/
    |-- domain.rs
    |-- ports.rs
    |-- registry.rs
    |-- stabilization.rs
    |-- validators/
    `-- probes/
```

Every `mod.rs` remains composition-only. Tests should live beside the focused
module or in package-level test files, not inside composition modules.
`tentgent-platform-fs/src/lib.rs` follows the same rule and re-exports the
focused replacement implementation. Every executable `ResourceOperation` has
one validator file; registry dispatch and shared reference scans remain
separate.

## Resource Coordination Boundary

Core domain values:

- `ResourceKind`: model, model capability, adapter, dataset, train plan, train
  run, training slot, cluster, server, route claim, and physical runtime;
- `ResourceKey`: one kind plus canonical validated or hashed identity;
- `ResourceLockMode`: shared or exclusive;
- `ResourceLockRequest`: operation id, sorted keys, deadline, and attempt limit;
- `ResourceBusy`: safe holder metadata and retry guidance;
- `ResourcePermit`: RAII owner of every acquired OS lock handle.
- `RuntimeHomeMaintenanceKey`: shared for short ownership transitions and
  exclusive for applied reconciliation.

The coordinator follows these invariants:

1. Sort and deduplicate keys before acquisition.
2. Use non-blocking advisory lock attempts.
3. Release every partially acquired lock when one key is unavailable.
4. Retry with bounded jitter until attempt or wall-clock limit is reached.
5. Re-read authoritative state only after the full key set is held.
6. Return a permit that remains alive through the guarded mutation.
7. Never force-unlock a live holder.

Pre-lock reads may discover candidate keys but cannot authorize mutation. If
the post-lock read reveals a different affected key set, release the complete
permit and retry with the new canonical set. Never mutate while holding only a
subset of the required locks.

The file coordinator owns lock-path derivation and safe diagnostic holder
metadata. Feature code passes `ResourceKey` values and never constructs lock
paths.

## Lock Participation Matrix

The same logical resource must map to the same key in every transition and
mutation. The first implementation uses this minimum matrix:

| Flow | Lock Set During The State Transition | Required Post-Lock Check |
| --- | --- | --- |
| Runtime generation start, ready, or close | maintenance shared; model shared; model capability shared; physical runtime exclusive | Re-read the exact generation and process health. |
| Route claim acquire, retire, or release | maintenance shared; cluster shared; protected target shared; route claim exclusive | Re-read the cluster revision, target, and matching claim. |
| Cluster apply | cluster exclusive; previous and next target resources shared | Re-read the stored policy, definition, server state, and active claims. |
| Server spec creation or reuse | server exclusive; referenced cluster shared, or referenced model and capability shared; any stored adapter refs shared | Re-read every referenced target and reject a changed or missing resource before persisting the spec. |
| Model delete | model exclusive | Re-read server, cluster, adapter-base-binding, train plan/run, claim, and physical-runtime blockers. |
| Model capability remove or replace | model exclusive while metadata remains one whole-record file | Re-read server, cluster, adapter-base-binding, claim, and physical-runtime blockers for each removed capability before rewriting the complete metadata record. |
| Adapter import or first bind | adapter exclusive; selected base model shared when present | Re-read the selected model and compatibility metadata before persisting the binding. |
| Adapter delete | adapter exclusive; current base model exclusive when present | Re-read server and LoRA resume references plus every live physical runtime for the base model. |
| Adapter rebind | adapter exclusive; current base model exclusive when present; next base model shared | Re-read stored references, model compatibility, and every live physical runtime for the current base model. |
| Dataset delete | dataset exclusive | Re-read train plan and run references. |
| Train plan creation or reuse | train plan exclusive; model and dataset shared; resume adapter shared when present | Re-read all operational dependencies before persisting or reusing the plan. |
| Train run start | training slot exclusive; train plan, model, dataset, and resume adapter shared; new train run exclusive | Re-read the plan, dependencies, and absence of another live run before persisting `starting`. |
| Train plan delete | train plan exclusive; all discovered train runs exclusive | Re-read run process state; block live or unverifiable runs and permit terminal or proven-stale cleanup. |
| Server spec delete | server exclusive | Re-read process identity and running state. |
| Applied runtime reconciliation | maintenance exclusive, then any derivable resource keys in canonical order | Re-run process inventory, health, legacy metadata, and record checks. |

Maintenance is always acquired before ordinary resource keys. Remaining keys
are sorted and deduplicated canonically. A flow that discovers a different key
set after locking releases all permits and retries; it never upgrades a partial
set in place.

The locks protect state transitions, not request execution. Once a durable
`starting`, route-claim, or train-reference record exists, guards use that
record as the blocker and the transition releases its permit. This is what lets
steady-state inference run without cross-process file locks while still closing
startup-versus-delete races.

## Persistence And Read Snapshots

The first implementation is file-backed under the local `TENTGENT_HOME`.
Writes use one kernel filesystem utility with a temporary file in the
destination directory, file sync, same-directory replacement, and best-effort
directory sync. Unix uses rename replacement. Windows delegates the only FFI
to the safe `tentgent-platform-fs` boundary and requests replace-existing plus
write-through behavior. Network filesystem lock and rename behavior is
unsupported in `#118`.

Records never contain prompts, generated text, request bodies, secrets,
credentials, or managed source paths. Each model, adapter, dataset, cluster,
server, claim, and generation store remains responsible for its own files; the
coordination feature provides permits and shared atomic-file helpers rather
than one global settings service.

Focused inspect operations acquire bounded shared locks and render one
in-memory snapshot after releasing them. Doctor uses non-blocking atomic record
reads and may be briefly stale; it never authorizes mutation.

Normal inference does not retain the runtime-home maintenance lock. Ownership
transitions acquire it shared only while reading and atomically changing claim
or generation state. Applied reconciliation acquires it exclusively before
process inventory and record repair, so a new transition cannot begin during
the exclusion check.

## Runtime Ownership Boundary

`RouteGenerationClaim` contains:

- owner id and lifecycle state;
- server ref, cluster ref, route, and definition hash;
- protected target resource key;
- owning server process identity;
- acquired and updated timestamps.

It is written once when a route generation is first used. Request counts stay
in process memory. The claim remains until the Rust proxy request-lease drain
completes and does not pin a Python model or process. A stop-time drain timeout
leaves an unresolved claim stale; the physical-runtime generation independently
protects Python work until reconciliation proves the proxy process dead.

`RuntimeGenerationRecord` contains:

- physical runtime key and generation id;
- `starting`, `ready`, or `closing` state;
- effective launch and idle policy;
- optional prepared launch target, spawned endpoint, and process identity
  stored only in internal records;
- an optional sanitized diagnostic for an unverifiable launch or close;
- operation, start, and update timestamps.

Route claims and runtime generations use separate stores and lifecycle use
cases. Neither record is an audit log.

The reconciliation use case supports the local `tentgent runtime reconcile`
maintenance command. Valid stale records are removed only after process
identity verification. Malformed records are moved atomically to a quarantine
directory only after probes confirm that no live Tentgent process under the
same runtime home can own them. Quarantine removes damaged records from active
coordination without pretending their contents were understood.

The applied command holds the runtime-home maintenance key exclusively while it
checks process inventory, health, legacy metadata, and records. Unverifiable
state fails closed. An explicit purge mode removes only records quarantined
before the current invocation after the same checks; it never deletes canonical
managed content.

## Shared Supervisor Integration

The existing `model_daemon.rs` should be split before adding coordination:

```text
features/runtime/infra/model_daemon/
|-- mod.rs
|-- client.rs
|-- health.rs
|-- launcher.rs
|-- metadata.rs
|-- policy.rs
`-- supervisor.rs
```

`supervisor.rs` remains the facade used by chat, embedding, rerank, media,
direct server, and cluster adapters. It delegates physical transitions to the
runtime-generation use case instead of implementing file locks directly.

Every caller first resolves one `RuntimeExecutionIdentity`. Explicit runtime
profiles win, known profiles are inferred when omitted, and the default identity
is used only where no known profile exists. An ensure flow is:

1. Resolve a physical key from model, capability, and effective profile.
2. Check the process-local healthy endpoint cache.
3. Acquire maintenance shared, model shared, capability shared, and the physical
   runtime key exclusive through one coordination request.
4. Re-read the generation record and stored endpoint health.
5. Reuse `ready`, wait on `starting`, reject/wait on `closing`, or persist a
   new `starting` generation.
6. Release the permit before spawning Python.
7. Reacquire and transition only the matching generation to `ready` or remove
   it after launch failure.

Closing uses the same generation comparison. The supervisor never attaches a
request to a closing endpoint and never starts an overlapping replacement
before old process termination is confirmed.

`RuntimeKernelComponent` continues to own one supervisor instance per Rust
process. Cross-process sharing comes from common file records and OS locks, not
from a process-global Rust object.

## Cluster Definition Watch Boundary

Cluster definition polling is daemon-local infrastructure because it controls
a running cluster worker, not kernel product policy.

```text
src/tentgent-daemon/src/server/cluster/watch/
|-- mod.rs
|-- domain.rs
|-- port.rs
|-- runner.rs
|-- strategy.rs
`-- probes/
    |-- mod.rs
    |-- metadata.rs
    |-- macos.rs
    |-- linux.rs
    `-- windows.rs
```

`DefinitionRevisionProbe`, the route observer, and the combined tick/clock
source are injected ports. `runner.rs` owns cancellation, schedule, and
reconciliation calls. `strategy.rs` combines cheap metadata checks with
periodic strong hash verification. Platform probe files isolate
timestamp, file-id, and replacement-detection differences. Shared metadata and
hash normalization stays in `metadata.rs`.

This shape allows another strategy on one platform, such as native filesystem
events with polling fallback, without changing cluster routing or ownership
logic.

## Polling Cost Policy

Initial internal schedule:

- cheap metadata probe every 1 second;
- SHA-256 only after metadata change;
- forced SHA-256 verification every 30 seconds to catch unchanged-size or
  coarse-timestamp replacements;
- one watcher task per running cluster worker;
- request and health paths retain their current immediate revision check.

The schedule is injected for tests and remains an internal constant in the
first version. Implementation must measure metadata and small-file hash cost on
supported macOS, Linux, and Windows adapters before freezing the defaults. A
platform may use a slower default only when measured behavior justifies it.

Watcher tests use synthetic ticks and fake probes; they do not sleep in real
time.
They verify no hash is performed on every unchanged poll, periodic strong
verification occurs, cancellation stops future probes, and reload failure does
not route through stale configuration.

The July 17, 2026 macOS development measurement observed 10,000 unchanged
metadata probes in about 21 ms (approximately 2,140 ns each). The measurement
is non-gating and does not freeze a cross-platform timing threshold.

## Extension Rules

- Add a new protected resource by adding a resource kind/key builder and only
  the affected guard validators.
- Add a new physical runtime caller by using the shared supervisor; do not add
  another startup lock.
- Add a persistence backend by implementing ownership and coordination ports;
  callers must not depend on file paths.
- Add an OS-specific watch method behind `DefinitionRevisionProbe` in its own
  adapter file.
- Add a same-OS alternative by composing probe strategies, not branching in
  request handlers.
- Keep CLI and REST rendering outside all coordination and ownership modules.

## Verification

- Resource coordination tests cover ordering, partial release, timeout,
  process exit, and multi-resource contention.
- Ownership tests cover claim/generation transitions, stale process identity,
  and exact-generation reconciliation.
- Supervisor tests cover concurrent one-shot, daemon, direct-server, and
  cluster callers for one physical key.
- Polling tests cover platform adapter normalization and hybrid metadata/hash
  strategy behavior.
- Integration tests prove unrelated resource keys proceed concurrently while
  conflicting operations serialize.
- Admission-versus-mutation tests cover runtime start against model delete,
  capability removal, adapter delete/rebind, server/cluster reference creation,
  and train plan/run reference creation against model, dataset, or resume-adapter
  deletion. Train tests also cover concurrent start and plan-delete races.
