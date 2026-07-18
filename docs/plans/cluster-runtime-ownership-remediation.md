# Cluster Runtime Ownership Remediation

Status: findings `R1`-`R18` are implemented and pass the final local
verification matrix for GitHub issue `#118` on
`feature/118-cluster-runtime-ownership`. Native Windows CI rerun remains
pending. Parent `#113` closeout is blocked.

Parent records:

- [cluster-runtime-ownership-plan.md](./cluster-runtime-ownership-plan.md)
- [cluster-runtime-coordination-architecture.md](./cluster-runtime-coordination-architecture.md)
- [cluster-roadmap.md](./cluster-roadmap.md)

## Purpose

Record the post-implementation audit findings without rewriting confirmed
decisions `1`-`28`. This remediation is part of `#118`; it is not a new cluster
slice and must not expand into SQLite, provider execution, tuple-aware LoRA
gates, or a general scheduler.

The first audit found lifecycle and transition races that the earlier green
test suite did not exercise. The second closeout audit confirmed that behavior
but found remaining module, injection, CLI projection, watcher verification,
and planning-alignment gaps. This document remains active until both finding
groups and the native Windows gate are closed.

## Verified Baseline

The July 16, 2026 audit produced these results before remediation:

- `cargo test --workspace` passed: CLI `104`, daemon `295`, and kernel `407`
  tests passed; one subprocess helper and seven manual macOS Keychain probes
  remained intentionally ignored in their parent suites;
- `uv run --project python/tentgent-model-runtime pytest` passed `26/26`;
- `git diff --check` passed;
- `cargo fmt --all -- --check` still reports formatting in the unchanged
  manual macOS Keychain probe;
- project-wide strict Clippy is not yet a green baseline and includes both
  existing warnings and warnings in the new coordination implementation;
- a read-only `tentgent runtime reconcile` run correctly found one stale
  physical generation and did not mutate it.

Passing the existing tests does not close the findings below because those
tests do not drive real streaming-body lifetime, graceful server shutdown,
post-spawn cleanup, cross-process mutation drift, or Windows record replacement.

## Finding Register

