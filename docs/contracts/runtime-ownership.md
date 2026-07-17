# Runtime Ownership

This contract defines durable attribution and recovery for local model runtime
use. It complements Python's in-process model leases; it does not replace them.

## Responsibilities

The Rust ownership layer records:

- which cluster server route generation selected a target;
- which physical runtime generation is starting, ready, or closing;
- which short ownership transition is in progress;
- whether persisted state is stale, malformed, or quarantined.

Python remains authoritative for loaded-model leases, active runtime tasks,
idle release, and graceful shutdown. A route claim protects configuration and
mutation boundaries but does not keep a model loaded.

## Runtime Identity

`RuntimeExecutionIdentity` has two forms:

- model-bound: model ref, capability, effective runtime profile id, and profile
  version;
- unbound: capability and effective profile for generic workers such as LoRA
  tuning.

Known profiles are inferred before identity is created. If no profile exists,
the identity uses the named `default@1` profile. A profile version change is a
different physical runtime identity.

## Persistence

Records are stored as atomically replaced TOML below:

```text
TENTGENT_HOME/runtime/ownership/
|-- claims/
|-- generations/
|-- operations/
`-- quarantine/
```

Writes use one shared filesystem boundary: a temporary file in the destination
directory, file sync, same-directory replacement, and best-effort directory
sync. Unix uses rename replacement. Windows uses
`MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)` through the isolated safe
platform filesystem adapter. Delete-then-rename is never used. Filenames are
derived from validated or hashed identities. Records must not contain prompts,
generated text, credentials, request bodies, or source paths.

The supported store is a local filesystem. SQLite migration remains isolated
behind the ownership ports; network filesystem locking and rename behavior are
unsupported.

Ownership orchestration is split into object-safe ports for route claims,
runtime generations, inspection, and reconciliation. The standard use case
composes injected coordination, ownership-store, process, health, and server
identity ports. Model-runtime supervision depends only on runtime-generation
ownership; cluster route management depends only on route-claim ownership.
Operation RAII cleanup retains the same injected store through `Drop` and does
not construct a file store behind the caller's boundary.

## Route Claims

A cluster worker creates one durable claim on first use of each unique:

```text
server + cluster + route + definition hash + resolved target
```

Later requests reuse the durable claim and take only an in-process RAII request
lease. The lease is released on success, error, cancellation, task abort, and
stream drop.

When a definition changes, new requests use the new route generation. The old
claim becomes `retiring` and is removed after its active request count reaches
zero. Route claims do not cause Python model loading or shutdown.

On cluster stop, admission closes and request leases drain for up to 30
seconds. If the deadline expires, the proxy exits and leaves unresolved claims
for reconciliation. It must not terminate shared Python work.

Claim creation holds shared locks for every referenced resource. Retiring or
releasing an existing claim holds only the maintenance shared lock and the
claim's exclusive key because it removes a reference rather than creating one.
This lets a worker release claims while `server stop` holds the server
transition lock and waits for graceful exit.

The route manager removes its in-process generation entry only after durable
claim release succeeds. A `resource-busy` release keeps the entry available for
drain retry or later stop/reconciliation; it must not silently discard the
retry owner while the durable claim remains.

The HTTP response body owns the request lease. Headers, data frames, body
errors, and trailers pass through unchanged, while EOF, error, cancellation,
task abort, or body drop releases the lease exactly once. During server stop,
Axum continues polling accepted connections while admission is closed and the
bounded lease drain is active.

## Physical Generations

Physical runtime generations move through:

- `starting`;
- `ready`;
- `closing`.

Generation transitions use the shared resource coordinator and never hold an
OS lock across process launch, model execution, or graceful shutdown. A closing
generation has a 5-second barrier; a replacement cannot start until termination
is verified.

Each spawned worker receives an opaque process-instance token and returns it in
internal health state. A PID without the matching token cannot prove ownership,
which prevents PID reuse from validating the wrong process.

A `starting` record may also contain optional `launch_target` and `endpoint`
evidence. Launch preparation stores host and port before spawn; successful
spawn attaches PID, endpoint, and process token before metadata and health
checks. These are internal launch sub-transitions and do not add public
lifecycle states. A sanitized optional `diagnostic` explains a launch or
shutdown state that could not be verified.

Every post-spawn failure terminates the Unix process group or Windows child and
waits for exit before removing its generation. If exit cannot be verified, the
generation remains `starting` or becomes `closing` with a diagnostic. A later
caller or `runtime reconcile --apply` may adopt a worker only when endpoint,
PID, token, capability, model, and effective profile identity match. Legacy
daemon metadata remains recovery evidence; port-only or PID-only adoption is
not permitted.

The first spawner chooses the idle policy for a generation. Later callers reuse
that generation and receive an idle-policy mismatch diagnostic when their
requested policy differs.

## Cluster Reload Policy

Cluster definitions accept:

```toml
route_update_policy = "drain" # or "block"
```

Missing legacy values default to `drain`.

- `drain` allows old and new route generations to coexist while old requests
  finish;
- `block` rejects an apply that changes route targets while the stored policy
  remains in force.

The currently stored policy governs apply. Moving from `block` to `drain` must
be a policy-only apply. Moving from `drain` to `block` may accompany a target
change.

Each cluster worker checks definition metadata every second, hashes after a
detected change, and performs a forced hash every 30 seconds. Request and health
paths also check revision state. An invalid reload returns
`cluster_definition_reload_failed`; stale configuration is never used
silently.

The watcher runner receives an injected revision probe, route observer,
schedule, and tick/clock source. Deterministic tests cover unchanged polls,
forced hashing, cancellation, reload failure, later recovery, and finite
shutdown without real sleeps. The production composition uses the platform
probe and Tokio timer.

## Inspection

Cluster and server inspect surfaces expose a safe scoped ownership view. REST
keeps the existing summary fields and additively returns `scope`, `claims`,
`generations`, and `issues`. Cluster scope includes claims for that cluster and
only their referenced generations. Cluster-server scope filters by server ref;
direct local server scope uses its effective runtime identity; cloud server
scope is empty.

Safe output includes counts, lifecycle state, server/cluster refs, route,
model ref, capability, effective profile, definition hash, timestamps, and
sanitized diagnostics. PID, process token, owner id, runtime key, generation
id, local paths, raw issue record ids, and internal lease ids remain private.
Active operation records persist optional resource keys so scoped counts can
exclude unrelated transitions. Legacy operation records without keys remain
readable and are excluded from non-global operation counts.

CLI doctor and `GET /v1/doctor` include a compact `runtime-ownership` check.
Focused inspect remains the place for claim and generation detail. Doctor does
not repair state.

There is no ownership-management REST route.

## Reconciliation

```bash
tentgent runtime reconcile [--home <HOME>] [--apply] [--purge-quarantine]
```

The command is a dry run unless `--apply` is present.

`--apply` takes the runtime-home maintenance key exclusively and repairs only
records proven stale by process identity and health evidence. Unverifiable
state fails closed. Valid stale claims, generations, and operations may be
removed. Malformed records move to quarantine only after live-process
exclusion can be proven.

A live but unverifiable PID, process token, runtime identity, or health
mismatch remains a diagnostic ownership issue. It is not treated as a
malformed record and is never quarantined through the malformed-record path.

`--purge-quarantine` requires `--apply`. It removes only records that were
already quarantined before the current invocation. Newly quarantined records
remain available for one later inspection and purge.

Reconciliation never deletes managed models, adapters, datasets, clusters, or
server specs. No force option bypasses active ownership, unreadable state, or
coordination contention.
