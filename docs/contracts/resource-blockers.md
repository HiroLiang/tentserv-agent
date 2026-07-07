# Resource Blockers

This contract defines the shared resource-protection model used before
deleting, rebinding, or mutating Tentgent-managed resources.

## Purpose

Tentgent stores resources that can be referenced by later workflows:

- models
- adapters
- datasets
- server specs
- runtime profile selections stored in server specs or proofs
- future serving target route bindings
- future runtime ownership records

A resource operation must not leave these references broken. Before a use case
deletes or changes a resource, it should run the relevant resource guard
validators and fail with structured blockers when the operation would invalidate
stored or active state.

## Vocabulary

Resource:
The managed object being changed, such as a model, adapter, dataset, server
spec, or future serving target route.

Operation:
The requested change, such as `delete`, `rebind`, `remove-capability`, or
`replace-capabilities`.

Blocker:
A structured explanation of one stored or active reference that prevents the
operation from continuing safely.

Probe:
A narrow reader that finds references from one state family, such as stored
server specs or LoRA train plans. Probes should not decide whether an operation
is allowed; they only report references.

Validator:
The use-case-facing policy object that runs one or more probes and decides
whether a resource operation is blocked. Validators are resource-specific and
operation-specific, but they should all return the same blocker shape.

## Structured Blocker Shape

Resource blockers should be represented as structured data before they are
rendered as CLI text, REST errors, or doctor details.

Required fields:

| Field | Meaning | Example |
| --- | --- | --- |
| `kind` | Type of reference that blocks the operation. | `server-spec` |
| `code` | Stable blocker code for entrypoint-specific rendering. | `capability-in-use` |
| `reference` | Stable user-facing reference for the blocker. | `abc123def456` |
| `reason` | Short operator-facing explanation. | `server spec uses this model for chat` |

Optional fields:

| Field | Meaning | Example |
| --- | --- | --- |
| `resource_kind` | Resource being protected. | `model` |
| `resource_ref` | Protected resource reference. | `<model_ref>` |
| `operation` | Blocked operation. | `remove-capability` |
| `capability` | Capability involved in the reference. | `chat` |
| `route` | Route key involved in a serving target. | `chat` |
| `field` | Stored field that contains the reference. | `model_ref` |
| `owner` | Owning grouped object, if different from `reference`. | `local-assistant` |

Known blocker kinds:

| Kind | Current Status | Meaning |
| --- | --- | --- |
| `server-spec` | current | A stored server spec references the resource. |
| `train-plan` | current | A LoRA train plan references the resource. |
| `train-run` | current | A LoRA train run references the resource. |
| `serving-target` | future | A serving target route references the resource. |
| `runtime-owner` | future | A running route or process owns the resource. |

Initial blocker codes:

| Code | Meaning |
| --- | --- |
| `model-in-use` | A model cannot be deleted because it is referenced. |
| `capability-in-use` | A model capability cannot be removed because it is referenced. |
| `adapter-in-use` | An adapter cannot be deleted because it is referenced. |
| `adapter-rebind-in-use` | An adapter cannot be rebound because it is referenced. |
| `dataset-in-use` | A dataset cannot be deleted because it is referenced. |
| `server-running` | A server spec cannot be deleted because it is running. |
| `runtime-resource-owned` | A runtime resource cannot be removed because an active owner uses it. |

Blockers must be sorted deterministically by `kind`, `owner`, `reference`,
`route`, `field`, and `capability`, then deduplicated.

## System Shape

The implementation should be a guard system, not a single hard-coded
server-reference check.

Recommended kernel shape:

```text
features/resource_guard/
├── domain.rs          # ResourceKind, ResourceOperation, ResourceBlocker
├── ports.rs           # shared probe and validator trait boundaries
├── validators/
│   ├── mod.rs         # validator registry/composition only
│   ├── model.rs       # model delete and capability mutation guards
│   ├── adapter.rs     # adapter delete and rebind guards
│   ├── dataset.rs     # dataset delete guards
│   ├── server.rs      # server spec delete guards
│   └── target.rs      # future serving target and runtime-owner guards
├── infra/
│   ├── server_specs.rs
│   ├── train_refs.rs
│   └── targets.rs     # future filesystem/indexed target probes
└── usecases/          # optional wrappers when CLI/daemon need one boundary
```