| ID | Severity | Status | Finding | Required Outcome |
| --- | --- | --- | --- | --- |
| `R1` | P1 | remediated; local tests pass | A spawned Python runtime can survive metadata or health failure after its ownership generation is removed. A caller crash can also leave a worker discoverable only through legacy daemon metadata. | Never remove the generation until child termination is verified; preserve enough starting identity for reconciliation to adopt or diagnose the worker. |
| `R2` | P1 | remediated; local tests pass | A cluster request lease is stored in response extensions, so it may be dropped after headers instead of after a streaming body finishes or is cancelled. | Make the response body own the lease through EOF, body error, cancellation, or drop. |
| `R3` | P1 | remediated; real-listener test passes | Cluster shutdown stops polling the Axum server before waiting for leases, preventing active handlers and bodies from making progress during drain. | Stop admission while continuing to poll existing connections for up to the bounded drain deadline. |
| `R4` | P1 | remediated; focused tests pass | Model capability and adapter mutation operations are derived before lock acquisition, but the post-lock state may imply a different operation or key set. | Re-derive and compare the operation under the acquired permit; release and retry when the protected key set changed. |
| `R5` | P1 | remediated; focused tests pass | LoRA plan/run creation locks preflight dependencies but does not revalidate every model, dataset, and resume-adapter dependency after locking. | Persist or start only from one stable, post-lock dependency snapshot. |
| `R6` | P1 | implemented; cross-check passes; native Windows CI pending | Ownership and daemon metadata replacement relies on `std::fs::rename` replacing an existing destination, which is not a supported Windows update contract. | Use one platform-aware local-filesystem atomic replacement boundary and verify repeated updates on Windows. |
| `R7` | P2 | remediated; local tests pass | A partially acquired lock set releases OS locks but leaves holder metadata behind; retry delay is bounded but deterministic across contenders. | Make provisional lock cleanup RAII-complete and derive real bounded jitter from per-operation entropy. |
| `R8` | P2 | remediated; kernel/CLI/REST tests pass | Cluster and server inspect return a global ownership count instead of focused safe claims and generations for the inspected resource. | Add kernel-owned scoped inspection and additive safe CLI/REST detail while doctor remains global and compact. |
| `R9` | P2 | implemented; local verification passes | `tentgent-platform-fs/src/lib.rs` and `resource_guard/validators/mod.rs` contain implementation logic, while `model_daemon/supervisor.rs` exceeds the mandatory source-file split threshold. | Restore composition-only `lib.rs`/`mod.rs` files and split supervisor tests and focused helpers without changing behavior. |
| `R10` | P2 | implemented; local verification passes | The plan requires one validator file per `ResourceOperation`, but the implementation groups several operations into resource-family files. | Give every operation one focused validator file and keep dispatch in the registry rather than a composition module. |
| `R11` | P2 | implemented; local verification passes | Mutation use cases bind directly to `StdResourceGuard`, and CLI adapters flatten structured blocker outcomes through the compatibility error bridge. One server fallback also returns `in_use` instead of the documented `server_in_use`. | Inject ports across all new orchestration boundaries, preserve typed outcomes through CLI/REST boundaries, render concise identifiable CLI summaries, and restore stable typed codes without changing serialized values or CLI exit policy. |
| `R12` | P2 | implemented; deterministic tests and local measurement pass | The cluster watcher hard-codes its probe and schedule, lacks deterministic cancellation/reload tests, and has no recorded polling-cost evidence. | Inject internal watcher dependencies, add fake-probe/clock tests, and record non-gating platform measurement evidence before freezing defaults. |
| `R13` | P2 | implemented; contracts and tracking aligned | The lock matrix says capability mutation uses a shared model lock plus exclusive capability locks, while the file-backed implementation correctly uses an exclusive model lock because it rewrites one complete metadata file. Issue and project status also lag the actual work. | Align the plan with the safe whole-record lock, update issue routing and project status, and do not claim contract completion before the second audit closes. |
| `R14` | P2 | implemented; local verification passes | `unbind-cluster-route`, `shutdown-runtime`, and `remove-runtime-resource` are declared as complete guard operations but have no production caller. Their current validators therefore do not prove executable policy; a standalone shutdown would also see its own generation as a blocker. | Remove all three variants and their validators until a real mutation use case can add complete operation-specific semantics, locking, adapter mapping, and tests. Do not use reserved enum values as future logging placeholders. |
| `R15` | P1 | remediated; Python regression and real smoke pass | Rust route validation honors a separate `TENTGENT_DATA_ROOT`, but a spawned Python worker previously resolved managed models from the control home and failed after successful cluster validation. | Propagate the resolved data root to every spawned worker and make Python managed-model lookup honor it without changing the default home-equals-data-root layout. |
| `R16` | P1 | remediated; lock regression and real stop pass | Cluster stop holds the server transition lock while waiting for the proxy, but route-claim release previously requested the same server key and could leave all claims stale after an otherwise clean stop. | Keep full dependency locks on claim creation, but let retire/release lock only maintenance plus the claim being removed so shutdown can drain and release claims without weakening reference creation. |
| `R17` | P2 | remediated; busy-release regression and real smoke pass | A hot-reload or drain release that returned `resource-busy` could still remove its in-process route entry, leaving the durable claim without a retry owner until manual reconciliation. | Remove in-process route state only after durable release succeeds; retain busy claims for bounded drain retry or later stop/reconciliation. |
| `R18` | P1 | remediated; Rust 1.81 local matrix passes; native Windows rerun pending | PR `#123` native Windows verification stopped before running ownership tests because the committed dependency graph had drifted beyond the workspace's declared Rust 1.81 minimum. The CLI also used `Option::is_none_or`, which was not stable in Rust 1.81. | Lock direct and transitive dependencies to Rust 1.81-compatible releases, use an equivalent stable CLI expression, and verify the full workspace plus the Windows filesystem adapter with Rust 1.81 before rerunning native Windows CI. |

