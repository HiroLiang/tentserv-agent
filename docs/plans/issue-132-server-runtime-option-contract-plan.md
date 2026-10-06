# Issue #132: Server Runtime Option Contract

Status: Steps 1-7, Rust 1.99.0 and release hardening are merged through
[PR #139](https://github.com/HiroLiang/tentserv-agent/pull/139) and released in
`v1.1.2`. Native, published stable and Homebrew installation gates passed.
Issue #132 is closed; its existing tracking milestone remains unchanged.
See [validation evidence](./issue-132-validation-evidence.md) and the
[release checklist](./bugfix-maintenance-plan.md#release-reassessment-2026-10-03).

Issue: [#132](https://github.com/HiroLiang/tentserv-agent/issues/132)

Branch: `bug/132-server-runtime-option-noops`

Milestone: `v1.2.0`

## Execution Baseline

Checked on `2026-09-26`: issue #132 is open in Tentgent Roadmap with the `bug`,
`area:api`, `area:runtime-profile`, and `type:implementation` labels. Reuse the
branch above. It contains `origin/main` at `f41a96d` plus planning commits
`fbd10ea`, `c7e8800`, and `0bde241`; implementation began from a clean working tree.
Step 2 began from clean Step 1 commit `8a28a68`, without pushing or changing branches.
Step 3 began from clean Step 2 commit `bb283fa`; the user resumed it after a
pause on `2026-09-28`. Keep each implementation slice in an independent commit.
Step 4 began from clean Step 3 commit `0316f41` on the same branch.
Closeout review on `2026-10-02` found two `Option::is_none_or` calls that do not
compile under the old declared Rust 1.81 minimum. The user approved raising
the local, workspace, CI/release and minimum compiler baseline together, then
selected stable Rust 1.99.0. Revalidation completed on `2026-10-03`: debug and
release each passed 917 tests, all 24 subprocess cases and three real MLX
cases passed. Edition 2021 and the dependency lockfile are unchanged. Keep
this upgrade in an independent commit before closing #132.
Keep the existing issue, labels, project, and
milestone; no new implementation branch or child issue is needed.

## Problem And Boundary

Local and Cluster specs retain `lazy_load`, but the CLI and daemon runtime
handlers discard it and the managed Python launcher always uses `--lazy-load`.
Cloud accepts local-runtime lifecycle fields although its worker has no local
model or managed Python runtime. This is a follow-up to the completed #131
model/runtime idle fix. It does not change #131's two idle clocks, proof v2,
adapter loading, or Cloud provider session policy.

The existing `server run` CLI flag defaults to `lazy_load=false`. Once fixed,
omitting `--lazy-load` means eager start for supported targets; lazy-only
image-generation targets reject that choice under D9. This changes observable
startup time and failure timing for existing commands and stored specs.

## Accepted Decisions

| ID | Decision | Observable result |
| --- | --- | --- |
| D1 | `lazy_load=true` delays model loading; `false` eagerly validates loading at server start. Eager with `model_idle_seconds=0` loads and then releases the model immediately after its final lease. | A positive model idle is required to keep the model warm. Runtime idle remains the independent #131 policy. |
| D2 | Eager Cluster start covers every configured local route and deduplicates by physical runtime identity. | Any local route that cannot be resolved or loaded fails startup with route-specific diagnostics. Missing unconfigured routes are allowed; declared provider routes retain their existing unsupported execution behavior. |
| D3 | Eager Cluster hot reload prepares and preloads a new definition before making it routable. | On preparation failure, the old definition and generation keep serving; a successful switch retires old claims after in-flight work drains. |
| D4 | New Cloud CLI and REST create/run inputs reject explicit `lazy_load`, `runtime_idle_seconds`, `model_idle_seconds`, and legacy `idle_seconds` fields. | No Cloud lifecycle input is silently ignored. An omitted CLI flag or absent REST field is allowed. REST must distinguish absent `lazy_load` from explicit `false`. |
| D5 | Existing Cloud specs remain readable and startable under their stored `server_ref`; existing fields are not rewritten. | Inspect identifies their lifecycle fields as legacy, ignored, and not applicable. New Cloud specs use canonical absent/default values with the existing identity algorithm, so default-valued legacy specs keep deduplicating. |
| D6 | Existing Local/Cluster specs honor their stored `lazy_load=false` as eager after upgrade. | No silent compatibility exception preserves the old accidental lazy behavior. |
| D7 | Load mode is a startup action, not a physical runtime ownership policy. | A reused generation still receives an explicit preload request for eager start; #131's first-spawner idle policy remains authoritative. |
| D8 | #132 precedes #128 and #130 because they share server/Cluster gates, diagnostics, and docs. #127 can proceed independently. | #132 stays a maintenance issue, not a child of compatibility parent #126. |
| D9 | All currently supported `image-generation` backends (Diffusers and MLX/MFLUX) require `lazy_load=true`; do not preload every workflow or choose one implicitly. This is an explicit exception to D1/D6. | CLI requires `--lazy-load`; REST requires `lazy_load:true`. Reject eager create/run and stored-spec start before launching a worker, with a corrective message. Existing specs remain readable and retain their refs. |

## Image-Generation Boundary

Python can infer one preload model kind for Chat, Embedding, Rerank, audio,
vision, and video. `image-generation` selects its model kind from the request's
workflow (text-to-image, image-to-image, inpaint, or control); the server spec
does not choose one. The current Python preload path therefore skips it. A
generic eager start cannot truthfully claim that the image model was loaded.
In addition, both Diffusers and MFLUX `load()` only initialize model metadata;
their actual pipeline/weight preparation happens inside image generation.

Accepted on `2026-09-26`: Diffusers and MLX/MFLUX stay lazy-only in #132. Load
only the workflow required by the incoming request. A new image server without
`--lazy-load` (or REST `lazy_load:true`) must fail validation. Starting an old
image spec with `lazy_load=false` also fails with instructions to
create a lazy spec using the same model and desired server settings. Do not
rewrite its stored ref or silently coerce its load mode.

Workflow-aware image preloading and backend preparation are deferred. Never
report eager success after skipping actual preload.

Select D9 by the resolved Local `image-generation` capability, including an
inferred capability. Do not infer it from a filename extension. Cluster has no image route;
this decision does not add one.

## Settled Internal Design

- Keep the stored `lazy_load` boolean and current identity encoding. Map it to
  kernel-owned `LoadMode::{Lazy,Eager}` intent. Validate new inputs separately
  from stored-spec readability and launchability. Old image eager specs must
  still support list, inspect, and remove. Existing invalid Cloud model-idle
  specs do not become valid as part of this compatibility rule.
- Preserve REST field presence until target validation: Cloud rejects each
  lifecycle field even when its value is `false`, `0`, or `null`. Existing
  Local/Cluster null/default handling and #131 alias rules remain unchanged.
- Keep managed Python process launch lazy. Its existing 20-second health and
  identity check is followed by internal `POST /v1/lifecycle/preload` for eager
  startup, including when a generation is reused. Do not expose that operation
  through the public proxy fallback. Request: unique `task_ref` and expected
  `process_token`; the runtime supplies its own bound model and capability.
- Preload runs as a `RuntimeTask` through `TaskManager` and acquires a normal
  model lease. Return `status: done`, task/model/capability identity and process
  token only after real loading. Reject wrong generation, closing/shutdown,
  unbound, and unsupported/image targets. Older runtimes missing the endpoint
  return an actionable bootstrap diagnostic. No arbitrary model path or adapter
  selection is accepted by this operation.
- Use a named internal 300-second preload wait budget per physical runtime,
  independent of the two idle clocks; do not add a new CLI option. Timeout or
  disconnected HTTP wait does not interrupt a Python loader thread. Keep work
  and resource protection active until it finishes; then release the lease.
  A failed load must clean up its partial resource so retry is safe, including
  positive model idle. Never use global `release_all()` or kill a shared
  generation as cleanup for one failed preload.
- Eager readiness means loading completed, even if model idle 0 immediately
  releases the model. Preserve asynchronous start semantics: CLI detached
  observation expiry and REST `wait_ready=false` may report starting/process
  accepted, but cannot claim load readiness or write `Verified` proof. Only a
  terminal successful preload may do that. REST readiness wait expiry remains
  an observation timeout, not proof of model incompatibility. Record the actual
  worker outcome using the existing proof schema, without duplicate conflicting
  caller writes. Health distinguishes starting from ready; traffic is admitted
  only after ready. `running` continues to describe process existence.
- Cluster prepares one definition snapshot and loads unique physical identities
  sequentially; identity includes model, capability, and resolved profile.
  Earlier models may be released while later ones load. This is not a promise
  that every model stays resident simultaneously.
- Eager reload separates committed and candidate snapshots. Requests and health
  read only the committed snapshot. Preload outside locks; compare the base and
  candidate revisions before atomic promotion. Staged claims must survive old
  traffic; failed/superseded candidates release only their own claims after
  active work is safe. Admission validates the selected committed revision or
  retries after promotion; a late request cannot re-enter a retired snapshot.
  Preserve old streaming leases until completion/drop.
  Lazy reload retains its existing admission/invalid-definition behavior.

## Reviewable Steps

Keep one issue and branch. On `2026-10-01` the user authorized completing the
remaining steps with internal review/test checkpoints and independent commits,
then handing them back together for human review. A step must compile and include its own tests
and affected contract/user docs. Do not merge intermediate steps to `main` or
publish a release as part of implementation. Step 0 is this planning checkpoint.

| Step | Slice | Depends on | Decisions |
| --- | --- | --- | --- |
| 0 | Final plan and tracking alignment | None | D1-D9 |
| 1 | Target option validation and legacy compatibility | 0 | D4-D6, D9 |
| 2 | Python managed preload, cleanup, and public lifecycle isolation | 1 | D1, D7 |
| 3 | Local eager/lazy startup and truthful readiness | 2 | D1, D6, D7 |
| 4 | Cluster eager startup | 3 | D2 |
| 5 | Cluster candidate/committed snapshot boundary | 4 | D3 |
| 6 | Cluster preload-before-promotion and safe drain | 5 | D3, D7 |
| 7 | Integration, live evidence, and closeout | 1-6 | D1-D9 |

### Step 1: Options And Legacy Specs

- Add the kernel load-mode mapping and one target-aware validation rule.
  Preserve explicit input presence; remove Cloud lifecycle flags from both
  hidden workers and generated launch arguments. Add Cloud applicability
  diagnostics without deleting existing raw response fields.
- Touch kernel `features/server/{domain,infra/runtime,infra/identity}.rs` and
  `usecases/{port,common}.rs`; CLI `cli/commands/server.rs`, `cli/server.rs`,
  `cli/server/render.rs`; daemon `main.rs`, `handlers/rest/server/{lifecycle,dto}.rs`;
  Python runtime config/CLI image guard. Do not tighten spec deserialization to
  reject previously readable image eager specs.
- Verify Cloud omitted vs true/false/null/all idle fields, canonical Cloud ref
  reuse, legacy Cloud read/start without rewriting, and both image backends'
  new/stored eager rejection. Cover inferred capability, `allow_unverified`,
  hidden entry points, and unchanged Local/Cluster idle validation.
- Review result: supported options and recovery messages are unambiguous;
  Local/Cluster eager execution still awaits later steps.

Step 1 evidence (`2026-09-26`): input presence uses `LifecycleInput` without
changing persisted boolean/identity shapes. Cloud detailed responses add
`lifecycle_options_applicability: "not_applicable_legacy_ignored"`.
Kernel server tests: 57 passed; CLI server-filtered tests: 14 passed; daemon
server-filtered tests: 119 library + 2 worker tests passed, 1 existing manual
watcher benchmark ignored. Python image guard/lifecycle/bound-model tests:
33 passed. Commands: `cargo test -p tentgent-kernel features::server --lib`,
`cargo test -p tentgent-cli server`, `cargo test -p tentgent-daemon server`, and
the Python command below plus `tests/test_image_lazy_only.py`.
`cargo check --workspace --all-targets`, `cargo fmt --all -- --check`, and
`git diff --check` passed. No real models were loaded; eager/resource smoke is
still deferred to the later slices. No preload, readiness, reload, or #131 idle
implementation changed. The user subsequently authorized Step 2.

### Step 2: Python Preload

- Add focused preload task/request modules under `runtime/task/` and
  `runtime/server/`; wire `server/routes/lifecycle.py` to TaskManager and the
  bound model. Reuse ResourceManager leasing and scoped failed-load cleanup.
- Enforce strict payload/generation checks and unsupported metadata-only
  backends. Preserve queued/running tasks after wait timeout/cancellation;
  quarantine cleanup failures without global release or disabling idle shutdown.
- Move public Local lifecycle namespace rejection forward from Step 3, as
  approved: reject before runtime resolution, generation creation, or proof writes.
- Tests cover load/reuse, idle 0/positive, clean retry, reserved waiters,
  quarantine, admission errors, and abandoned waits with tracked native loading.
- Review result: the internal endpoint has an independently tested request,
  response, activity, and resource contract. Fake backends avoid model downloads.

Step 2 evidence (`2026-09-26`): see the [managed preload contract](../contracts/model-runtime-server.md#managed-preload).
Full Python runtime suite: 114 passed plus 11 subtests. Rust Local: 33 passed;
Cluster: 17 passed, 1 existing manual benchmark ignored. Run
`uv run --project python/tentgent-model-runtime pytest python/tentgent-model-runtime/tests -q`
and the Local/Cluster commands below with `--lib`. Local socket tests required
execution outside the sandbox; all passed on rerun. Workspace all-target checks,
Rust/Python formatting, and diff checks passed. Fake backends only: no model
download, GPU/RSS smoke, readiness/proof wiring, or Cluster integration in this
slice. The user subsequently authorized Step 3; no push, merge, or release.

### Step 3: Local Startup And Readiness

- Add a typed preload operation beside kernel `runtime/infra/model_daemon/`
  supervisor/health adapters. Carry load mode through both hidden hosts into
  daemon `server/local/runtime.rs`; add explicit startup/readiness state and
  readiness admission guards (lifecycle namespace isolation is done in Step 2).
- Update CLI background observation and daemon REST server health/start proof
  recording. A 10-second CLI observation or 30/120-second REST readiness wait
  must not turn unfinished preload into successful verification or failure proof.
  Reuse the current proof format; keep runtime-generation idle policy intact.
- Test first spawn and reuse, lazy first-request load, eager load failure and
  timeout, stale Python endpoint, startup longer than caller observation,
  CLI foreground/detached and REST waiting/non-waiting modes. Validate cleanup
  without terminating another owner's generation.
- Review result: Local startup genuinely honors the stored mode and reports
  process state separately from completed eager readiness.

#### Step 3 Evidence (`2026-09-28`)

- Typed Rust preload client with unique task refs, completion identity checks,
  305-second transport budget, and separate load/observation/unavailable errors.
- Both hidden Local hosts carry load mode. Local eager startup binds health in
  `starting`, blocks inference with 503, preloads new/reused endpoints, then
  becomes ready. Lazy startup skips model-runtime resolution/preload.
- Worker-owned terminal preload proof; removed CLI/REST launch-based proof
  writes. Shared runtime termination and #131 idle policy remain unchanged.
- CLI reports still-starting after its 10-second observation; REST distinguishes
  readiness from reachability and re-inspects process state after waiting.
- Updated affected contracts and user/developer documentation. Cluster edits only
  initialize the new Local state fields; Cluster eager behavior is not implemented.

Completed test runs:

| Command | Result |
| --- | --- |
| `cargo test -p tentgent-kernel model_daemon --lib` | 15 passed, including 5 preload and supervisor/identity regressions |
| `cargo test -p tentgent-kernel features::server --lib` | 57 passed |
| `cargo test -p tentgent-cli server` | 15 passed; includes real 10-second observation expiry |
| `cargo test -p tentgent-daemon server` | 129 library + 2 worker tests passed, including 38 Local tests; 1 existing manual benchmark ignored |
| `uv run --project python/tentgent-model-runtime pytest python/tentgent-model-runtime/tests -q` | 114 passed + 11 subtests |
| `uv run --project python/tentgent-model-runtime python scripts/test-local-server-startup.py` | 8 subprocess integration tests passed |

The integration script uses built CLI/daemon binaries, the real Python HTTP,
TaskManager and ResourceManager, and an instrumented fake chat backend in
temporary homes. It verifies first spawn/reuse, foreground/detached and REST
waiting/non-waiting starts, lazy first-request loading, model idle 0/positive
release, health polling, terminal failure, and timeout/stop while loading.
Both abandoned-start scenarios preserve Python work until its lease releases;
only confirmed worker completion writes proof. The timeout scenario shortens
only the fixture's Python observation budget, not production's 300-second value.

Workspace all-target check, both binary builds, Rust formatting, script Ruff,
and diff checks passed without compile warnings. Socket/process tests ran
outside the sandbox. Initial fixture path/profile/metadata assumptions were
corrected before passing reruns. No models were downloaded; real backend/GPU/RSS
and native cross-platform integration remain Step 7 evidence, not claimed here.
Stop at Step 3 review; no push, merge, release, architecture cleanup, or
stale-ownership remediation in this slice.

### Step 4: Cluster Startup

- Extend `server/cluster/{state,runtime}.rs`; separate route resolution from
  request admission. Resolve configured local routes from one snapshot, obtain
  claims, deduplicate exact physical keys, and reuse Step 3's preload operation.
- Test all local routes, equal/unequal identity keys, route-specific failures,
  no partial readiness, bind/stop cleanup, zero preload calls in lazy mode,
  and existing provider-route limitations. Do not add image routes.
- Review result: an eager Cluster is ready only after its local routes load;
  claims and active work remain protected on partial failure.

#### Step 4 Evidence (2026-09-28)

The Cluster worker now honors stored load mode in both hosts. Startup resolves
one snapshot before claiming/loading routes, deduplicates full physical keys,
and preloads sequentially. Health remains `starting` and inference returns
`503 cluster_starting` until all local routes complete. The watcher starts only
after readiness; a definition change during startup fails the startup check.
Lazy startup does not resolve/preload routes. Provider execution stays unsupported.

Listener bind precedes preload. Terminal failures clean completed claims;
explicit pre-admission rejection releases claims without failed proof. Stop
observes current work for the existing 30-second drain budget, starts no next
route, and never publishes readiness afterward. Uncertain completion retains
claims for reconciliation rather than terminating shared Python work. Per-route
terminal evidence reuses Local's worker-owned proof boundary.

| Command | Result |
| --- | --- |
| `cargo test -p tentgent-daemon server` | 139 library + 2 worker tests passed; 1 existing manual benchmark ignored. Includes 27 Cluster tests. |
| `cargo test -p tentgent-kernel features::server --lib` | 57 passed |
| `cargo test -p tentgent-kernel runtime_ownership --lib` | 14 passed |
| `cargo test -p tentgent-kernel model_daemon --lib` | 16 passed |
| `cargo test -p tentgent-cli server` | 15 passed, including Local/Cluster readiness classification |
| `uv run --project python/tentgent-model-runtime pytest python/tentgent-model-runtime/tests -q` | 114 passed + 11 subtests |
| `uv run --project python/tentgent-model-runtime python scripts/test-local-server-startup.py` | 8 subprocess integration tests passed in the final isolated rerun |
| `uv run --project python/tentgent-model-runtime python scripts/test-cluster-server-startup.py` | 11 subprocess integration tests passed |

The Cluster subprocess suite uses both built Rust hosts and real Python
HTTP/TaskManager/ResourceManager with fake backends in isolated temporary homes.
It covers all five local routes, eager/lazy, foreground/detached, REST wait
expiry, reused generations/first-spawner policy, zero/positive model idle,
partial failure, pre-admission rejection, bind failure, preload timeout, and
stop during loading including the real 30-second drain budget. Rust tests also
cover exact-key grouping, provider limitations, snapshot changes and active
stream leases. Fixture fixes addressed colliding temporary names, media assets,
current/history proof counting, log paths, and macOS nonblocking socket reads.
One concurrent smoke rerun hit two Local wait timeouts; their cause was not
captured before cleanup. The final isolated Local rerun passed all eight tests.
Run the suites sequentially because they share host TCP ports; failed fixtures
now print worker logs before cleanup. Concurrent-suite reliability is not claimed.

Workspace all-target check, binary builds, Rust formatting, script Ruff, and
diff checks passed without compile warnings. No models were downloaded. Real
backend/GPU/RSS and cross-platform integration remain Step 7 evidence. Eager
hot reload remains Steps 5-6: do not release this intermediate branch. Stop at
Step 4 review; no push, merge, release, or stale-ownership remediation.

### Step 5: Cluster Snapshot Staging

- Split `server/cluster/cache.rs` into committed reads, candidate reads, and
  conditional promotion. Update `state.rs`, `router.rs`, and watcher seams so
  health or ordinary requests cannot independently publish an eager candidate.
- Test immutable old reads while a candidate is pending, failed/invalid
  candidate retention of the old eager snapshot, compare-before-promote, and
  preserved lazy invalid-definition behavior. Keep this review focused on
  snapshot state; wire asynchronous preload in Step 6.
- Review result: no candidate can become routable before explicit promotion.

Step 5 checkpoint (`2026-10-01`, base `f6b1780`): separated committed reads,
candidate reads and conditional promotion with monotonic base revisions (ABA
protection) and a final disk hash check. Eager request/health/watcher reads cannot
publish a candidate; lazy invalid-reload behavior remains unchanged. Five new
tests cover staging, supersession/ABA, rejected promotion, invalid files,
block policy, and eager health retention. `cargo test -p tentgent-daemon
server::cluster --lib`: 32 passed, 1 existing manual benchmark ignored.
Workspace all-target check, format/diff checks and internal diff review passed.
Eager watcher preparation remains deliberately unwired until Step 6.

### Step 6: Cluster Reload And Drain

- Wire asynchronous preparation into `server/cluster/watch/{port,runner}.rs`
  and `leases.rs`. Separate staged/active/retiring claim handling; never hold
  a mutex or cross-process transition lock while awaiting preload.
- Test old traffic during preload/failure, candidate claims surviving old
  traffic, successful switch with old streaming leases, B superseded by C,
  repeated failed revisions, and stop during preload. Pause a request between
  snapshot selection and lease acquisition, promote the candidate, and verify
  admission retries against the committed revision. Expose a failed candidate
  diagnostic while retaining the committed eager definition.
- Update the eager-specific invalid-reload contract in `docs/contracts/cluster.md`.
  Existing drain policy guards, watcher hash checks, and shared ownership must
  remain covered.
- Review result: failed or stale candidates cannot replace working routes;
  completed promotion drains only the previous generation.

Step 6 checkpoint (`2026-10-01`, base `1217166`): added asynchronous staged
preload, compare-before-promotion, admission retry, committed-route fallback,
failed-candidate diagnostics, and bounded stop observation. Staged claims are
isolated from old traffic; retired claims retry transient busy releases even
without another revision. No lock spans a preload await. Cluster tests: 40
passed, 1 existing manual benchmark ignored. Five subprocess reload tests passed
with real Python lifecycle and fake models, including old streaming responses,
partial failure/recovery, supersession, stop, and unknown-completion retention.
The initial stream test incorrectly expected concurrent generation on a backend
held by a long stream; its ordering was corrected, not the backend serialization.
Workspace all-target check, binary builds, formatting, Ruff, diff review passed.
Real-model/GPU and complete regression evidence remain Step 7.

### Step 7: Integration And Evidence

- Reconcile `docs/contracts/{model-runtime-server,http-daemon,cluster,
  server-runtime-profile,api-surface-stability}.md`, `docs/user/{servers,
  clusters,runtime,version}.md`, and relevant image guides with implemented
  behavior. Remove the now-fixed no-op limitations and add upgrade recovery.
- Run Rust formatting/check/workspace tests, Python runtime tests, and release
  readiness checks after all slices. Each earlier step runs only its affected
  test modules and compile checks; do not repeatedly run the complete matrix.
- Live smoke uses one authorized small test model: eager with model idle 0 and
  positive retention, lazy first request, runtime reuse/restart, and health
  polling without retention. Use fake multi-route models for deterministic
  reload/race tests; do not require paid providers or all image workflows.
- Record exact commands, model/backend, exit results, resource counts and
  process/ownership cleanup. Commit evidence and prepare one final #132 PR.

Step 7 checkpoint (`2026-10-01`, base `3346c5e`): complete regression matrix,
24 subprocess integration cases, and three real MLX smoke cases passed.
Added eight image workflow dispatch/lease tests and two CLI help regressions;
reconciled contracts, user docs and Unreleased notes. All D1-D9 and GitHub #132
acceptance criteria are mapped in the [validation record](./issue-132-validation-evidence.md),
including commands, cleanup, limits and a draft PR message. User-authorized
download remains only in the ignored test store. No external publication.

Focused commands (run the relevant subset for each slice):

```bash
cargo test -p tentgent-kernel features::server
cargo test -p tentgent-kernel model_daemon::supervisor
cargo test -p tentgent-cli server
cargo test -p tentgent-daemon server::local
cargo test -p tentgent-daemon server::cluster
cargo test -p tentgent-daemon transport::rest::tests::datasets_and_servers
uv run --project python/tentgent-model-runtime pytest python/tentgent-model-runtime/tests/test_runtime_lifecycle.py python/tentgent-model-runtime/tests/test_server_bound_models.py
git diff --check
```

Add each new preload test module to its focused command and confirm tests
actually execute; a zero-test filtered run is not evidence.

## Verification Matrix

| Scenario | Expected evidence |
| --- | --- |
| Local lazy and eager, new and stored specs | First request triggers lazy load; eager start proves load before ready; eager load failure fails start. |
| Diffusers and MLX/MFLUX image targets | New CLI/REST eager requests and old eager spec starts fail before worker launch. Explicit lazy succeeds, starts without loading a pipeline, and loads only the requested workflow on first use. Non-image targets remain eligible for eager. |
| Eager with model idle 0 and positive value | Resource count returns to zero for 0; stays available until the positive idle expires. Runtime process follows its separate idle policy. |
| Shared runtime reuse | Eager preload runs even without spawning a new Python process; ownership policy is unchanged. |
| Cluster multiple routes and reload | All valid routes checked, shared physical runtime loaded once, partial failure blocks promotion, old traffic continues, successful switch drains safely. |
| Cloud new and legacy inputs | Each explicit unsupported field fails on create/run; omitted fields work; old specs retain ref and start; inspect says legacy ignored. |
| CLI, REST, inspect, docs | Same target applicability, defaults, alias handling, and failure descriptions. |

## Dependencies And Out Of Scope

- #131 is merged and released in v1.1.1; preserve its `runtime_idle_seconds=300`
  and `model_idle_seconds=0` defaults, finite policy, and observational health.
- #127 proof v2 can progress independently. The merged #132 prerequisite for
  #128 server/Cluster tuple gates and #130 diagnostics is now satisfied.
- #129 adapter/load identity remains responsible for adapter-specific preload
  and proof semantics. This issue validates only the base model for each route.
- Do not add Cloud retention, model prefetch, new public lifecycle controls,
  or new backend families. The subsequent user-authorized publication is
  tracked by the maintenance release checklist, not an extra #132 feature.

## Completion

- [x] Settle D1-D9, including explicit lazy for Diffusers and MLX/MFLUX images.
- [x] Confirm current issue/branch and define independently reviewable slices.
- [x] Implement and validate Steps 1-3.
- [x] Implement and validate Step 4; retain its review checkpoint.
- [x] Implement, validate and internally review Step 5.
- [x] Implement, validate and internally review Step 6.
- [x] Complete Step 7 with intermediate checks before human review.
- [x] Every accepted decision D1-D9 is implemented and verified within the recorded test scope.
- [x] Each review step records its focused test result and remaining risk.
- [x] All #132 issue implementation acceptance criteria pass and user-facing docs match.
- [x] PR review and merge are complete.
- [x] Installed stable release and Homebrew verification pass; #132 closeout is recorded.