The exact module name may change if implementation shows a better local fit,
but the responsibility split should stay the same:

- domain types are pure data
- probes read one state family
- validators execute resource-operation rules
- each resource family owns its guard logic in its own validator file
- adding a future resource blocker should add or update only the relevant
  probe and validator file, plus shared domain values when a new blocker kind
  or operation is introduced
- feature use cases call validators before mutation
- CLI and REST render blockers; they do not inspect files directly

The guard feature should remain independently testable. A model capability
mutation test should not need to exercise adapter binding rules, and an adapter
rebind test should not need to exercise dataset train-plan references.

## Validator Model

Validators should be selected by resource kind and operation.

Initial resource operations:

| Operation | Resource | Meaning |
| --- | --- | --- |
| `delete-model` | model | Remove a managed model and its indexes. |
| `delete-adapter` | adapter | Remove a managed adapter and its indexes. |
| `rebind-adapter` | adapter | Change an adapter's local base-model binding. |
| `remove-model-capability` | model capability metadata | Remove one or more model capabilities. |
| `replace-model-capabilities` | model capability metadata | Replace the full model capability set. |
| `delete-dataset` | dataset | Remove a managed dataset and its indexes. |
| `delete-server-spec` | server spec | Remove a stopped server spec. |
| `delete-serving-target` | future serving target | Remove a stored serving target definition. |
| `unbind-serving-target-route` | future serving target route | Remove one route binding from a target. |
| `remove-runtime-resource` | future runtime resource | Remove or clean up an active-owned runtime resource. |

Example operation request types:

```text
ResourceOperation::DeleteModel { model_ref }
ResourceOperation::DeleteAdapter { adapter_ref }
ResourceOperation::RemoveModelCapabilities { model_ref, capabilities }
ResourceOperation::RebindAdapter { adapter_ref, old_base_model_ref, new_base_model_ref }
ResourceOperation::DeleteDataset { dataset_ref }
```

Example validators:

| Validator | Inputs | Probes | Blocks When |
| --- | --- | --- | --- |
| `ModelDeleteGuard` | `model_ref` | server spec probe, future target probe | Any server spec or target route references the model. |
| `ModelCapabilityMutationGuard` | `model_ref`, removed capabilities | server spec probe, future target probe | A removed capability is used by a stored route. |
| `AdapterDeleteGuard` | `adapter_ref` | server spec probe, future target probe | Any server spec or target route references the adapter. |
| `AdapterRebindGuard` | `adapter_ref`, new base model | server spec probe, future target probe | Existing bindings would become incompatible. |
| `DatasetDeleteGuard` | `dataset_ref` | train plan/run probe | Any train plan or run references the dataset. |
| `ServerDeleteGuard` | `server_ref` | server process probe | The server spec is running. |

Validators should return `Ok(())` when no blockers exist and a typed
`in_use`-style error when blockers exist.

## Current Rules

### Model Delete

Deleting a model must be blocked when stored server specs reference that model.
The blocker should include:

- `kind = "server-spec"`
- `code = "model-in-use"`
- `resource_kind = "model"`
- `resource_ref = <model_ref>`
- `operation = "delete-model"`
- `reference = <server short ref>`
- `capability = <server capability>`
- `field = "model_ref"`

Future serving target route references must use the same blocker shape with
`kind = "serving-target"` and `route = <capability route>`.

### Model Capability Mutation

Manual capability updates must protect existing bindings.

Allowed:

- adding a capability
- removing a capability that no stored binding uses
- replacing the capability set when all currently referenced capabilities remain

Blocked:

- removing `chat` from a model used by a chat server spec
- removing `embedding` from a model used by an embedding server spec
- removing `rerank` from a model used by a rerank server spec
- future: removing a capability used by a serving target route

The operation should report blockers with
`operation = "remove-model-capability"` or
`operation = "replace-model-capabilities"`, `code = "capability-in-use"`,
and the affected `capability`.

### Adapter Delete

Deleting an adapter must be blocked when stored server specs reference it.
Current recognized server-spec fields are:

- `adapter_ref`
- `default_adapter_ref`
- `allowed_adapters`
- `adapter_refs`

The blocker should include `code = "adapter-in-use"` and `field` so users can
understand why the adapter is still in use.

Future serving target adapter bindings must use the same blocker model.

### Adapter Rebind

Rebinding an adapter to a different base model must be blocked when existing
server specs or future target routes reference that adapter. The guard should
not try to prove that the rebind would remain compatible with every existing
route. The safer rule is that referenced adapters are not rebound until the
caller removes or updates the server spec or target route that depends on the
old binding.

The blocker should include the matched adapter field, the referencing server or
target, `code = "adapter-rebind-in-use"`, and enough base-model context to
explain why the adapter must be unreferenced before rebinding.

### Dataset Delete

Deleting a dataset must be blocked when LoRA train plans or train runs
reference that dataset. Dataset blockers should use `code = "dataset-in-use"`.

Dataset guards may continue to live in the dataset feature package until a
shared guard module is introduced, but their blockers should be convertible to
the shared structured blocker shape.

### Server Spec Delete

A stored server spec must not be removed while it is running. The caller should
stop the server first, then delete the stopped spec. Server delete blockers
should use `code = "server-running"`.

Server specs are also blockers for model and adapter operations.

### Store Garbage Collection

`tentgent store gc` is not a resource guard operation. It only removes direct
children of managed staging directories and must not delete canonical model,
adapter, or dataset content.

## Error Mapping

CLI output should render concise grouped blockers, for example:

```text
cannot remove capability `chat` from model <model_ref>; still referenced by:
- server-spec abc123def456 uses this model for chat
```

Daemon REST means Tentgent's HTTP API exposed by `tentgent daemon`, not the CLI
or an external web service. Blocked operations should map to the existing
`409 in_use` class when the route already exposes in-use behavior.

The first implementation may expose only the stable error code and a clear
human-readable reason message. The full structured blocker list should remain
internal unless the specific REST route contract is updated to document a
machine-readable `blockers` field.

## Serving Target Integration

Serving target route bindings must plug into this guard system instead of
adding an independent delete or mutation policy.

The first serving target implementation should provide probes that can answer:

- which target routes reference one model
- which target routes reference one adapter
- which target routes require one capability
- which active target routes own a runtime resource

The validators should then combine existing server-spec blockers with target
blockers and return one sorted blocker list.

Partial targets remain valid. A missing route is not a blocker by itself; a
configured route becomes a blocker when it references a resource that a user is
trying to delete or mutate.

Serving targets do not exist in the current implementation. This contract only
defines the required integration point. When #115 or a later serving target
slice adds target definitions or target route bindings, that implementation
must add target probes to `features/resource_guard/` and must not introduce a
separate target-only deletion or mutation policy.

## Non-Goals

- Do not add force delete in the first guard contract implementation.
- Do not require one global SQLite migration before adding guards.
- Do not make probes responsible for policy decisions.
- Do not make CLI or REST handlers inspect resource files directly.
- Do not block `store gc` staging cleanup on canonical resource blockers.

## Verification Expectations

Guard implementation should be tested at the validator level before entrypoint
tests:

- deleting a referenced model returns structured server-spec blockers
- deleting an unreferenced model succeeds
- removing a referenced model capability is blocked
- adding a model capability is allowed
- deleting a referenced adapter returns the matched server-spec field
- deleting a referenced dataset returns train plan/run blockers
- blocker ordering is stable
- CLI and REST render or map blockers without losing the reason