Decision `27` is clarified as follows: LoRA plan/run `model_ref` is a
model-deletion dependency because current LoRA records do not persist a
Tentgent capability. It does not create a new capability-mutation blocker.
Capability mutation must continue to protect adapter bindings using their
explicit capability or the legacy `chat` default. Tuple-aware LoRA capability
gating remains later work.

## Confirmed Resolution Decisions

Decisions `29`-`33` were accepted and are implemented by the first remediation.
Decisions `34`-`41` were accepted during the closeout audit and govern the
remaining work.

### 29. Post-Spawn Runtime Ownership

Recommended design:

- persist the planned endpoint and process token in the `starting` generation
  before launch;
- after spawn, atomically attach the child PID and endpoint to the same
  generation before waiting for health;
- keep a `PendingRuntimeProcess` RAII guard responsible for terminating the
  process group and waiting for exit on every post-spawn error;
- disarm the guard only after the generation reaches `ready` and the supervisor
  accepts ownership;
- when termination cannot be verified, retain the generation as `starting` or
  move it to `closing` with diagnostic context instead of deleting it;
- let reconciliation probe the planned endpoint plus token and inspect legacy
  daemon metadata so it can adopt a matching live worker or remove only a
  proven-dead record.

Do not solve this with port-only cleanup, PID-only matching, or unconditional
generation removal. Those approaches can terminate an unrelated process or
permit an overlapping replacement.

Completion requires startup-failure, metadata-failure, health-timeout,
cleanup-failure, and caller-crash subprocess tests that prove there is either
no child or one durable diagnosable generation.

### 30. Streaming Request Drain

Recommended design:

- replace response-extension ownership with a `LeasedBody` wrapper that
  delegates the HTTP body protocol and owns `RouteRequestLease`;
- retain the lease through data frames and trailers;
- release it exactly once on normal EOF, body error, cancellation, connection
  drop, or task abort;
- keep non-streaming responses on the same body-owned path so routing code does
  not need two lifecycle models;
- split route-manager shutdown into `begin_drain`, `finish_drain`, and
  `preserve_on_timeout` transitions;
- on termination, close route admission and trigger Axum graceful shutdown;
- continue polling the server future so existing handlers and response bodies
  can complete;
- bound that graceful period to 30 seconds;
- release drained claims after completion, or preserve unresolved claims as
  stale when the deadline expires;
- never terminate shared Python runtime work from the cluster proxy timeout.

Converting the body to a data-only stream is not recommended because it can
lose trailer semantics. Holding a lease in response extensions is not a valid
stream-lifetime boundary. Waiting for leases after dropping the server future
is rejected because it prevents the leases from completing. Unbounded graceful
shutdown is also rejected because `server stop` must remain bounded.

Completion requires a real listener with one delayed streaming body. The test
must observe the claim after headers, reject new admission after the stop
signal, retire the claim only after EOF or body drop, and cover a separate
timeout that leaves a reconcilable claim.

### 31. Stable Resource Transitions

Recommended design:

- add one reusable bounded stabilization helper around guarded mutations;
- each resource use case provides a function that derives the
  `ResourceOperation` and dependency fingerprint from authoritative state;
- authorize and acquire the complete sorted key set, re-read the state, and
  derive the operation again;
- mutate only when the operation and dependency fingerprint match;
- otherwise drop every permit and retry from the beginning without in-place
  lock upgrades;
- return typed busy or unstable-state guidance after the bounded retry limit.

The same stabilization protocol applies to train-plan and train-run creation:

- after locking, reload the plan and compare model, dataset, and resume-adapter
  references with the preflight dependency fingerprint;
- verify the model and required chat support, dataset, and resume adapter still
  exist and remain compatible while their shared permits are held;
- if the plan dependency set changed, release all permits and retry with the
  new set;
