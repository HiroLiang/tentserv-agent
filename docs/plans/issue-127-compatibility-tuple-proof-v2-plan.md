# Issue 127 Compatibility Tuple And Proof v2 Sub-Plan

Status: implementation complete; final native validation and PR review pending.
Updated: 2026-10-08. User decisions and intermediate checks are recorded below.

- Issue: [#127](https://github.com/HiroLiang/tentserv-agent/issues/127)
- Parent: [v1.2.0 Local Compatibility State Plan](./v1.2.0-local-compatibility-state-plan.md)
- Branch: `feature/127-compatibility-tuple-proof-v2`
- Main baseline: `8c5d215595379a3b6e10a1b5c1da36efdf7d76f0`
- Main integration: merge commit `4f38069`; only two documentation-router
  conflicts required resolution. Product source matched main at that checkpoint.
- Companion: [risk, transaction, and validation checklist](./issue-127-risk-and-validation.md)

This refresh replaces the old six-phase checklist with reviewable steps that
include their own tests. The user accepted the recommendations and authorized
completion of #127 on 2026-10-08. Keep steps in this branch with separate commits;
do not create one branch per implementation layer.

## Outcome And Scope

Build the kernel foundation for complete compatibility tuples and durable
proof v2. Later issues can persist and query exact evidence without confusing
different models, runtimes, adapters, or request shapes.

Included:

- Normalized tuple, versioned identity, proof record, evidence, and filter types.
- Exact get/save/replace/remove and model/capability list/filter ports.
- Three-generation reads: v2, tuple-aware v1, and legacy latest proof.
- Atomic, coordinated persistence for **existing v1/legacy writers as well as
  v2**; capability-wide clear covers all three generations.
- Conservative exact resolution, while existing partial callers retain their
  compatibility behavior.
- Schema, support-status, model-store, and coordination contract updates;
  concurrency, crash, migration, and cross-entrypoint regression tests.

Excluded:

- New Local/Cluster hard gates or runtime fact collection (`#128`).
- Adapter load-identity computation and LoRA serving gates (`#129`).
- New CLI/REST exact-query flags, DTOs, doctor views, and recovery UX (`#130`).
- Automatic migration/backfill, lossy v2 projection, persistent secondary
  indexes, SQLite, fallback, runtime scheduling, or new model families.
- Runtime ownership refactors, broad code/document cleanup, and releases.

Adapter/load identity and execution shape are foundation types here. A new
proof-write API accepts authoritative facts; it does not discover or guess
them. The known error-redaction gap is a separate maintenance decision, not an
unannounced addition to this issue (see X127-01 below).

## Baseline Findings

1. Current proof writes use `fs::write` twice: tuple-aware support, then legacy
   latest. Clear counts and removes in separate operations, without shared
   coordination. These production paths must benefit from this issue.
2. Current producers write `runtime_version = None`. Manual verify checks
   capability metadata; eager preload proves loading, not an inference shape.
   Neither can be relabeled as a complete v2 execution proof.
3. Local server gates and Cluster readiness consume the existing partial
   resolver. Changing all legacy results to `stale` would silently introduce
   `#128` behavior and block previously working starts.
4. Model deletion locks `Model` exclusively. Proof-only capability locks are
   insufficient; a late writer could recreate a deleted model directory.
5. Server preparation already holds resource permits when reading proof.
   Blindly adding nested locks risks self-contention and lock-order inversion.

Code locations and risk priorities are in the companion. These are source
findings, not a claim that corruption or credential leakage occurred locally.

## Accepted Tuple Contract

| Dimension | Required representation / fact source |
| --- | --- |
| Model | Validated canonical `model_ref` and capability enum. |
| Format | Typed primary format and explicit quantization; no guessing from names. |
| Backend | Registered backend and general runtime-family identifiers. |
| Runtime | Selected backend package and observed version, not CLI version or a fixed Python project version. |
| Profile | Selected execution profile id/version, or contract-approved not-applicable; not bootstrap dependency profiles. |
| Environment | Typed OS, architecture, and execution device class. |
| Adapter | Explicit base-model state, or paired opaque adapter ref/load identity supplied by `#129`. |
| Shape | Typed input/output family, sorted modalities, provider shape, streaming/output format, and allowlisted load-affecting attributes. |

Normalization must be idempotent, bounded, and validated on deserialization as
well as construction. Canonicalize only registered aliases; preserve opaque
case-sensitive identity/version values. Sort and deduplicate unordered sets.
Reject unknown attributes instead of hashing arbitrary request payloads.

Missing legacy dimensions, unresolved facts, and explicit not-applicable are
different states. Only contract-approved absence is valid in a complete v2
tuple. Do not use `latest`, empty strings, or not-applicable to conceal missing
runtime facts. Runtime facts must eventually describe the selected/reused
runtime, not whichever Python environment the caller happens to inspect.

Under recommended U127-03, the initial identity tracks the selected backend
package version, not every transitive dependency, CUDA/driver version, OS patch,
or GPU model. It is not a full environment reproducibility or capacity promise.
Stronger environment fingerprinting requires an explicit schema/scope decision.

## Internal Design Decisions

These implementation decisions were frozen in Step 1. Escalate only if later
integration needs materially different behavior.

| ID | Accepted decision |
| --- | --- |
| D127-01 | Separate v2 tuple/proof/evidence types; retain legacy domain types and wire formats. Use focused modules; keep composition files free of implementation. |
| D127-02 | TOML `schema_version = 2`, `record_kind = local-proof`; SHA-256 of canonical tuple JSON including an explicit identity-version marker. A normalization change requires a new identity version. |
| D127-03 | One current file per exact v2 tuple. Same-key replacement is serialized; the last successfully committed replacement is current. Timestamps are provenance, not an interprocess compare-and-swap clock. |
| D127-04 | Evidence preserves generation, source, status, time, safe failure summary, present values, missing dimensions, and optional v2 key. Never deduplicate v2 by a partial legacy key. |
| D127-05 | Add a separate complete-query resolver. Keep current gate adapters and legacy ordering/precedence until `#128`/`#129`; diagnostic filters never authorize execution. |
| D127-06 | Reuse `atomic_write` and `FileResourceCoordinator`, with explicit runtime/store context and one transaction acquisition. Follow the companion's lock matrix and borrowed-permit requirements. |
| D127-07 | Exact-key lookup plus typed in-memory filtering over a model's files; stable ordering and bounded record parsing. No durable secondary index. |
| D127-08 | Current producers retain source, schema, and trigger timing, but adopt safe persistence. Only callers with complete observed facts may write v2. |
| D127-09 | Promise per-file atomic replacement and coordinated operations, not power-loss all-or-nothing across legacy dual writes or bulk clear. Report partial failure and allow safe retry. |

V2 path:

```text
models/store/<model_ref>/support-proofs/v2/<capability>/<tuple_sha256>.toml
```

Validate directory model/capability, filename key, and body tuple together.
Reject unsupported schema/identity versions, malformed records, and key/body
mismatches in v2 APIs; do not turn corrupt evidence into a successful match.
Keep legacy readers generation-specific so v2 files do not break old gates.

Exact resolution preserves hard incompatibility and applicable failure
precedence. A current exact v2 record outranks less-specific old evidence.
Older evidence missing a required dimension is stale evidence, never exact
verified/failed. Unrelated newer proof must not override an exact match.

`remove_exact` deletes only the selected v2 record. Older evidence may remain
visible as older evidence, but cannot resurrect an exact result. Capability
clear removes v2/v1/legacy together under one transaction. Its count includes
distinct v2 keys plus deduplicated v1/legacy records, not physical mirror files;
legacy-only fixtures retain their existing count.

## User Decisions Before Implementation

All five recommended options were **accepted** on 2026-10-08.

| ID | Recommendation | Alternative and tradeoff |
| --- | --- | --- |
| U127-01 | Dual-read only; never project v2 into lossy legacy records. | Dual-write helps old binaries see new results but may authorize the wrong tuple. Old binaries cannot see or clear v2; do not promise semantic rollback. |
| U127-02 | Exact hashed key plus in-memory typed filters. | Persistent secondary indexes add multi-file consistency/recovery work; revisit only with measured scale requirements. |
| U127-03 | Stable OS/architecture/device class plus selected runtime package version. | Full environment/hardware fingerprint increases precision and proof churn, collection scope, and privacy risk. Initial v2 does not detect every driver/dependency change. |
| U127-04 | Execution shape only; exclude prompts, files, arbitrary headers, ordinary sampling values, and request-size buckets. | Request profiles could cover size/capacity limits, but fragment proof and require additional route policy. Verified does not promise every input size succeeds. |
| U127-05 | Strict complete v2; reject unresolved facts. | Unknown fields are easier for callers but reproduce ambiguous legacy authorization. Retain such evidence as legacy instead. |

### X127-01: Separate Proof Error-Redaction Maintenance

Current sanitization removes known environment-variable **names**, not their
values. For example, a synthetic `HF_TOKEN=example-secret` still retains its
value. No real credential exposure has been established.

Accepted: a separate narrowly scoped bug/commit for safe error
persistence and presentation, using synthetic-secret fixtures. Do not silently
fold a general logging/security refactor into `#127`, or defer the known gap
until diagnostics work. Track this as
[#141](https://github.com/HiroLiang/tentserv-agent/issues/141), with an independent
commit before proof persistence integration; no separate checkout is needed.

This does not block tuple/storage design or synthetic tests. Before connecting
v2 failure persistence to real errors or declaring the branch release-ready,
require either the approved maintenance fix or a reviewed fixed-code/allowlist
error summary that never persists arbitrary exception strings. Historical
proof rewriting and broad PII detection require separate approval.

## Reviewable Execution Steps

Each step includes tests and documentation, ends with its own commit and diff
review, and stops on an unresolved contract or regression. The user authorized
proceeding through all steps; perform intermediate automated checks and review
without waiting for another approval unless scope or policy must change.
Do not defer all validation to the last step.

### Step 1 — Freeze Decisions And Compatibility Fixtures

- [x] Resolve U127-01–05 and the handling/dependency of X127-01 (#141).
- [x] Update proof-schema, support-status, model-store, and resource-blockers
  contracts with fields, per-dimension not-applicable rules, value limits,
  lock scope, and failure rules.
- [x] Separate historical aspirational schema examples from implemented v1
  and new v2. Correct stale server-start wording to terminal eager evidence.
- [x] Capture legacy files and current CLI/REST/gate results as fixtures;
  enumerate every proof reader, writer, clear, and already-held permit.
- [x] Define explicit evidence scope so preload never claims unexercised
  inference/provider/streaming shapes.

Check: fixture tests actually run and preserve today's behavior. Review the
tuple table, migration examples, transaction API, and public non-goals before
changing persistence.

### Step 2 — Make Existing Proof Persistence Safe

- [x] Introduce runtime/store context and owned/borrowed proof transactions;
  private raw helpers prevent nested public-method lock acquisition.
- [x] Coordinate list, v1 save, legacy mirror, and capability clear; count and
  mutation use the same snapshot. Revalidate model identity under the lock.
- [x] Replace both legacy direct writes with the existing atomic primitive.
  Surface partial mirror failure without claiming rollback.
- [x] Wire existing callers without changing proof source/trigger timing,
  best-effort inference policy, stable response shape, or gate policy.

Check: same/different-key writers, reader/write, clear/write, model-delete/write,
metadata-update/write, held-permit server paths, and injected write failures.
Run model, coordination, resource-guard, server, Cluster, and REST proof tests.
Review legacy-only fixture diffs; no v2 or startup-policy change in this step.

### Step 3 — Add Complete Tuple And Deterministic Identity

- [x] Add focused tuple/component, v2 proof, evidence, key, and filter types.
- [x] Validate paired adapter/profile fields, explicit absence, typed shapes,
  runtime-version provenance, and size limits.
- [x] Freeze canonical JSON/identity-version golden vectors and SHA-256 keys.
- [x] Test every dimension independently, aliases, ordering, round-trip,
  normalization idempotence, and old filename collision counterexamples.

Check: pure domain tests; no new runtime calls, store mutation, or gate wiring.
Review readable examples for base, adapter, and changed-runtime tuples.

### Step 4 — Add v2 Storage And Three-Generation Operations

- [x] Implement exact get/save/replace/remove and typed list/filter using
  Step 2 transactions; retain v1 path/key compatibility.
- [x] Add explicit three-generation evidence reading and consistent model-wide
  snapshots; do not expose mixed-generation results through legacy gate APIs.
- [x] Cover body/path validation, deterministic ordering, ignored temporary
  files, parse limits, corrupt/unknown schema, and interrupted operations.
- [x] Extend capability clear/count to all generations without touching other
  capabilities or model assets. Validate rollback limitations with fixtures.

Check: atomic/concurrent/crash cases in the companion; fixture demonstration of
upgrade, exact removal, bulk clear, downgrade, and upgrade again. Review disk
layout and error results, including partial failure, not only the happy path.

### Step 5 — Add Exact Resolution Without Migrating Existing Gates

- [x] Implement complete-query precedence and stable missing-dimension reasons.
- [x] Keep legacy gate adapters behavior-compatible; no v2-to-legacy flattening.
- [x] Prove base/adapter/runtime/profile/platform/shape isolation, exact failure
  precedence, and no authorization from a broad filter result.
- [x] Recheck manual verify, runtime execution, and eager writer timing through
  CLI/REST/Local/Cluster callers after persistence wiring changes.

Check: model resolver, server/Cluster gates, CLI/REST fixtures, chat/embedding/
rerank evidence, and #131/#132 suites. A previously working start becoming
blocked is a regression to investigate, not a snapshot to approve silently.

### Step 6 — Native Regression, Evidence, And Handoff

- [x] Run the full Rust/Python source gate and sequential subprocess suites
  listed in the companion; inspect all skipped/ignored/zero-test results.
- [ ] Validate new persistence tests on native macOS, Linux, and Windows;
  add focused PR coverage where the existing workflow would not execute them.
- [x] Record test counts, commands, environment, fault-injection outcomes, and
  unchanged runtime cleanup behavior. No additional model download is required
  for a storage-only change; rerun isolated live lifecycle smoke if relevant
  runtime wiring changes or a regression makes it necessary.
- [x] Update contracts and parent progress, summarize remaining limitations,
  and hand exact tuple/store/query APIs to `#128`/`#129`, diagnostics to `#130`.

Close `#127` only after accepted decisions, implementation, and evidence agree.
Passing storage tests alone does not complete `#126` or authorize publication.

## Progress

- [x] Switched to the existing issue branch and merged current `origin/main`.
- [x] Preserved #131/#132 fixes, Rust 1.99.0, and the v1.1.2 baseline.
- [x] Refreshed scope, risks, decisions, review steps, and validation plan.
- [x] User decision/review checkpoint; recommended options accepted.
- [ ] Steps 1–6 implementation and validation.

## Implementation Review Map

| Checkpoint | Commit / boundary |
| --- | --- |
| Separate #141 | `e1c4226`: bounded credential-value redaction at write/read boundaries. |
| Step 1 | `52c233e`: frozen tuple, proof, migration, and transaction contracts. |
| Step 3 | `fe64e86`: pure validated types, canonical keys, and 39 domain tests. |
| Step 2 | `44dc105`: safe legacy transactions, permit borrowing, and caller integration. |
| Step 4 | `19c9a5b`: v2 storage, generation-aware evidence, and process/fault fixtures. |
| Step 5 | `f6ca67a`: complete-query resolver and 23 precedence/isolation tests. |

Pure types landed before transaction integration so every commit remains
buildable; the legacy transaction commit was separately compiled from an
isolated archive with warnings denied. No runtime facts or gate policies were
migrated. Final counts, native CI, and remaining limitations belong in the
[validation checklist](./issue-127-risk-and-validation.md).

Handoff: #128 collects actual selected-runtime facts and adopts complete model
and Cluster queries; #129 defines adapter load identity and serving checks;
#130 adds exact-query diagnostics and recovery UX. This branch does not publish
or merge itself. #127/#141 stay open until the reviewed PR is merged.
