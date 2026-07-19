# Resource Coordination And Blockers

This contract defines how Tentgent coordinates short resource transitions and
rejects mutations that would break stored or active references.

## Boundary

The kernel implementation is split into three independent features:

```text
features/
|-- resource_coordination/  # cross-process advisory locks
|-- resource_guard/         # operation-specific blocker policy
`-- runtime_ownership/      # durable route claims and runtime generations
```

`resource_coordination` does not know mutation policy. `resource_guard` reads
references through focused probes and returns structured blockers. Its
coordinator, ownership store, and process probes are injected ports; standard
constructors compose the file-backed implementations. Runtime ownership is one
input to those validators and is specified separately in
[runtime-ownership.md](./runtime-ownership.md).

CLI and REST handlers must call the owning kernel use case. They must not scan
resource files or recreate guard policy themselves.

## Coordination Protocol

Transitions use operating-system advisory locks through `fs2`. Lock files are
stored below:

```text
TENTGENT_HOME/locks/resource-coordination/
```

Resource identities are canonicalized and hashed before becoming filenames.
Raw user paths and secrets must not appear in lock filenames or holder
metadata.

An operation declares all required resource keys and whether each key is
shared or exclusive. The coordinator:

1. sorts and deduplicates the keys;
2. acquires them in canonical order;
3. releases a partial set before retrying;
4. retries for at most 2 seconds and 40 attempts by default, with bounded
   jitter;
5. returns an RAII permit or a typed `resource-busy` result.

Locks protect only lookup, authoritative re-read, validation, and atomic state
transition. They must never remain held during model execution, streaming,
provider calls, or graceful process shutdown. Disjoint resource-key sets may
proceed concurrently.

The open advisory lock is authoritative. Per-holder metadata is diagnostic
only and may be stale after a crash. Process exit releases the advisory lock.
Network filesystem lock and rename semantics are unsupported.

Provisional locks own both the OS lock and diagnostic holder metadata through
RAII. Failure while acquiring a later key or writing holder metadata releases
the entire partial set without leaving a holder record. Retry jitter is
derived from the operation id, process id, and attempt number and remains
inside the configured bound.

## Resource Operations

`ResourceOperation` is the complete operation vocabulary used by the guard
registry:

| Code | Protected change |
| --- | --- |
| `delete-model` | Delete a managed model. |
| `remove-model-capability` | Remove one capability from a model. |
| `replace-model-capabilities` | Replace a model's capability set. |
| `delete-adapter` | Delete a managed adapter. |
| `rebind-adapter` | Change an adapter's base-model binding. |
| `delete-dataset` | Delete a managed dataset. |
| `delete-train-plan` | Delete a LoRA plan and removable run records. |
| `delete-cluster` | Delete a cluster definition. |
| `replace-cluster` | Replace a cluster definition. |
| `delete-server-spec` | Delete a stored server spec. |

Adding another destructive or reference-changing operation requires a new enum
value and a focused validator. It must not be represented by an untyped string
passed from an entrypoint.

## Mutation Result

A guarded mutation returns `ResourceMutationOutcome<T>`:

- `Applied(T)`: validation passed and the mutation completed while its permit
  was held;
- `Blocked(ResourceGuardRejection)`: authoritative references make the change
  unsafe;
- `Busy(ResourceBusy)`: the transition keys could not be acquired within the
  bounded retry limit.

Reference-changing mutations use a three-attempt stabilization protocol. Each
attempt derives a typed dependency token and complete key set, acquires the
permit, then re-reads and re-derives authoritative state while that permit is
held. A changed operation or dependency token releases the whole permit and
retries after 25-75 ms bounded jitter; locks are never upgraded in place.
Exhaustion returns retryable `resource-state-unstable` guidance. REST maps this
code to HTTP `409`.

Persistence or probe failures remain `KernelError`. Unreadable authoritative
state fails closed; it is not converted into an empty blocker list.

There is no force option that bypasses an active owner, unreadable state, or
lock contention.

## Structured Blockers

Every blocker contains:

| Field | Meaning |
| --- | --- |
| `kind` | Reference family, such as `server-spec`, `cluster-route`, `train-run`, `runtime-generation`, or `cluster-policy`. |
| `code` | Stable machine-readable reason. |
| `reference` | Safe user-facing reference for the blocking object. |
| `reason` | Short operator-facing explanation. |
| `resource_kind` | Optional protected resource family. |
| `resource_ref` | Optional protected resource reference. |
| `operation` | Optional `ResourceOperation` code. |
| `capability` | Optional affected capability. |
| `route` | Optional cluster route. |
| `field` | Optional stored field that created the reference. |
| `owner` | Optional grouping owner, such as a cluster or server ref. |
| `next_actions` | Ordered safe recovery actions. |

Blockers are sorted by kind, owner, reference, route, field, and capability,
then deduplicated. CLI and REST projections consume the same ordered vector.

REST mutation conflicts preserve their existing top-level compatibility codes,
including `model_in_use`, `capability_in_use`, `adapter_in_use`,
`dataset_in_use`, `train_plan_in_use`, `cluster_in_use`, `server_in_use`, and
the coordination codes `resource-busy` and `resource-state-unstable`. The
structured `blockers` array is additive. Kernel guard, blocker, and
coordination codes are typed enums whose serialized values remain these stable
strings.

Guarded CLI commands consume the typed outcome directly. Blocked output shows
the guard code, operation, protected resource, each blocker
code/kind/ref/reason, and deduplicated ordered next actions. Busy output shows
the coordination code, blocked key, description, and retry delay. Success
exits `0`; blocked, busy, and execution failures exit `1`; command-line usage
errors remain parser-owned exit `2`.

## Current Dependency Rules

| Mutation | Blocking references |
| --- | --- |
| Model deletion | Server specs, cluster routes, adapter base-model bindings, LoRA plans/runs, route claims, and matching live runtime generations. |
| Capability removal/replacement | Server specs, cluster routes, adapter capability bindings, route claims, and matching live runtime generations. Current LoRA records remain model-deletion dependencies because they do not persist a Tentgent capability. The mutation takes the model key exclusively because model capabilities are persisted by rewriting one whole metadata record. |
| Adapter deletion/rebind | Server references, LoRA resume-adapter dependencies, and a live physical runtime for the adapter's base model. |
| Dataset deletion | LoRA plans and runs that use the dataset. |
| Train-plan deletion | A live run or a run whose process state cannot be verified. Terminal and proven-stale runs may be removed with the plan. |
| Cluster replacement | Active route claims, target references created concurrently, and the stored `route_update_policy`. |
| Cluster deletion | Any running or stopped server spec or active route claim that references the cluster. |
| Server deletion | A server process that is still running or cannot be verified as stopped. |

For a stored cluster using `route_update_policy = "block"`, an apply that
changes route targets is rejected. Switching from `block` to `drain` must be a
policy-only apply before a later target-changing apply.

## Reference-Creation Races

Guarding only deletion is insufficient. Operations that create references use
matching shared locks, while destructive operations use conflicting exclusive
locks. This applies to server-spec creation, cluster apply, adapter creation or
binding, and LoRA plan/run creation. Each use case re-reads authoritative state
after acquiring its complete key set.

LoRA plan creation and run start hold shared model, chat capability, dataset,
and optional resume-adapter keys while revalidating existence and adapter
compatibility. Run start additionally holds the training-slot and new run keys
exclusively until its `starting` record is persisted.

The following references are intentionally non-blocking:

- session history references;
- model support proof records;
- adapter training provenance after the adapter has its own managed identity;
- completed-run output adapter references.

These records remain useful evidence but do not own the referenced resource.

## Diagnostics

Busy results report the blocked key, operation id, attempts, elapsed wait,
diagnostic holders, retry delay, and a concise description. Blockers report
what must be stopped, removed, or changed before retrying.

Stale runtime claims or generations are not removed by ordinary mutation.
Inspect the owning cluster/server and use `tentgent runtime reconcile` after the
reported process is no longer active. Recovery details are defined in
[runtime-ownership.md](./runtime-ownership.md).