- persist the run's `starting` record before releasing transition permits.

Resource-specific derivation remains in focused model, adapter, train, cluster,
and server files. The common helper coordinates retries but must not absorb
resource policy. This remediation does not add a LoRA capability field or tuple
gate.

Completion requires races for capability replacement, adapter rebind and
deletion, plus train creation-versus-delete and creation-versus-rebind tests
for model, dataset, and resume adapter dependencies.

### 32. Cross-Platform State Coordination

Recommended design:

- create one shared local atomic-file adapter used by runtime ownership and
  model-daemon metadata;
- retain temporary-file creation, file sync, same-directory replacement, and
  best-effort directory sync;
- use rename replacement on Unix and an explicit Windows replacement primitive
  with replace-existing and write-through semantics;
- clean temporary files on every failure;
- continue to declare network filesystems unsupported;
- make each provisional `HeldLock` own idempotent release of holder metadata
  and the OS lock;
- construct a provisional lease while acquiring the set so any early return
  triggers the same RAII cleanup as a successful permit;
- derive retry jitter from operation id, process id, and attempt number while
  keeping the existing bounded delay and overall two-second limit;
- keep holder metadata diagnostic-only and the open advisory lock authoritative.

Delete-then-rename is rejected because it creates a missing-record window.
Keeping separate atomic writers is rejected because their platform behavior
will drift. A deterministic attempt-only delay is rejected because competing
processes can remain synchronized. Removing metadata entirely is rejected
because `resource-busy` diagnostics need safe holder context.

Completion requires repeated create-and-replace tests, a Windows CI or
Windows-host execution proving `starting -> ready -> closing` updates, and
assertions that partial acquisition leaves no holder record while independent
operation seeds remain inside the configured jitter bounds.

### 33. Focused Ownership Inspection

Recommended design:

- add a kernel-owned inspection query that can scope by cluster ref, server
  ref, and resolved runtime identities;
- return safe claim and generation projections plus a scoped summary;
- cluster inspect filters claims by cluster and links only their target runtime
  identities;
- cluster-server inspect filters by server ref; direct local server inspect
  supplies its resolved physical runtime identity because it has no route
  claim;
- expose safe fields only: lifecycle state, resource refs, route, capability,
  definition hash, and timestamps;
- keep PID, process token, local paths, and internal lease ids private;
- keep doctor on the current global compact summary.

Filtering global counts only in CLI or REST is rejected because entrypoints
would duplicate ownership policy and could not reliably link generations.

Completion requires kernel filter tests, CLI focused rendering tests, additive
REST compatibility tests, and proof that unrelated cluster/server ownership is
not included.

### 34. Operation-Specific Validator Files

Keep one validator file per `ResourceOperation`. Registry dispatch remains
outside composition modules. Shared scans and reference readers remain probes,
so operation files compose reusable evidence without duplicating filesystem
logic.

### 35. Guard Interface Injection

Mutation use cases must depend on `ResourceGuardUseCase`, not directly on
`StdResourceGuard`. Standard constructors may compose the file-backed default,
while explicit injected constructors support tests and later adapters without
changing product behavior.

### 36. CLI Structured Blocker Projection

CLI mutation commands consume `ResourceMutationOutcome` directly. They render
a concise summary containing the stable result code, operation, resource, and
clearly distinguishable blockers plus next actions. They do not parse the
compatibility error string. REST continues to serialize the same ordered
blocker vector.

### 37. Watcher Injection And Cost Verification

Add an internal injected watcher runner for probe, schedule, and clock-driven
tests. Cover cancellation, unchanged polls, forced hashes, reload failure, and
shutdown. Record non-gating platform measurement evidence; do not use unstable
wall-clock thresholds as correctness gates.

### 38. Issue And Project Tracking

Keep issue `#118` open, set its active project state to `In Progress`, raise it
to `P1`, and link the ownership plan, companion architecture, and remediation
register from the issue. Parent `#113` remains incomplete until all closeout
gates pass.

