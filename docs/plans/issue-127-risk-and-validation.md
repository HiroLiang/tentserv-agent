# Issue 127 Risk And Validation Checklist

Status: implementation checklist, 2026-10-08. Matrices below define required
coverage; only the execution evidence section records completed runs.

Owner: [#127 execution plan](./issue-127-compatibility-tuple-proof-v2-plan.md).
Baseline: main `8c5d215`, integrated by `4f38069`.

## Source Map

Paths are relative to the repository root. Line numbers describe the baseline,
not a promise that implementation will preserve source positions.

| Boundary | Source location / observation |
| --- | --- |
| Proof storage | `src/tentgent-kernel/src/features/model/infra/proof.rs`: direct writes at 52/115, nested list/save calls, capability removal at 120, TOML reads at 181. |
| Producers and clear | `features/model/usecases/proof.rs`: runtime recorder 58, manual metadata verification 102, general recorder 145, separate count/remove 175. Runtime version is absent at 225; sanitizer at 234 masks names only. |
| Partial identity | `features/model/domain.rs:847`: filename escaping is not injective (`/` and `_2f`; empty and `empty`). Preserve old paths; new keys need collision fixtures. |
| Legacy resolver | `features/model/support_status.rs:307`: proof selection depends on sorted input; version matching at 350 compares only known values. |
| Production consumers | `features/server/usecases/support_gate.rs:35` and `features/cluster/usecases/readiness.rs:360`: shared legacy proof results determine startup/readiness. |
| Existing permits | `features/server/usecases/lifecycle.rs`: acquire/re-resolve at 257/276 and 349/361; declared lock set at 564 already includes proof-related shared keys. |
| Model mutation | `features/resource_guard/registry.rs:144`: model delete/capability replacement hold `Model Exclusive`. `features/model/usecases/remove.rs:61` preserves resource blockers. |
| Atomic primitive | `src/tentgent-kernel/src/foundation/fs/atomic_write.rs`: same-directory temporary file, sync, replacement, directory sync where supported; platform-fs owns Windows replacement. |
| Coordinator | `features/resource_coordination/infra/file_coordinator.rs:88`: one request is sorted; no nested acquisition or upgrade protocol. `ResourcePermit::keys()` alone does not prove lock mode. |
| Eager proof owner | `src/tentgent-daemon/src/server/local/startup.rs:99`: worker terminal preload outcome owns proof; callers must not add observation-timeout writes. |
| Runtime evidence | Kernel chat/embedding/rerank use cases and daemon `server/local/evidence.rs`: evidence persistence may be best-effort; inference success does not prove disk persistence. |

Unqualified `features/` paths above are under `src/tentgent-kernel/src/`.

## Risk Register

Priority describes implementation impact, not a claim of a reproduced incident.

| Risk | Priority / failure | Required guard and step |
| --- | --- | --- |
| R1: gate policy drifts | High: working Local/Cluster starts become blocked. | Separate complete resolver from legacy gate adapters; fixture parity, Steps 1/5. |
| R2: guessed execution facts | High: wrong runtime/adapter/shape becomes verified. | Strict v2, no backfill, observed-fact provenance; manual and preload remain limited evidence, Steps 1/3/5. |
| R3: old writers remain unsafe | High: new v2 is safe but production v1 is unchanged. | Atomic and coordinated v1 plus legacy mirror, Step 2. |
| R4: lock reentry/order | High: startup hangs or reports self-Busy. | Single acquisition or validated borrowed transaction; no hidden upgrades, Steps 1/2. |
| R5: proof/model-delete race | High: deleted model directory is recreated, or obsolete metadata is used. | Shared Model lock and authoritative reread, Steps 2/4. |
| R6: false transaction promise | High: crash exposes only one of the two legacy writes, or partly cleared evidence. | Explicit per-file guarantee, partial errors, restart/retry fixtures, Steps 2/4. |
| R7: old evidence regains authority | High: remove/downgrade/partial filter authorizes an unrelated tuple. | No v2 projection, generation-aware reads and exact results, Steps 4/5. |
| R8: identity drift/collision | High: unrelated proof overwrites or matches. | Versioned canonical bytes and per-dimension golden vectors, Step 3. |
| R9: secret/payload persistence | High conditional exposure; current sanitizer is incomplete. | X127-01 maintenance decision; typed bounded shapes and safe error boundary before real v2 failures. |
| R10: #131/#132 regressions | High: changed readiness, retention, release, or reload. | No lifecycle/key changes; worker proof timing and lifecycle regression matrix, Steps 2/5/6. |
| R11: scale/corruption | Medium: long scans, memory use, or one malformed file disrupts diagnostics. | Model-scoped filters, bounded parsing, explicit errors; no unbounded global scan or silent skip. |
| R12: incomplete CI evidence | Medium: a green existing PR check omits new model/Python tests. | Run local gates and add targeted native proof coverage; zero filtered tests do not count, Step 6. |

## Transaction Design To Freeze In Step 1

Every independent proof operation declares its complete lock set in one
`ResourceLockRequest`, using the existing canonical ordering and bounded Busy
policy. Use the same runtime-home lock root; pass `RuntimeLayout` explicitly
instead of deriving it from model-store parent paths.

| Operation | Maintenance | Model | ModelCapability |
| --- | --- | --- | --- |
| Exact or capability read | Shared | Shared | Shared for the selected capability |
| Model-wide snapshot | Shared | Shared | Shared for every capability in the current kernel enum, acquired together |
| Save/replace/exact remove/capability clear | Shared | Shared | Exclusive for the selected capability |
| Existing model deletion/capability replacement | Existing shared maintenance policy | Exclusive | Do not change resource-guard policy |

Model-wide snapshots lock all known capability keys, not only those seen before
acquisition; otherwise a newly written capability can escape the snapshot.
Scope single-capability gate reads narrowly. Unknown schema/capability handling
must remain explicit, not cause an unlocked fallback scan.

Inside the transaction:

1. Re-read authoritative model metadata/existence and validate model identity.
   Do not recreate a model after deletion. If previously observed facts changed,
   reject/retry from resolution; do not rewrite evidence to claim new facts.
2. Read/count, serialize, and perform only the bounded filesystem transition.
3. Release permits before model loading, network calls, streams, shutdown waits,
   or callbacks that may acquire additional resources.

Provide owned and borrowed transaction paths. A borrowed path must verify the
required keys **and modes**; a raw permit with matching key names is not enough.
Use a typed token/validated permit extension, not `skip_lock: bool`. Restrict
unguarded helpers to transaction internals. Audit all already-held permits in
both Local and Cluster callers, not only direct model commands.

No in-place shared-to-exclusive upgrade. If a caller needs a different lock
set, release, acquire the complete set, and reread with bounded retries. Avoid
holding a later Server key while acquiring an earlier Model key. The existing
coordinator sorts only a single request, not an entire nested call stack.

Proofs remain evidence, not durable ownership claims or long-lived resource
blockers. Preserve delete-model guards, runtime leases, and reconcile behavior.

### Filesystem Failure Semantics

- Reuse the existing atomic primitive for v1, legacy, and v2; do not add another
  platform replacement implementation or unsafe code to the kernel.
- Legacy dual-write order: authoritative tuple-aware support file first,
  compatibility latest mirror second, under one exclusive transaction.
  Return success only after both succeed. If the second fails, report that the
  primary may already be committed; do not claim automatic rollback.
- Readers retain the existing same-key support-file preference over its legacy
  mirror. Different-key older evidence remains older evidence.
- Bulk clear is serialized with writers, but not crash-atomic across all files.
  Count from the same snapshot; success means all selected generations are
  gone. Partial failure is explicit and retryable, not a successful count.
- A directory-sync error can occur after replacement: an error may mean a
  complete new value is visible. Re-read and retry safely.
- Ignore atomic `.tmp` artifacts during reads. Any cleanup must target only
  recognized proof temporaries and must not delete another live writer's file.
- Keep stored records bounded and reject malformed/unknown v2; recovery UX and
  quarantine are deferred to `#130`, not silently invented in the store.

Supported concurrency assumes local filesystems, cooperating binaries, and the
same `TENTGENT_HOME` coordination root. Network filesystems, old binaries that
ignore the protocol, and different homes sharing one data root are not covered.

## Upgrade, Removal, And Rollback Fixtures

| Scenario | Required result |
| --- | --- |
| Read existing v1 and latest-only files | Preserve current partial API results; mark missing fields in the new evidence view. Never synthesize exact v2. |
| Add complete v2 beside old evidence | Three generations coexist; legacy gates see only their compatible evidence; exact queries use exact v2. |
| Same tuple saves two outcomes | Serialized atomic replacement; one current result, no torn record or filesystem-order tie-break. |
| Different tuple saves a newer outcome | Must not replace or outrank the queried exact tuple. |
| Remove one v2 key | Only that file disappears; older records may be listed but cannot restore exact verified/failed. No new tombstone protocol. |
| Clear one capability | Remove all three generations, preserve every other capability and model asset; count logical evidence, not mirrors. |
| Downgrade to old binary | Old version ignores v2; old clear does not remove v2. Re-upgrade can reveal it again. Do not promise old-version clear semantics for new records. |
| Run new and old writers together | Unsupported on a shared store; document the limitation rather than claiming advisory locks protect nonparticipants. |
| Malformed/unknown v2 or body/key mismatch | New v2 operation returns an explicit store error; no false verified result and no destructive repair. |

## Test Matrix By Step

| Step | Required tests and checkpoints |
| --- | --- |
| 1: contract fixtures | Legacy metadata/manual/server/runtime evidence; current CLI/REST statuses and clear counts; enumerate producer fact availability and held locks. |
| 2: safe legacy | Independent-process read/write, same/different tuple, clear/write, model-delete/write, metadata-change/write, maintenance exclusion, held-permit reads, bounded Busy, process-exit lock release. Inject failures before and after replace and between legacy writes. |
| 3: tuple/key | Every dimension changes the key; aliases/order do not; normalization is idempotent. Base absence differs from missing adapter fields; paired fields validate. `/` versus `_2f`, empty versus `empty`, case-sensitive opaque values, Unicode/length/unknown-field validation, stable JSON/TOML round-trip. |
| 4: v2 store | Three-generation coexistence, exact query/replace/remove, capability clear/count, model-wide snapshot, corrupt/path-mismatch/schema-version rejection, ignored temp files, bounded input, rollback table above. |
| 5: resolver | Hard incompatibility, exact current failure vs positive hint, exact vs unrelated newer proof, missing legacy fields, adapter/base isolation, runtime/profile/platform/shape isolation, broad filter cannot authorize, identical legacy gate behavior. |
| 6: native closeout | Complete workspace and Python tests; macOS/Linux/Windows atomic/concurrency proof tests and long paths. Record native vs simulated coverage honestly; no zero-test success claims. |

Crash tests terminate only fixture-owned subprocesses in isolated temporary
stores. Test exit after support replace but before mirror replace, and during
bulk clear; restart and safely retry. Do not exercise crash injection against
the user's runtime home, models, running servers, or installed release.

## Existing Feature Regression Matrix

| Surface | Invariant | Existing test entry points |
| --- | --- | --- |
| Manual verify/inspect/clear | Existing CLI/REST schema and status remain compatible. Metadata verification does not load a model or become complete execution proof. Inspect does not refresh idle clocks. | Kernel `features::model`; CLI model tests; daemon `model_resources`. |
| Local runtime evidence | Only resolved, dispatched local outcomes write proof. Cloud/lookup/validation failures do not. Preserve best-effort evidence policy; test persistence failures directly. | Kernel `features::chat`, `features::embedding`, `features::rerank`; daemon Local evidence tests. |
| #131 retention/ownership | Model idle default 0 and runtime default 300 stay distinct; health is observational; leases, release-all, generation reuse, first-spawner policy, and reconcile remain unchanged. | Kernel `runtime_ownership`, `model_daemon`, `resource_guard`; Python lifecycle/resource cleanup tests. |
| #132 Local eager | Lazy writes no preload proof; new/reused eager preload remains worker-owned. Observation timeout/transport/old endpoint do not write Failed. Starting rejects inference. | Daemon `server::local`; `test-local-server-startup.py`. |
| #132 Cluster | Physical-key dedup, candidate isolation, failed reload rollback, superseded candidate rejection, promotion and old-stream drain remain intact. | Daemon `server::cluster`; Cluster startup/reload scripts. |
| Cloud/image | Explicit Cloud false/0/null lifecycle options remain invalid; legacy refs stay stable. Diffusers and MLX/MFLUX generation stay lazy-only even with allow-unverified. | Kernel/CLI/daemon server-option tests; Python image lazy tests. |
| Model mutation | Stored/runtime resource blockers still apply. Proofs do not add durable claims or resurrect removed models. | Kernel model removal, resource-guard and coordinator tests. |

Load mode, idle values, server refs, and runtime generation tokens are not new
compatibility identity fields. Do not change physical runtime ownership keys
as a side effect of proof identity work. Training success is not serving proof.

## Validation Commands And Evidence Rules

Discover test names before filtering; append `-- --list` to the selected Rust
suite if unsure. Every filtered run must record a nonzero executed count.
These commands are the planned starting set, not completed validation:

```bash
cargo test --locked -p tentgent-kernel features::model
cargo test --locked -p tentgent-kernel atomic_write
cargo test --locked -p tentgent-kernel resource_coordination
cargo test --locked -p tentgent-kernel resource_guard
cargo test --locked -p tentgent-kernel features::server
cargo test --locked -p tentgent-kernel features::cluster
cargo test --locked -p tentgent-kernel features::chat
cargo test --locked -p tentgent-kernel features::embedding
cargo test --locked -p tentgent-kernel features::rerank
cargo test --locked -p tentgent-kernel runtime_ownership
cargo test --locked -p tentgent-kernel model_daemon
cargo test --locked -p tentgent-daemon model_resources
cargo test --locked -p tentgent-daemon server::local
cargo test --locked -p tentgent-daemon server::cluster
cargo test --locked -p tentgent-cli
```

After building both local binaries, run the POSIX subprocess suites sequentially
inside an isolated Python environment; do not share fixture ports. In a temporary
validation shell, prepare the environment before using `--no-sync`:

```bash
cargo build --locked -p tentgent-cli -p tentgent-daemon --bins
proof_test_dir="$(mktemp -d "${TMPDIR:-/tmp}/tentgent-proof-validation.XXXXXX")"
export UV_PROJECT_ENVIRONMENT="${proof_test_dir}/env"
uv sync --project python/tentgent-model-runtime --frozen --no-editable \
  --reinstall-package tentgent-model-runtime --group dev --python 3.12
uv run --no-sync --project python/tentgent-model-runtime python scripts/test-local-server-startup.py
uv run --no-sync --project python/tentgent-model-runtime python scripts/test-cluster-server-startup.py
uv run --no-sync --project python/tentgent-model-runtime python scripts/test-cluster-server-reload.py
```

The existing comprehensive source gate provides fmt, warning-denied all-target
compile, full Rust tests, clean Python source reinstall/import/tests, and
platform-appropriate subprocess smoke; it does not publish a release:

```bash
bash scripts/test-release-source.sh
git diff --check
```

The full source gate already includes the three POSIX suites; do not immediately
repeat them without a relevant code change. Prefer that wrapper for full
closeout because it also cleans up its isolated environment. A focused run
above keeps its uniquely created environment for inspection; clean up only
that recorded directory after collecting results and close the temporary shell.

Use the pinned Rust 1.99.0 / Python 3.12 baseline; do not upgrade toolchains or
dependencies as part of proof work. New platform-specific tests need native
evidence; cross-compilation alone does not validate filesystem locking.
The existing Windows PR workflow checks the workspace but only runs selected
runtime tests. Add a narrow proof test job/step before closeout as needed;
general CLI/Python PR-CI expansion remains separate maintenance work.

For each checkpoint record: commit, command, actual passed/failed counts,
skipped/ignored reasons, native platform, fixture scope, and any remaining
limitation. Stop on changed stable output, self-Busy/deadlock, corrupted proof,
unreleased fixture process, source/schema mismatch, or an unexplained skip.
Do not update expected snapshots merely to conceal a compatibility regression.

## Intermediate Execution Evidence

Initial integration checkpoint on 2026-10-08 (macOS arm64, working tree after
the independent #141 commit `e1c4226`; not final issue closeout):

- `cargo check --locked --workspace --all-targets`: passed, no warnings.
- `cargo test --locked -p tentgent-kernel features::model --lib`: 112 passed,
  0 failed/ignored, including legacy storage/use cases and #141 synthetic
  write/read/parse-error cases. Later domain changes require another run.
- `cargo test --locked -p tentgent-daemon model_resources -- --nocapture`:
  21 library tests passed; the separate binary target has no matching tests
  and is not counted as evidence.
- Pure tuple refinement: 36 focused domain tests passed; the fixed canonical
  JSON vector's SHA-256 was independently checked outside the Rust code.

Native CI, full source regression, concurrency/crash coverage, and final diff
review are still pending. Intermediate green checks do not complete #127.
