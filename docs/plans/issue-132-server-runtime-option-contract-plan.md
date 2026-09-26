# Issue #132: Server Runtime Option Contract

Status: core decisions and Diffusers lazy-only behavior accepted. Applicability
to MLX/MFLUX image generation remains to be confirmed. Implementation not started.

Issue: [#132](https://github.com/HiroLiang/tentserv-agent/issues/132)

Branch: `bug/132-server-runtime-option-noops`

Milestone: `v1.2.0`

## Problem And Boundary

Local and Cluster specs retain `lazy_load`, but the CLI and daemon runtime
handlers discard it and the managed Python launcher always uses `--lazy-load`.
Cloud accepts local-runtime lifecycle fields although its worker has no local
model or managed Python runtime. This is a follow-up to the completed #131
model/runtime idle fix. It does not change #131's two idle clocks, proof v2,
adapter loading, or Cloud provider session policy.

The existing `server run` CLI flag defaults to `lazy_load=false`. Once fixed,
omitting `--lazy-load` means eager start for supported targets; the lazy-only
Diffusers image target rejects that choice under D9. This changes observable
startup time and failure timing for existing commands and stored specs.

## Accepted Decisions

| ID | Decision | Observable result |
| --- | --- | --- |
| D1 | `lazy_load=true` delays model loading; `false` eagerly validates loading at server start. Eager with `model_idle_seconds=0` loads and then releases the model immediately after its final lease. | A positive model idle is required to keep the model warm. Runtime idle remains the independent #131 policy. |
| D2 | Eager Cluster start covers every configured, valid local route and deduplicates routes sharing the same physical runtime identity. | Any route that cannot be resolved or loaded fails startup with route-specific diagnostics. No partial ready result. |
| D3 | Eager Cluster hot reload prepares and preloads a new definition before making it routable. | On preparation failure, the old definition and generation keep serving; a successful switch retires old claims after in-flight work drains. |
| D4 | New Cloud CLI and REST create/run inputs reject explicit `lazy_load`, `runtime_idle_seconds`, `model_idle_seconds`, and legacy `idle_seconds` fields. | No Cloud lifecycle input is silently ignored. An omitted CLI flag or absent REST field is allowed. REST must distinguish absent `lazy_load` from explicit `false`. |
| D5 | Existing Cloud specs remain readable and startable under their stored `server_ref`; existing fields are not rewritten. | Inspect identifies their lifecycle fields as legacy, ignored, and not applicable. New Cloud specs use canonical absent/default values with the existing identity algorithm, so default-valued legacy specs keep deduplicating. |
| D6 | Existing Local/Cluster specs honor their stored `lazy_load=false` as eager after upgrade. | No silent compatibility exception preserves the old accidental lazy behavior. |
| D7 | Load mode is a startup action, not a physical runtime ownership policy. | A reused generation still receives an explicit preload request for eager start; #131's first-spawner idle policy remains authoritative. |
| D8 | #132 precedes #128 and #130 because they share server/Cluster gates, diagnostics, and docs. #127 can proceed independently. | #132 stays a maintenance issue, not a child of compatibility parent #126. |
| D9 | Diffusers `image-generation` requires `lazy_load=true`; do not preload every workflow or choose a workflow implicitly. This is an explicit exception to D1/D6. | CLI requires `--lazy-load`; REST requires `lazy_load:true`. Reject eager create/run and stored-spec start before launching a worker, with a corrective message. Existing specs remain readable and retain their refs. |

## Image-Generation Boundary

Python can infer one preload model kind for Chat, Embedding, Rerank, audio,
vision, and video. `image-generation` selects its model kind from the request's
workflow (text-to-image, image-to-image, inpaint, or control); the server spec
does not choose one. The current Python preload path therefore skips it. A
generic eager start cannot truthfully claim that the image model was loaded.
In addition, both Diffusers and MFLUX `load()` only initialize model metadata;
their actual pipeline/weight preparation happens inside image generation.

Accepted on `2026-09-26`: Diffusers stays lazy-only in #132. Load only the
workflow required by the incoming request. A new Diffusers image server without
`--lazy-load` (or REST `lazy_load:true`) must fail validation. Starting an old
Diffusers image spec with `lazy_load=false` also fails with instructions to
create a lazy spec using the same model and desired server settings. Do not
rewrite its stored ref or silently coerce its load mode.

Workflow-aware image preloading and backend preparation are deferred for
Diffusers. MLX/MFLUX has the same preparation gap, but extending D9 to that
backend still requires confirmation. Never report eager success after skipping
actual preload. Settle that remaining applicability before Step 1.

Select D9 by the resolved Local `image-generation` target and Diffusers backend.
Do not infer it from a filename extension. Cluster currently has no image route;
this decision does not add one.

## Implementation Contract

- Keep serialized `lazy_load` compatible. Use one internal load-mode intent
  across spec creation, hidden runtime commands, Local/Cluster proxy startup,
  and restart from stored specs. Do not encode load mode in runtime ownership
  identity or alter #131's canonical idle aliases/defaults.
- Provide a managed, local-only preload operation that returns success only
  after the bound model is actually loaded. Use it after the Python runtime is
  healthy so large models do not consume the existing 20-second process-health
  timeout. Verify the target generation before invoking it; a reused runtime
  must also be preloaded. Bound the preload wait and report a load/timeout
  failure through the existing server-start error path. Avoid treating a
  successful proxy health probe as proof of eager readiness.
- `model_idle_seconds=0` permits immediate release after preload completes;
  this is readiness validation, not a warm-residency promise. Apply D9 to the
  resolved Diffusers image target before starting a runtime. A capability must
  never report eager success after skipping preload.
- Make eager startup failure leave no falsely ready server metadata or leaked
  route claim. Keep existing shared runtime ownership and guarded shutdown
  behavior; do not stop another server's healthy shared runtime on failure.
- For Cluster, resolve the candidate definition's complete local route set,
  preload each unique physical runtime, and atomically promote its generation
  only after all required routes succeed. Keep the prior definition and
  ownership claims intact on failure; preserve active request draining on
  success. A lazy Cluster does not preload during start or reload.
- Validate Cloud fields before creating or launching a new spec. For CLI
  commands, reject flags explicitly supplied for Cloud; for REST, preserve
  field presence (including `lazy_load:false`) until target validation.
  Stored Cloud specs bypass new-input rejection only on read/restart. Keep
  their original refs and display legacy values with an applicability marker.

## Reviewable Steps

One issue and branch; each step should be a focused commit or small commit
series with a concise diff, its own verification notes, and a review checkpoint
before the next step. The plan commit is the first checkpoint.

| Step | Review focus | Changes and proof required |
| --- | --- | --- |
| 0 | Decision and dependency baseline | This plan, active-plan routing, #132 issue decisions, and #128/#130 ordering. No runtime change. |
| 1 | Load-mode contract and preload boundary | Define internal load-mode intent and the managed preload request/result. Show how health identity, timeout, unsupported capability, concurrency, and model-idle release are handled. Focused Python and Rust tests; no Local/Cluster start behavior change yet. |
| 2 | Local end-to-end behavior | Propagate the stored choice through CLI, daemon, kernel, and Local proxy. Eager waits for actual preload; lazy stays on demand. Check new and legacy specs, reused runtime, startup failure cleanup, `model_idle_seconds=0`, positive retention, and Diffusers lazy-only validation/recovery. |
| 3 | Cluster start | Preload all resolved local routes, deduplicate by physical runtime, and fail with a named route when any route is invalid or fails loading. Verify claims and process cleanup; lazy start remains on demand. |
| 4 | Cluster hot reload | Stage a candidate definition, preload it, then switch routing; preserve old routing on load failure and drain old generation after success. Test concurrent requests and repeated failed reloads. |
| 5 | Cloud contract and stored-spec compatibility | Reject every explicit unsupported CLI/REST field, including REST `lazy_load:false`; preserve old Cloud spec read/start/ref, mark ignored legacy values in inspect, and cover identity collision/compatibility. |
| 6 | Cross-surface closeout | Align CLI help, REST/API contract, Local/Cluster/Cloud user docs and inspect examples. Run focused Rust/Python suites, formatting/checks, release-readiness checks, and a local model smoke test for eager load/release and lazy first request. Record evidence in this plan and the PR. |

Review steps are implementation checkpoints, not new GitHub issues or separate
branches. Move a checkpoint's own affected docs and tests with its code;
Step 6 closes remaining cross-surface consistency and smoke evidence.

## Verification Matrix

| Scenario | Expected evidence |
| --- | --- |
| Local lazy and eager, new and stored specs | First request triggers lazy load; eager start proves load before ready; eager load failure fails start. |
| Diffusers image target | New CLI/REST eager requests and old eager spec starts fail before worker launch. Explicit lazy succeeds, starts without loading a pipeline, and loads only the requested workflow on first use. Non-image targets remain eligible for eager. |
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

- [x] Decide Diffusers image-generation load mode: D9 requires explicit lazy.
- [ ] Confirm whether D9 also applies to MLX/MFLUX image generation.
- [ ] Every accepted decision D1-D9 is implemented and verified.
- [ ] Each review step records its focused test result and remaining risk.
- [ ] All #132 issue acceptance criteria pass and user-facing docs match.
- [ ] PR review and merge are complete; #132 can then be closed.