### 39. Interface Injection Scope

Decision `35` applies to every new `#118` orchestration boundary. Inject
`ResourceGuardUseCase`, `ResourceCoordinator`, `RuntimeOwnershipStore`, process
probes, and health probes at orchestration boundaries while default
constructors compose the standard file-backed adapters. Leaf filesystem
adapters remain concrete implementations of those ports; do not add interfaces
around helpers that do not represent an orchestration or infrastructure
boundary.

### 40. Typed Code Vocabulary And CLI Exit Policy

Top-level guard, blocker, and coordination codes become typed kernel enums with
their existing serialized strings preserved. CLI renders the stable code and
keeps exit code `0` for success, `1` for blocked/busy or execution failure, and
the existing parser-owned `2` for command usage errors. Do not assign numeric
exit codes per blocker family and do not add a new JSON CLI mode in `#118`.

### 41. Unused Guard Operations

Remove `unbind-cluster-route`, `shutdown-runtime`, and
`remove-runtime-resource` until real mutation use cases exist. A future variant
must arrive with its caller, validator, lock derivation, adapter mapping, and
behavior tests. Runtime generation lifecycle operations retain their separate
ownership vocabulary and must not be duplicated in the guard merely for future
logging.

## Execution Order

1. Close `R9` by restoring composition-only modules and splitting the oversized
   supervisor source without changing behavior.
2. Close `R10` and `R14` together by isolating one validator per executable
   operation and removing unused operation variants.
3. Close `R11` through full orchestration-boundary injection, typed code enums,
   structured CLI projection, and compatibility tests.
4. Close `R12` through injected watcher dependencies, deterministic lifecycle
   tests, and non-gating polling-cost evidence.
5. Close `R13` by aligning contracts and the lock matrix, then updating issue
   and project tracking.
6. Rerun focused, workspace, Python, formatting, and documentation checks, then
   verify the native Windows workflow.
7. Mark `#118` and this plan complete only after every closeout gate passes;
   parent `#113` remains blocked until then.

## Implementation Evidence

- runtime launch adapters now make spawn, metadata, and startup-probe failures
  injectable; focused tests cover spawn failure, metadata failure, health
  failure, verified cleanup, failed cleanup retention, matching-worker
  adoption, and rejection of a mismatched process token;
- reconciliation quarantines only malformed or unreadable records from the
  ownership store; a live but unverifiable PID, token, identity, or health
  mismatch remains a fail-closed diagnostic;
- `LeasedBody` owns request leases across headers, data, errors, trailers,
  cancellation, and drop; a real-listener test proves graceful streaming drain
  while new admission is rejected;
- model capability, adapter rebind/removal, and LoRA plan/run creation use the
  shared three-attempt typed stabilization protocol; focused tests cover
  capability and adapter operation/key drift plus model, dataset, and resume
  adapter changes after train-run dependency locking;
- ownership, daemon metadata, and holder metadata use one atomic-write utility;
  Windows replacement is isolated behind `tentgent-platform-fs` and compiles
  for `x86_64-pc-windows-msvc`;
- `.github/workflows/runtime-ownership-windows.yml` verifies the locked
  workspace with Rust 1.81 before running repeated native Windows replacement
  and ownership-transition tests on pull requests;
- cluster/server inspect use kernel-scoped safe projections, REST keeps legacy
  summary fields, CLI rendering consumes the projection, and doctor remains on
  the global compact summary.
- platform replacement logic lives in a focused implementation module;
  composition files contain declarations and re-exports only, and model-daemon
  supervisor subprocess tests live in a dedicated test module;
- every executable guard operation has one validator file, while shared scans
  remain in probes and dispatch remains in the registry; unused placeholder
  operations were removed;
- guard, coordination, ownership, process, health, model-catalog, and server
  identity dependencies are injected at orchestration boundaries, with
  standard constructors preserving file-backed composition;
- CLI guarded mutations use one typed projector, REST guarded mutations use
  one `RestError` conversion, and typed codes preserve their prior serialized
  strings;
