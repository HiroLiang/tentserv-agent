# Issue 127 Compatibility Tuple And Proof v2 Sub-Plan

Status: draft issue-level execution plan for
[#127](https://github.com/HiroLiang/tentserv-agent/issues/127).

Parent plan:
[v1.2.0 Local Compatibility State Plan](./v1.2.0-local-compatibility-state-plan.md)

Branch: `feature/127-compatibility-tuple-proof-v2`

This document is the decision register and implementation checklist for
`#127`. It may be revised during issue planning, but it must not widen the
approved `v1.2.0` slice or move `#128`, `#129`, or `#130` behavior into this
branch.

## Outcome

Add the kernel-owned compatibility tuple and file-backed proof v2 foundation
that later execution-gate issues can consume.

When this issue is complete:

- every execution-affecting field has a normalized tuple representation;
- proof v2 records have a versioned schema and deterministic exact-tuple key;
- different tuples coexist and only the same tuple replaces itself;
- callers can query and remove exact proof without clearing unrelated proof;
- v2, tuple-aware v1, and legacy latest-proof records remain readable;
- incomplete older evidence cannot become precise `verified` or `failed`
  evidence for a query that requires its missing fields;
- writes are atomic and coordinated across local processes;
- existing user-facing gates keep their current policy until `#128` and
  `#129` provide complete execution tuples.

## Current Baseline And Gaps

The current `ModelCapabilityProof` and `ModelCapabilityProofKey` cover model,
capability, format, optional MLX family, backend, optional runtime version,
optional runtime profile/version, status, source, time, server, and error.

The current implementation does not yet provide:

- schema-versioned complete proof records;
- quantization, general runtime family/package, platform/device, adapter load
  identity, or normalized input/output shape in proof identity;
- exact removal or typed multi-field queries;
- atomic proof writes;
- one coordination boundary for concurrent list, save, replace, and removal;
- conservative matching for every dimension missing from old records.

The current tuple-aware path and legacy latest path are both active. Direct
`fs::write` is used, and capability clear removes all proof for that
capability.

## Scope Lock

### Included

- Kernel-owned normalized tuple and component types.
- Proof v2 record and exact key types.
- Canonical serialization used only for identity generation.
- Versioned file layout for proof v2.
- Exact get, save/replace, list/filter, and remove store boundaries.
- Backward-compatible capability-wide list and clear behavior.
- Dual reads of proof v2, tuple-aware v1, and legacy latest proof.
- Evidence-origin and completeness metadata needed by the resolver.
- Conservative stale comparison for precise queries.
- Atomic writes and cross-process coordination.
- Contract updates for proof schema, support status, and model-store layout.
- Domain, store, resolver, migration, collision, and concurrency tests.

### Included As Foundation Only

- Optional adapter identity is represented by an opaque `adapter_ref` and
  `load_identity`. Computing LoRA weight and load-configuration identity is
  deferred to `#129`.
- Platform and device fields are represented and normalized. Runtime flows
  begin supplying authoritative values in `#128` and `#129`.
- Input and output shapes are represented and normalized. Later gate issues
  map concrete endpoint requests into these shapes.
- A complete-proof write API accepts an already resolved tuple. This issue
  does not invent missing execution facts for current producers.

### Explicitly Excluded

- Model-bound server or Cluster hard-gate changes (`#128`).
- LoRA adapter identity computation and serving gates (`#129`).
- CLI or daemon REST exact-tuple flags and response changes (`#130`).
- Model, adapter, server, Cluster, or doctor presentation work (`#130`).
- Runtime fallback, candidate selection, or route mutation.
- Persistent secondary indexes, SQLite migration, or a shared registry.
- Automatic destructive migration or guessed backfill of legacy records.
- New runtime families, model conversion, or new verification workflows.
- Release publication, artifacts, signing, or Homebrew changes.

## Proposed Tuple Contract

The kernel tuple should contain:

| Group | Fields | Normalization rule |
| --- | --- | --- |
| Model | `model_ref`, `capability` | Use validated kernel identities and enums. |
| Format | `primary_format`, `quantization` | Use typed format and an explicit quantization state; never infer from a display name. |
| Backend | `backend`, `runtime_family` | Trim and canonicalize registered identifiers; do not use entrypoint route names. |
| Runtime | `runtime_package`, `runtime_version` | Record the selected executable package identity and version facts supplied by the caller. |
| Profile | `runtime_profile`, `runtime_profile_version` | Use the selected profile identity, or an explicit not-applicable state. |
| Environment | `platform`, `device_class` | Use stable execution classes, not raw hardware descriptions. |
| Adapter | optional `adapter_ref`, `load_identity` | Both absent for base-model proof; both present for adapter-bound proof. |
| Request | `input_shape`, `output_shape` | Use canonical enums and sorted attributes; never store request content. |

Legitimate absence in a complete v2 tuple is not the same as a field omitted
from an older schema. Proof evidence must therefore preserve both:

- the normalized tuple value, including explicit not-applicable values; and
- whether an older record never contained that dimension.

## Issue-Internal Decisions

These are implementation decisions that should be settled inside `#127`.
The recommended choice is recorded so implementation can proceed after the
user decision items below are accepted.

### D127-01: Separate v2 Domain Instead Of Expanding Legacy In Place

Recommendation: accepted by default.

- Keep `ModelCapabilityProof` as the readable v1/legacy representation.
- Add focused compatibility tuple, proof v2, evidence, and key types.
- Do not label a partial legacy record as schema v2.
- Keep `mod.rs` and `lib.rs` as composition files.

Reason: current proof producers cannot supply every v2 field, and expanding the
existing type in place would blur explicit v2 absence with legacy missing data.

### D127-02: Versioned Path And Hashed Exact Key

Recommendation: accepted by default.

Use:

```text
models/store/<model_ref>/support-proofs/v2/<capability>/<tuple_sha256>.toml
```

Generate `tuple_sha256` from deterministic canonical JSON for the normalized
tuple. Store the complete tuple in the TOML body and verify on read that the
body recomputes to the filename key.

Do not place raw tuple values in the filename.

### D127-03: Exact Replacement And Retention

Recommendation: accepted by default.

- Saving the same normalized tuple atomically replaces its one current proof.
- Saving a different tuple creates a different file.
- `remove_exact` removes only the selected v2 key.
- Existing capability-wide clear removes v2, v1, and legacy proof for that
  model capability.
- This issue does not add append-only history for repeated results of one
  exact tuple.

### D127-04: One Normalized Evidence View

Recommendation: accepted by default.

Expose a normalized evidence view with:

- record generation: `v2`, `tuple-aware-v1`, or `legacy-latest`;
- proof status, source, timestamp, and sanitized failure;
- normalized values that are present;
- an explicit set of dimensions unavailable in the source record;
- the exact v2 key when one exists.

Do not deduplicate v2 and older evidence by the current partial v1 key.

### D127-05: Conservative Matching

Recommendation: accepted by default.

- Exact v2 evidence can produce `verified` or `failed`.
- Older evidence may still apply to a query that asks only for dimensions the
  evidence contains.
- If a query requires a dimension absent from older evidence, that evidence is
  `stale`, not exact.
- An exact current v2 record wins over less-specific older evidence.
- A positive hint must not hide an applicable local failure.
- Corrupt or key-mismatched v2 records fail closed as store errors; quarantine
  and operator recovery belong to `#130`.

This preserves current partial callers while allowing `#128` and `#129` to
become strict when they start supplying complete queries.

### D127-06: Reuse Atomic Write And Resource Coordination

Recommendation: accepted by default.

- Reuse `foundation::fs::atomic_write`.
- Reuse `FileResourceCoordinator` and
  `ResourceKind::ModelCapability`; do not create a second lock protocol.
- Use a shared lock for a consistent list/query snapshot.
- Use an exclusive lock for save, exact remove, and capability-wide clear.
- Keep the lock only around lookup, authoritative reread, and filesystem
  transition.
- Use identity `<model_ref>|<capability>` so existing resource transitions and
  proof mutations coordinate on the same key.

The store/use-case boundary may need a small context object carrying both
runtime and model-store layouts rather than deriving lock paths from arbitrary
filesystem parents.

### D127-07: Query Boundary Without Persistent Secondary Indexes

Recommendation: pending user confirmation under U127-02.

- Use the SHA-256 filename as the exact primary index.
- Provide a typed filter for model, capability, backend/runtime, adapter,
  platform/device, shape, and exact key.
- Filter a model's bounded local proof set in memory for non-key fields.
- Keep the port replaceable if a later scale requirement justifies SQLite or
  persistent secondary indexes.

### D127-08: Existing Producers Stay Legacy Until They Know The Tuple

Recommendation: pending user confirmation under U127-01.

- Current manual metadata probe remains v1/legacy evidence.
- Current server-start and runtime-execution producers retain their existing
  behavior in this issue.
- `#128` migrates model-bound server and Cluster producers to complete v2.
- `#129` migrates adapter-bound producers to complete v2.
- Tests and kernel callers may save constructed complete v2 proof through the
  new port.

## User Decisions Required Before Implementation

### U127-01: Write Transition

Recommended: dual-read only; do not project a complete v2 record back into the
lossy legacy latest path.

Why a decision is needed:

- Dual-writing a lossy legacy projection helps an older binary see a new
  result, but that older binary may treat the incomplete projection as exact.
- Dual-read-only avoids false authorization. Existing legacy writers continue
  until their flows migrate in `#128` and `#129`.

Decision options:

1. `dual-read-only` (recommended);
2. `v2-plus-lossy-legacy-dual-write`.

### U127-02: Meaning Of Indexed Query

Recommended: exact hashed path plus typed in-memory filtering; no durable
secondary index files in `#127`.

Why a decision is needed:

- Persistent secondary indexes make one proof write a multi-file transaction
  and materially widen concurrency and recovery work.
- The proof set is local and bounded per model, so scanning one model's proof
  files remains predictable at current scale.

Decision options:

1. `exact-key-index-plus-filter` (recommended);
2. `persistent-secondary-indexes`.

### U127-03: Platform And Device Granularity

Recommended: stable execution class only, such as:

- `macos/aarch64/metal`;
- `linux/x86_64/cuda`;
- `linux/x86_64/cpu`;
- `windows/x86_64/cuda`.

Runtime package, CUDA runtime, and similar versions remain explicit runtime
facts instead of being embedded in a hardware display string.

Decision options:

1. `stable-execution-class` (recommended);
2. `full-host-fingerprint`.

The full-host option creates more proof churn after irrelevant hardware or
driver-detail changes and risks recording identifying hardware text.

### U127-04: Shape Granularity

Recommended: identity contains endpoint family, sorted modalities, provider
shape, streaming/output format, and only execution-affecting normalized
attributes. Request size, prompt content, filenames, and ordinary sampling
values are excluded.

Decision options:

1. `execution-shape` (recommended);
2. `request-profile`, which would additionally key context/size buckets and
   other request limits.

The broader request-profile option may be useful later but can fragment proof
before concrete route policies define which limits are compatibility-relevant.

### U127-05: v2 Completeness Policy

Recommended: a v2 constructor requires every tuple dimension to be explicitly
resolved as a value or typed not-applicable state. It rejects unknown or
omitted execution facts.

Decision options:

1. `strict-complete-v2` (recommended);
2. `allow-unknown-v2-fields`.

Allowing unknown fields would make runtime or platform changes impossible to
detect reliably and would reproduce the legacy ambiguity inside schema v2.

## Implementation Plan

### Phase 1: Lock Contracts And Types

- [ ] Resolve U127-01 through U127-05.
- [ ] Update the proof-schema contract with the exact v2 marker and tuple
  shape.
- [ ] Update support-status rules for missing dimensions and precedence.
- [ ] Update model-store layout and transition rules.
- [ ] Define focused tuple component, proof v2, evidence, filter, and key
  types.

### Phase 2: Normalize And Key Exact Tuples

- [ ] Validate required identifiers and paired adapter fields.
- [ ] Canonically order modalities and extension attributes.
- [ ] Make normalization idempotent.
- [ ] Serialize canonical identity JSON.
- [ ] Generate and validate the SHA-256 exact key.

### Phase 3: Add v2 Persistence Boundaries

- [ ] Add the versioned proof v2 layout.
- [ ] Add exact get, save/replace, list/filter, and remove ports.
- [ ] Preserve the existing v1/legacy compatibility methods.
- [ ] Use atomic replacement and directory synchronization.
- [ ] Coordinate list/save/remove/clear through the model-capability key.
- [ ] Keep capability-wide clear backward compatible.

### Phase 4: Add Dual-Read Evidence And Resolver Semantics

- [ ] Read v2, tuple-aware v1, and legacy latest proof.
- [ ] Preserve generation and missing-dimension metadata.
- [ ] Apply exact v2 precedence.
- [ ] Return stable stale reasons for required missing dimensions.
- [ ] Avoid partial-key deduplication across schema generations.
- [ ] Preserve current behavior for callers that still submit partial queries.

### Phase 5: Test The Boundary

- [ ] Same normalized tuple produces the same key.
- [ ] Every execution-affecting difference produces a different key.
- [ ] Same tuple replaces only itself.
- [ ] Backend/runtime, adapter, platform, profile, and shape variants coexist.
- [ ] Exact removal leaves unrelated proof intact.
- [ ] Capability-wide clear removes all three generations.
- [ ] Legacy records remain readable without becoming precise evidence.
- [ ] Exact v2 wins over less-specific older evidence.
- [ ] Key/body mismatch and malformed v2 fail closed.
- [ ] Concurrent independent store instances do not create torn TOML, lost
  unrelated proof, or visible temporary files.

### Phase 6: Validate And Hand Off

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo check --workspace`.
- [ ] Run focused `tentgent-kernel` model proof and resolver tests.
- [ ] Run `cargo test --workspace`.
- [ ] Run `git diff --check`.
- [ ] Confirm no `#128`, `#129`, or `#130` user-facing behavior entered the
  branch.
- [ ] Record the final tuple and proof v2 contract for downstream issue plans.

## Expected Code Touchpoints

The exact file split may change after implementation inspection, but the
expected boundary is:

- focused compatibility domain/key modules under
  `src/tentgent-kernel/src/features/model/`;
- model proof ports and use-case request/result types;
- model-store proof layout and filesystem proof infrastructure;
- support-status matching and stale-reason logic;
- focused model domain, persistence, resolver, migration, and concurrency
  tests;
- the three source contracts named by issue `#127`.

Large new types should not be appended to the already broad `domain.rs` or
turn `mod.rs` into an implementation file.

## Risks And Guardrails

| Risk | Guardrail |
| --- | --- |
| Partial old data is mislabeled as v2. | Separate legacy and v2 types; strict v2 construction. |
| A lossy legacy projection authorizes the wrong tuple. | Prefer dual-read-only transition. |
| Tuple normalization changes silently. | Canonical identity tests and an explicit schema version. |
| Persistent indexes become inconsistent. | Keep only the exact-key file index in this issue. |
| Concurrent operations expose partial records. | Shared coordinator plus atomic replacement. |
| Hardware details over-fragment proof or leak identifying text. | Use stable typed execution classes. |
| Shape identity stores request data. | Store only normalized shape metadata. |
| `#127` accidentally changes production gates. | Keep current partial callers compatible; migrate gates in later issues. |

## Completion Boundary

Close `#127` only when all accepted decisions are reflected in contracts,
implementation, and tests, and when `#128` and `#129` can build complete
queries and proof producers without redefining the storage or tuple identity.

Move this document to `docs/plans/archive/` only when the issue is complete and
the parent plan no longer needs it as an active execution reference.
