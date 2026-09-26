# Issue #132: Server Runtime Option Contract

Status: decisions settled; ready for implementation one reviewed step at a time.
Implementation not started.

Issue: [#132](https://github.com/HiroLiang/tentserv-agent/issues/132)

Branch: `bug/132-server-runtime-option-noops`

Milestone: `v1.2.0`

## Execution Baseline

Checked on `2026-09-26`: issue #132 is open in Tentgent Roadmap with the `bug`,
`area:api`, `area:runtime-profile`, and `type:implementation` labels. Reuse the
branch above. It contains `origin/main` at `f41a96d` plus planning commits
`fbd10ea` and `c7e8800`; the working tree was clean and matched its remote.
No PR is open for this branch. Keep the existing issue, labels, project, and
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

Keep one issue and branch. Once implementation is requested, complete one step,
provide its commit/diff, focused test results, and remaining risks, then stop
for review before the next step. A step must compile and include its own tests
and affected contract/user docs. Do not merge intermediate steps to `main` or
publish a release as part of implementation. Step 0 is this planning checkpoint.

| Step | Slice | Depends on | Decisions |
| --- | --- | --- | --- |
| 0 | Final plan and tracking alignment | None | D1-D9 |
| 1 | Target option validation and legacy compatibility | 0 | D4-D6, D9 |
| 2 | Python managed preload operation and cleanup | 1 | D1, D7 |
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

### Step 2: Python Preload

- Add focused preload task/request modules under `runtime/task/` and
  `runtime/server/`; wire `server/routes/lifecycle.py` to TaskManager and the
  bound model. Reuse ResourceManager leasing and scoped failed-load cleanup.
- Tests in `test_runtime_lifecycle.py`, `test_server_bound_models.py`, and a
  focused preload test file cover actual load, idle 0/positive release, clean
  retry after failure, concurrency, closing/generation rejection, and a dropped
  wait while a blocking load remains correctly tracked.
- Review result: the internal endpoint has an independently tested request,
  response, activity, and resource contract. Fake backends avoid model downloads.

### Step 3: Local Startup And Readiness

- Add a typed preload operation beside kernel `runtime/infra/model_daemon/`
  supervisor/health adapters. Carry load mode through both hidden hosts into
  daemon `server/local/runtime.rs`; add explicit startup/readiness state and
  guards for public fallback forwarding.
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

### Step 4: Cluster Startup

- Extend `server/cluster/{state,runtime}.rs`; separate route resolution from
  request admission. Resolve configured local routes from one snapshot, obtain
  claims, deduplicate exact physical keys, and reuse Step 3's preload operation.
- Test all local routes, equal/unequal identity keys, route-specific failures,
  no partial readiness, bind/stop cleanup, zero preload calls in lazy mode,
  and existing provider-route limitations. Do not add image routes.
- Review result: an eager Cluster is ready only after its local routes load;
  claims and active work remain protected on partial failure.

### Step 5: Cluster Snapshot Staging

- Split `server/cluster/cache.rs` into committed reads, candidate reads, and
  conditional promotion. Update `state.rs`, `router.rs`, and watcher seams so
  health or ordinary requests cannot independently publish an eager candidate.
- Test immutable old reads while a candidate is pending, failed/invalid
  candidate retention of the old eager snapshot, compare-before-promote, and
  preserved lazy invalid-definition behavior. Keep this review focused on
  snapshot state; wire asynchronous preload in Step 6.
- Review result: no candidate can become routable before explicit promotion.

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

### Step 7: Integration And Evidence

- Reconcile `docs/contracts/{model-runtime-server,http-daemon,cluster,
  server-runtime-profile,api-surface-stability}.md`, `docs/user/{servers,
  clusters,runtime,version}.md`, and relevant image guides with implemented
  behavior. Remove the now-fixed no-op limitations and add upgrade recovery.
- Run Rust formatting/check/workspace tests, Python runtime tests, and release
  readiness checks after all slices. Each earlier step runs only its affected
  test modules and compile checks; do not repeatedly run the complete matrix.
- Live smoke uses an existing small local model: eager with model idle 0 and
  positive retention, lazy first request, runtime reuse/restart, and health
  polling without retention. Use fake multi-route models for deterministic
  reload/race tests; do not require paid providers or all image workflows.
- Record exact commands, model/backend, exit results, resource counts and
  process/ownership cleanup. Commit evidence and prepare one final #132 PR.

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
- #127 proof v2 can progress independently. Land #132 before #128 server/Cluster
  tuple gates and #130 diagnostics to avoid conflicting edits.
- #129 adapter/load identity remains responsible for adapter-specific preload
  and proof semantics. This issue validates only the base model for each route.
- Do not add Cloud retention, model prefetch, new public lifecycle controls,
  new backend families, or release publication in this issue.

## Completion

- [x] Settle D1-D9, including explicit lazy for Diffusers and MLX/MFLUX images.
- [x] Confirm current issue/branch and define independently reviewable slices.
- [ ] Complete Steps 1-7, stopping for review at each checkpoint.
- [ ] Every accepted decision D1-D9 is implemented and verified.
- [ ] Each review step records its focused test result and remaining risk.
- [ ] All #132 issue acceptance criteria pass and user-facing docs match.
- [ ] PR review and merge are complete; #132 can then be closed.