- watcher tests use synthetic ticks. The July 17, 2026 macOS development
  measurement observed 10,000 unchanged metadata probes in about 21 ms
  (approximately 2,140 ns each); this is non-contractual evidence, not a
  timing threshold;
- spawned model workers receive the resolved `TENTGENT_DATA_ROOT`, and Python
  managed-model lookup uses that root. A focused Python regression covers a
  control home separated from the existing managed model store;
- route-claim creation retains cluster, server, target, maintenance, and claim
  transition protection, while retire/release uses maintenance plus the claim
  key. A focused lock regression proves release can complete while server stop
  holds the server transition lock. A separate busy-once regression proves an
  unsuccessful durable release remains tracked until retry succeeds;
- the opt-in local cluster smoke validates all five local routes, scoped
  ownership, policy hot reload, zero active request leases, clean route-claim
  release, and stale generation recovery without provider auth or Keychain
  access.
- the dependency graph now honors the workspace's declared Rust 1.81 minimum;
  direct dependency bounds prevent known edition-2024 drift, the lockfile pins
  compatible transitive releases, and CLI optional filters use an equivalent
  Rust 1.81 expression.

The final local verification matrix passes:

- `cargo test --workspace`: CLI `108`, daemon `306`, and kernel `434` tests
  passed; one watcher measurement, one subprocess lock-holder helper, and seven
  manual macOS Keychain probes remain intentionally ignored in their parent
  suites, and the helper passes when launched by its parent test;
- `uv run --project python/tentgent-model-runtime pytest`: `27/27` passed;
- `cargo check --workspace` passed;
- `cargo +1.81.0 check --workspace` and `cargo +1.81.0 test --workspace`
  passed;
- `cargo check -p tentgent-platform-fs --target x86_64-pc-windows-msvc`
  passed;
- `cargo +1.81.0 check -p tentgent-platform-fs --target
  x86_64-pc-windows-msvc` passed;
- focused resource coordination, resource guard, runtime ownership,
  model-daemon, adapter, train, cluster server, REST, and CLI tests passed;
- remediation code introduces no new strict-Clippy warning; the repository
  still has 27 pre-existing warnings outside this slice;
- `scripts/test-cluster-runtime-ownership-smoke.sh` passed against the local
  five-model fixture set without reading provider credentials or Keychain;
- `scripts/test-release-readiness.sh`, targeted Rust formatting checks for all
  files except the unchanged `#105` manual Keychain probe, and
  `git diff --check` passed. The repository-wide formatter still reports only
  that pre-existing probe baseline.

All remediation findings pass the final local behavior, structure, and Rust
1.81 compatibility matrix. The native Windows workflow rerun remains the final
closeout gate. This document and `#118` must not be marked complete before it
passes.

## Completion Gates

- [x] Every first-audit finding `R1`-`R8` has focused local regression
  coverage.
- [x] Closeout findings `R9`-`R14` are implemented and locally verified.
- [x] Completion-smoke findings `R15`-`R17` are remediated and covered by
  focused regressions plus a real five-route local smoke.
- [x] MSRV finding `R18` is remediated and the full workspace compiles and
  tests with Rust 1.81 locally.
- [x] Existing direct local/cloud server and one-shot runtime behavior remains
  unchanged under the workspace suite.
- [x] `cargo test --workspace` and all Python runtime tests pass.
- [x] Real cluster streaming and stop tests prove lease and drain lifetime.
- [x] Cross-process guard tests prove operation/key-set stabilization.
- [ ] Windows repeated atomic replacement is verified on a native Windows
  runner or host.
- [x] Cluster/server inspect show only scoped safe ownership details; doctor
  stays compact.
- [x] `git diff --check` passes and no new strict-Clippy warning is introduced
  by the remediation.
- [x] The affected contracts and active plans match the final implementation.
- [x] GitHub issue `#118` and the Tentgent Roadmap fields reflect active
  closeout status.
